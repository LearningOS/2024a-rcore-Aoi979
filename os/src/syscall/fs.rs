//! File and filesystem-related syscalls
use crate::fs::{open_file,  OpenFlags, Stat, StatMode, ROOT_INODE};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer, VirtAddr};
use crate::syscall::process::v2p_addr;
use crate::task::{current_task, current_user_token, };

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    let binding = current_task().unwrap();
    let token = current_user_token();
    let task_inner = binding.inner_exclusive_access();
    let va = VirtAddr(_st as usize);
    if let Some(file) = &task_inner.fd_table[_fd] {
        if let Some(pa) = v2p_addr(va,token) {
            let ino = file.get_inode_id() as u64;
            let link_num = file.get_link_num() as u32;
            let temp = pa.0 as *mut Stat;
            unsafe {
                (*temp).dev = 0;
                (*temp).ino = ino;
                (*temp).mode = StatMode::FILE;
                (*temp).nlink = link_num;
                (*temp).pad = [0;7];
            }
           return  0;
        }else {
            return -1;
        }
    }else {
        return -1;
    }
}


/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    let token = current_user_token();
    let _old_name = translated_str(token, _old_name);
    let _new_name = translated_str(token, _new_name);
    if _old_name == _new_name {
        return -1;
    }
    if let Some(_) = ROOT_INODE.link(_old_name.as_str(), _new_name.as_str()) {
        return 0;
    }
    -1
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    let token = current_user_token();
    let _name = translated_str(token, _name);
    if let Some(inode) = ROOT_INODE.find(_name.as_str()) {
        if ROOT_INODE.is_last(&inode) {
            inode.clear();
        }
        return ROOT_INODE.unlink(_name.as_str());
    }
    -1
}