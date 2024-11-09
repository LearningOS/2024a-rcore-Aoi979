//! Process management syscalls
//!
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use crate::{
    config::MAX_SYSCALL_NUM,
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus,
    },
};
use crate::config::{PAGE_SIZE, TRAP_CONTEXT_BASE};
use crate::fs::{Stdin, Stdout};
use crate::mm::{MapPermission, MemorySet, PageTable, PhysAddr, StepByOne, VirtAddr, KERNEL_SPACE};
use crate::mm::address::VPNRange;
use crate::sync::UPSafeCell;
use crate::task::{get_current_task_running_time, get_syscall_times, kstack_alloc, new_map_area, pid_alloc, unmap_area, vpn2pte_curr_task, TaskContext, TaskControlBlock, TaskControlBlockInner, BIG_STRIDE};
use crate::timer::get_time_us;
use crate::trap::{trap_handler, TrapContext};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let va = VirtAddr(_ts as usize);
    let token = current_user_token();
    if let Some(pa) = v2p_addr(va, token) {
        let temp = pa.0 as *mut TimeVal;
        unsafe {
            *temp = TimeVal{
                sec: us / 1_000_000,
                usec: us % 1_000_000,
            };
        }
        0
    }else {
        -1
    }
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let va = VirtAddr(_ti as usize);
    let token = current_user_token();
    if let Some(pa) = v2p_addr(va, token) {
        let temp = pa.0 as *mut TaskInfo;
        unsafe {
            (*temp).status = TaskStatus::Running;
            (*temp).syscall_times = get_syscall_times();
            (*temp).time = get_current_task_running_time();
        }
        0
    }else {
        -1
    }
}
pub fn v2p_addr(virt_addr: VirtAddr, token: usize) -> Option<PhysAddr> {
    let offset = virt_addr.page_offset();
    let vpn = virt_addr.floor();
    let ppn = crate::mm::page_table::PageTable::from_token(token).translate(vpn).map(|p|p.ppn());
    if let Some(ppn) = ppn {
        Some(PhysAddr::from_ppn_and_offset(ppn, offset))
    }else {
        None
    }
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    if _start % PAGE_SIZE != 0 || _port & !0x7 != 0 || _port & 0x7 == 0 || _start >= usize::MAX {
        return -1;
    }
    let vpns = VPNRange::new(
        VirtAddr::from(_start).floor(),
        VirtAddr::from(_start+_len).ceil(),
    );
    for v in vpns{
        if let Some(pte) = vpn2pte_curr_task(v){
            if pte.is_valid(){
                return -1;
            }
        }
    }
    new_map_area(
        VirtAddr::from(_start).floor().into(),
        VirtAddr::from(_start+_len).ceil().into(),
        MapPermission::from_bits_truncate((_port<<1) as u8) | MapPermission::U
    );
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start >= usize::MAX || _start % PAGE_SIZE != 0 {
        return -1;
    }
    let mlen = _len.min(usize::MAX - _start);
    unmap_area(_start,mlen)

}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    let task = current_task().unwrap();
    let mut parent_inner = task.inner_exclusive_access();
    let token = parent_inner.memory_set.token();
    let _path = translated_str(token,_path);
    if let Some(inode) = open_file(_path.as_str(), OpenFlags::RDONLY) {
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(inode.read_all().as_slice());
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(&task)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    task_syscall_times: [0; MAX_SYSCALL_NUM],
                    running_time: 0,
                    stride: 0,
                    pass: BIG_STRIDE / 16,
                    priority: 16,
                })
            },
        });
        parent_inner.children.push(task_control_block.clone());
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        let pid = task_control_block.pid.0 as isize;
        add_task(task_control_block);
        pid
    }else {
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    if _prio >= 2{
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();
        inner.priority = _prio;
        inner.pass = BIG_STRIDE / _prio;
        _prio
    } else {
        -1
    }
}
