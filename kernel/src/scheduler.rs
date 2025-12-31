use crate::process::{CpuContext, TrapFrame};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

static PID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ProcessState {
    Ready,
    Running,
    Dead,
}

pub struct Process {
    pub pid: u64,
    pub state: ProcessState,
    pub context: CpuContext,
    pub stack: Vec<u8>,
}

impl Process {
    pub fn new(entry_point: u64, arg: u64) -> Self {
        let mut stack = Vec::with_capacity(4096 * 4); // 16KB stack
        unsafe { stack.set_len(4096 * 4) };

        let sp = stack.as_ptr() as u64 + stack.len() as u64;

        let mut user_stack: Vec<u8> = Vec::with_capacity(4096 * 4);
        unsafe { user_stack.set_len(4096 * 4) };
        let user_sp = user_stack.as_ptr() as u64 + user_stack.len() as u64;
        core::mem::forget(user_stack);

        let mut context = CpuContext::default();
        context.sp = sp;

        extern "C" {
            fn kernel_thread_starter();
        }
        context.x30 = kernel_thread_starter as *const () as u64;

        context.x19 = entry_point;
        context.x20 = user_sp;
        context.x21 = arg;

        Self {
            pid: PID_COUNTER.fetch_add(1, Ordering::Relaxed),
            state: ProcessState::Ready,
            context,
            stack,
        }
    }
}

pub struct Scheduler {
    pub processes: VecDeque<Box<Process>>,
    pub current_process: Option<Box<Process>>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            processes: VecDeque::new(),
            current_process: None,
        }
    }

    pub fn add_process(&mut self, process: Process) {
        self.processes.push_back(Box::new(process));
    }

    // Returns (ptr_to_prev_ctx, ptr_to_next_ctx)
    // Box<Process> ensures memory location of Process struct is stable on heap.
    pub fn schedule_next(&mut self) -> Option<(*mut CpuContext, *const CpuContext)> {
        if let Some(next_proc) = self.processes.pop_front() {
            // We have a next process.

            // If there is a current process, put it back in queue.
            if let Some(mut prev) = self.current_process.take() {
                prev.state = ProcessState::Ready;
                self.processes.push_back(prev);
            }

            // Promote next to current
            self.current_process = Some(next_proc);

            // Now we need pointers.

            let next_ctx_ptr = &self.current_process.as_ref().unwrap().context as *const CpuContext;

            // Prev address? It is now at the BACK of the queue.
            let prev_ctx_ptr = &mut self.processes.back_mut().unwrap().context as *mut CpuContext;

            return Some((prev_ctx_ptr, next_ctx_ptr));
        } else {
            // No ready process. Keep running current.
            return None;
        }
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
