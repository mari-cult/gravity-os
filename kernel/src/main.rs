#![no_std]
#![no_main]

// Enable alloc crate
extern crate alloc;

mod elf;
mod heap;
mod process;
mod scheduler;
mod uart;

use crate::scheduler::{Process, SCHEDULER};
use crate::uart::print_str;
use core::arch::asm;
use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(include_str!("boot.s"));
global_asm!(include_str!("vectors.s"));
global_asm!(include_str!("switch.s"));

static USER_PROGRAM: &[u8] =
    include_bytes!("../../target/aarch64-unknown-none-softfloat/release/user");

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    kprintln!("Hello from GravityOS. Spawning processes...");

    heap::init_heap();
    process::init_vectors();
    kprintln!("Vectors initialized");

    let entry_point = elf::load_elf(USER_PROGRAM).expect("Failed to load ELF");
    kprintln!("ELF loaded. Entry: {:x}", entry_point);

    {
        let mut sched = SCHEDULER.lock();
        // Pass 0 as argument to initial processes
        sched.add_process(Process::new(entry_point, 0));
        kprintln!("Added Process 1");
        sched.add_process(Process::new(entry_point, 0));
        kprintln!("Added Process 2");

        sched.schedule_next();
        kprintln!("First process scheduled as Current");
    }

    let mut boot_ctx = process::CpuContext::default();
    unsafe {
        // Scope to drop lock!
        let next_ctx_ptr = {
            let sched = SCHEDULER.lock();
            &sched.current_process.as_ref().unwrap().context as *const _
        };

        kprintln!("Switching to Process 1...");
        extern "C" {
            fn __switch_to(prev: *mut process::CpuContext, next: *const process::CpuContext);
        }
        __switch_to(&mut boot_ctx, next_ctx_ptr);
    }

    kprintln!(
        "Returned to kmain loop? This shouldn't happen for the first switch unless it returns."
    );

    loop {
        unsafe { asm!("wfe") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("Kernel Panic: {:?}", info);
    loop {
        unsafe { asm!("wfe") };
    }
}
