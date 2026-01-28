use crate::ipc::IpcSpace;
use crate::kprintln;
use crate::process::CpuContext;
use crate::vfs::FileHandle;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec;
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
    pub ipc_space: IpcSpace,
    pub files: Vec<Option<FileHandle>>,
}

impl Process {
    pub fn new(
        entry_point: u64,
        user_sp: u64,
        args: &[u64],
        tls_base: u64,
        is_64bit: bool,
    ) -> Self {
        let stack_size = 64 * 1024;
        let stack = vec![0u8; stack_size];
        let sp = (stack.as_ptr() as u64 + stack.len() as u64) & !15;

        kprintln!(
            "Creating process: KStack top {:x}, UStack top {:x}",
            sp,
            user_sp
        );

        let mut context = CpuContext::default();
        context.regs[12] = sp; // sp

        unsafe extern "C" {
            fn kernel_thread_starter();
        }
        context.regs[11] = kernel_thread_starter as *const () as u64; // x30/lr

        context.regs[0] = entry_point; // x19
        context.regs[1] = user_sp; // x20
        context.regs[9] = tls_base; // x28 -> TLS
        kprintln!(
            "Process::new: entry={:x}, user_sp={:x} -> regs[0]={:x}, regs[1]={:x}",
            entry_point,
            user_sp,
            context.regs[0],
            context.regs[1]
        );

        // Pass up to 6 args in x21..x26
        context.regs[2..(args.len().min(6) + 2)].copy_from_slice(&args[..args.len().min(6)]);

        // SPSR: Mask all DAIF bits (0x3c0)
        // If 64-bit: EL0t (0x000)
        // If 32-bit: User mode (0x010)
        let mut spsr = 0x3c0u64;
        if !is_64bit {
            spsr |= 0x10;
        }
        context.regs[8] = spsr; // x27

        Self {
            pid: PID_COUNTER.fetch_add(1, Ordering::Relaxed),
            state: ProcessState::Ready,
            context,
            stack,
            ipc_space: IpcSpace::new(),
            files: (0..32).map(|_| None).collect(),
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
    pub fn schedule_next(&mut self) -> Option<(Option<*mut CpuContext>, *const CpuContext)> {
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
            let prev_ctx_ptr = self
                .processes
                .back_mut()
                .map(|p| &mut p.context as *mut CpuContext);

            Some((prev_ctx_ptr, next_ctx_ptr))
        } else {
            // No ready process. Keep running current.
            None
        }
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
