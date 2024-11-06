//! Process management syscalls

use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus,
    },
};
use crate::config::PAGE_SIZE;
use crate::mm::{PhysAddr, VirtAddr, MapPermission};
use crate::mm::address::{VPNRange};
use crate::task::{current_user_token, get_current_task_running_time, get_syscall_times, new_map_area, unmap_area, vpn2pte_curr_task};
use crate::timer::{ get_time_us};
use crate::mm::page_table::{ PageTable};
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

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let va = VirtAddr(_ts as usize);
    if let Some(pa) = v2p_addr(va) {
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
    if let Some(pa) = v2p_addr(va) {
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







// YOUR JOB: Implement mmap.
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

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if _start >= usize::MAX || _start % PAGE_SIZE != 0 {
        return -1;
    }
    let mlen = _len.min(usize::MAX - _start);
    unmap_area(_start,mlen)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
fn v2p_addr(virt_addr: VirtAddr) -> Option<PhysAddr> {
    let offset = virt_addr.page_offset();
    let vpn = virt_addr.floor();
    let ppn = PageTable::from_token(current_user_token()).translate(vpn).map(|p|p.ppn());
    if let Some(ppn) = ppn {
        Some(PhysAddr::from_ppn_and_offset(ppn, offset))
    }else {
        None
    }
}
