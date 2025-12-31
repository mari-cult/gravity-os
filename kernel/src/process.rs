use crate::kprintln;
use crate::scheduler::SCHEDULER;
use core::arch::asm;

#[repr(C)]
#[derive(Debug, Default)]
pub struct TrapFrame {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct CpuContext {
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
    pub sp: u64,
}

#[no_mangle]
pub extern "C" fn handle_sync_exception(frame: &mut TrapFrame) {
    let esr: u64;
    unsafe { asm!("mrs {}, esr_el1", out(reg) esr) };

    let ec = esr >> 26;

    // EC 0x15 = SVC instruction
    if ec == 0x15 {
        handle_syscall(frame);
    } else {
        kprintln!("Unknown exception! ESR: {:x} EC: {:x}", esr, ec);
        loop {
            unsafe { asm!("wfe") }
        }
    }
}

fn handle_syscall(frame: &mut TrapFrame) {
    let syscall_num = frame.x8;

    match syscall_num {
        0 => sys_yield(),
        1 => sys_exit(),
        2 => sys_write(frame.x0, frame.x1),
        3 => frame.x0 = sys_spawn(frame.x0, frame.x1),
        4 => frame.x0 = sys_getpid(),
        _ => {
            kprintln!("Unknown syscall: {}", syscall_num);
        }
    }
}

fn sys_yield() {
    unsafe {
        extern "C" {
            fn __switch_to(prev: *mut CpuContext, next: *const CpuContext);
        }

        let pointers = {
            let mut scheduler = SCHEDULER.lock();
            scheduler.schedule_next()
        };

        if let Some((prev, next)) = pointers {
            __switch_to(prev, next);
        }
    }
}

fn sys_exit() {
    kprintln!("Process Exiting");
    loop {
        unsafe { asm!("wfe") }
    }
}

fn sys_write(ptr: u64, len: u64) {
    if ptr == 0 {
        return;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    if let Ok(s) = core::str::from_utf8(slice) {
        crate::uart::print_str(s);
    } else {
        kprintln!("sys_write: invalid utf8");
    }
}

fn sys_spawn(fn_ptr: u64, arg: u64) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    let process = crate::scheduler::Process::new(fn_ptr, arg);
    let pid = process.pid;
    scheduler.add_process(process);
    pid
}

fn sys_getpid() -> u64 {
    let scheduler = SCHEDULER.lock();
    if let Some(p) = &scheduler.current_process {
        p.pid
    } else {
        0
    }
}

pub fn init_vectors() {
    extern "C" {
        static vectors: u8;
    }
    unsafe {
        asm!("msr vbar_el1, {}", in(reg) &vectors);
    }
}
