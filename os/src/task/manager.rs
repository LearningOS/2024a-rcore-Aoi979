//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

/// big stride
pub const BIG_STRIDE: isize = 888888;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let mut min_index: usize = 0;
        let mut min_task: Option<Arc<TaskControlBlock>> = None;
            for (index,task) in self.ready_queue.iter().enumerate() {
                if let Some(min) = &min_task{
                    if task.inner.exclusive_access().stride < min.inner_exclusive_access().stride {
                        min_task =Some(task.clone());
                        min_index = index;
                    }
                }else { 
                    min_task = Some(task.clone());
                    min_index = index;
                }
            }
        let target = min_task.unwrap();
        let pass = target.inner_exclusive_access().pass;
        target.inner_exclusive_access().stride += pass;
        self.ready_queue.remove(min_index);
        Some(target)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
