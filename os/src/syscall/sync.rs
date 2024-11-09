use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let dead_lock_switch = process_inner.enable_deadlock_detect;
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        if dead_lock_switch {
            process_inner.available_mutex.push(1);
            for v in process_inner.allocation_mutex.iter_mut() {
                v.push(0);
            }
            for v in process_inner.need_mutex.iter_mut() {
                v.push(0);
            }
        }
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        if dead_lock_switch {
            process_inner.available_mutex.push(1);
            for v in process_inner.allocation_mutex.iter_mut() {
                v.push(0);
            }
            for v in process_inner.need_mutex.iter_mut() {
                v.push(0);
            }
        }
        process_inner.mutex_list.len() as isize - 1
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let dead_lock_switch = process_inner.enable_deadlock_detect;
    if dead_lock_switch {
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        process_inner.need_mutex[tid][mutex_id] += 1;
        let thread_num = process_inner.tasks.len();
        let mut work = process_inner.available_mutex[mutex_id];
        ///finish
        let mut is_safe = vec![false; thread_num];
        for i in 0..thread_num {
            if process_inner.allocation_mutex[i][mutex_id] == 0 {
                is_safe[i] == true;
            }
        }
        let mut flag = true;
        while flag {
            flag = false;
            for i in 0..thread_num {
                if process_inner.need_mutex[i][mutex_id] <= work && !is_safe[i]{
                   work += process_inner.allocation_mutex[i][mutex_id];
                    is_safe[i] = true;
                    flag = true;
                }
            }
        }
        if is_safe.iter().find(|&&x| !x).is_some() {
            return -0xdead;
        }
        process_inner.allocation_mutex[tid][mutex_id] += 1;
        process_inner.available_mutex[mutex_id] -= 1;
        process_inner.need_mutex[tid][mutex_id] -= 1;
    }
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let dead_lock_switch = process_inner.enable_deadlock_detect;
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        if dead_lock_switch {
            process_inner.available_sem.push(res_count);
            let thread_num = process_inner.tasks.len();
            for v in 0..thread_num {
                process_inner.allocation_sem[v].push(0);
                process_inner.need_sem[v].push(res_count);
            }
        }
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        if dead_lock_switch {
            let thread_num = process_inner.tasks.len();
            process_inner.available_sem.push(res_count);
            for v in 0..thread_num {
                process_inner.allocation_sem[v].push(0);
                process_inner.need_sem[v].push(0);
            }
        }
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let dead_lock_switch = process_inner.enable_deadlock_detect;
    if dead_lock_switch {
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;

            process_inner.allocation_sem[tid][sem_id] -= 1;
            process_inner.available_sem[sem_id] += 1;

    }
    drop(process_inner);
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let dead_lock_switch = process_inner.enable_deadlock_detect;
    if dead_lock_switch {
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        let thread_num = process_inner.tasks.len();
        let mut work = process_inner.available_sem[sem_id];
        let mut is_safe = vec![false; thread_num];
        let mut flag = true;
        while flag {
            flag = false;
            for i in 0..thread_num {
                if !is_safe[i]&& process_inner.need_sem[i][sem_id] <= work {
                    work += process_inner.allocation_sem[i][sem_id];
                    is_safe[i] = true;
                    flag = true;
                }
            }
        }
        if is_safe.iter().find(|&&x| !x).is_some() {
            return -0xdead;
        }
            process_inner.allocation_sem[tid][sem_id] += 1;
            process_inner.available_sem[sem_id] -= 1;
            process_inner.need_sem[tid][sem_id] -= 1;


    }
    drop(process_inner);
    sem.down();
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    match _enabled {
        1 => {
            current_process().inner_exclusive_access().enable_deadlock_detect = true;
        }
        0 => {
            current_process().inner_exclusive_access().enable_deadlock_detect = false;
        }
        _ => {
            return -1;
        }
    }
    0
}
