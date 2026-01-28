use crate::kprintln;
use crate::scheduler::SCHEDULER;
use core::arch::asm;

#[repr(C)]
#[derive(Debug, Default)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub __reserved: u64, // for alignment
    pub elr: u64,
    pub spsr: u64,
    pub sp_el0: u64,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct CpuContext {
    pub regs: [u64; 13], // x19..x28, x29, sp, x30
}

static mut EXCEPTION_COUNT: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn handle_sync_exception(frame: &mut TrapFrame) {
    unsafe {
        EXCEPTION_COUNT += 1;
        if EXCEPTION_COUNT > 10 {
            // Probably a loop in exception handler
            loop {
                asm!("wfe")
            }
        }
    }

    let esr: u64;
    let far: u64;
    let sp_el0: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, far_el1", out(reg) far);
        asm!("mrs {}, sp_el0", out(reg) sp_el0);
    }
    frame.sp_el0 = sp_el0;

    let ec = esr >> 26;
    let iss = esr & 0x1FFFFFF;

    match ec {
        0x15 => {
            // EC 0x15 = SVC instruction in AArch64
            handle_a64_syscall(frame);
        }
        _ => {
            kprintln!(
                "Unknown exception! ESR: {:x} EC: {:x} ISS: {:x} FAR: {:x} PC: {:x} SPSR: {:x}",
                esr,
                ec,
                iss,
                far,
                frame.elr,
                frame.spsr
            );
            kprintln!(
                "regs: x0={:x} x1={:x} x2={:x} x3={:x}",
                frame.x[0],
                frame.x[1],
                frame.x[2],
                frame.x[3]
            );
            kprintln!(
                "      x4={:x} x5={:x} x6={:x} x7={:x}",
                frame.x[4],
                frame.x[5],
                frame.x[6],
                frame.x[7]
            );
            kprintln!(
                "      x8={:x} x9={:x} x10={:x} x11={:x}",
                frame.x[8],
                frame.x[9],
                frame.x[10],
                frame.x[11]
            );

            let sp_el0: u64;
            unsafe { asm!("mrs {}, sp_el0", out(reg) sp_el0) };

            kprintln!(
                "      x12={:x} sp_el0={:x} lr={:x}",
                frame.x[12],
                sp_el0,
                frame.elr
            );

            // Dump code around PC (backwards and forwards)
            kprintln!("Code at PC:");
            dump_mem((frame.elr & !0xF).saturating_sub(32), 64);

            // Dump memory around FAR if it was a data abort
            if ec == 0x24 || ec == 0x25 {
                kprintln!("Data at FAR:");
                dump_mem(far & !0xF, 32);
            }

            loop {
                unsafe { asm!("wfe") }
            }
        }
    }
}

fn dump_mem(addr: u64, len: u64) {
    for i in (0..len).step_by(16) {
        let curr = addr + i;
        // Basic safety check for known mapped user/kernel regions
        // QEMU virt RAM: 0x40000000 - 0x80000000
        // Peripheral/IO: 0x00000000 - 0x10000000 (roughly)
        let is_ram = (0x40000000..0x80000000).contains(&curr);
        let is_io = (0x09000000..0x09001000).contains(&curr); // UART

        if !is_ram && !is_io {
            continue;
        }

        unsafe {
            let p = curr as *const u32;
            kprintln!(
                "{:08x}: {:08x} {:08x} {:08x} {:08x}",
                curr,
                core::ptr::read_volatile(p.offset(0)),
                core::ptr::read_volatile(p.offset(1)),
                core::ptr::read_volatile(p.offset(2)),
                core::ptr::read_volatile(p.offset(3))
            );
        }
    }
}

fn handle_a64_syscall(frame: &mut TrapFrame) {
    let syscall_num = frame.x[8];

    match syscall_num {
        0 => sys_yield(),
        1 => sys_exit(),
        2 => sys_write(frame.x[0], frame.x[1], frame.x[2]),
        3 => frame.x[0] = sys_spawn(frame.x[0], frame.x[1]),
        4 => frame.x[0] = sys_getpid(),
        _ => {
            kprintln!("Unknown A64 syscall: {}", syscall_num);
        }
    }
}

fn sys_write(fd: u64, buf: u64, len: u64) {
    if fd == 1 || fd == 2 || fd == 4 {
        let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, len as usize) };
        if let Ok(s) = core::str::from_utf8(slice) {
            kprintln!("sys_write: {}", s);
        }
    }
}

fn sys_yield() {
    unsafe {
        unsafe extern "C" {
            fn __switch_to(prev: *mut CpuContext, next: *const CpuContext);
        }

        let pointers = {
            let mut scheduler = SCHEDULER.lock();
            scheduler.schedule_next()
        };

        if let Some((prev, next)) = pointers {
            __switch_to(prev.expect("No prev context in sys_yield"), next);
        }
    }
}

fn sys_exit() {
    kprintln!("Process Exiting");
    loop {
        unsafe { asm!("wfe") }
    }
}

fn sys_spawn(fn_ptr: u64, arg: u64) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    // For now, kernel-spawned threads in EL0
    let process = crate::scheduler::Process::new(fn_ptr, 0, &[arg], 0, true);
    let pid = process.pid;
    scheduler.add_process(process);
    pid
}

fn sys_getpid() -> u64 {
    1
}

pub fn init_vectors() {
    unsafe extern "C" {

        static vectors: u8;

    }

    unsafe {
        asm!("msr vbar_el1, {}", in(reg) &vectors);
    }
}
