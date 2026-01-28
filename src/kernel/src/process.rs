use crate::kprintln;
use crate::scheduler::SCHEDULER;
use core::arch::asm;

#[repr(C)]
#[derive(Debug, Default)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub __padding: u64,
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
        0x11 => {
            // EC 0x11 = SVC instruction in AArch32
            handle_a32_syscall(frame);
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
            for i in (0..31).step_by(4) {
                kprintln!(
                    "x{:02}={:016x} x{:02}={:016x} x{:02}={:016x} x{:02}={:016x}",
                    i,
                    frame.x[i],
                    i + 1,
                    if i + 1 < 31 { frame.x[i + 1] } else { 0 },
                    i + 2,
                    if i + 2 < 31 { frame.x[i + 2] } else { 0 },
                    i + 3,
                    if i + 3 < 31 { frame.x[i + 3] } else { 0 }
                );
            }

            let sp_el0: u64;
            unsafe { asm!("mrs {}, sp_el0", out(reg) sp_el0) };

            kprintln!("sp_el0={:016x}", sp_el0);

            // Dump code around PC (more bytes)
            kprintln!("Code at PC:");
            dump_mem((frame.elr & !0xF).saturating_sub(128), 256);

            // Dump stack
            kprintln!("Stack at SP:");
            dump_mem(sp_el0 & !0xF, 128);

            // Dump memory around FAR if it was a data abort
            if ec == 0x24 || ec == 0x25 {
                kprintln!("Data at FAR:");
                dump_mem(far & !0xF, 64);
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

fn handle_a32_syscall(frame: &mut TrapFrame) {
    let r12 = frame.x[12] as i32;
    let r7 = frame.x[7] as i32;

    kprintln!(
        "A32 Syscall: R12={} R7={} R0={:x} PC={:x}",
        r12,
        r7,
        frame.x[0],
        frame.elr
    );

    if r12 < 0 || r12 == -2147483648 {
        // Mach trap
        match r12 {
            -26 => {
                // mach_msg_trap(msg, option, send_size, rcv_size, rcv_name, timeout, notify)
                let msg = frame.x[0] as *mut crate::ipc::MachMsgHeader;
                let option = frame.x[1] as u32;
                let send_size = frame.x[2] as u32;
                let rcv_size = frame.x[3] as u32;
                let rcv_name = frame.x[4] as u32;
                let timeout = frame.x[5] as u32;
                // notify is on stack? For now ignore.

                let mut sched = SCHEDULER.lock();
                if let Some(space) = sched.current_ipc_space() {
                    frame.x[0] = crate::ipc::mach_msg(
                        msg, option, send_size, rcv_size, rcv_name, timeout, 0, space,
                    ) as u64;
                } else {
                    frame.x[0] = 0x10000003 as u64; // MACH_PORT_UNKNOWN
                }
            }
            -28 => {
                // mach_reply_port
                let mut sched = SCHEDULER.lock();
                if let Some(space) = sched.current_ipc_space() {
                    frame.x[0] = space.allocate_port() as u64;
                } else {
                    frame.x[0] = 0;
                }
            }
            -3 => {
                // mach_absolute_time
                // Simple incrementing counter for now
                static mut TIME: u64 = 0;
                unsafe {
                    TIME += 100;
                    // A32 returns 64-bit in R0:R1
                    frame.x[0] = (TIME & 0xFFFFFFFF) as u64;
                    frame.x[1] = (TIME >> 32) as u64;
                }
            }
            _ => {
                if r12 == -2147483648 {
                    // This is 0x80000000. Might be some special private trap or error.
                    // Return success for now to see if it unblocks.
                    frame.x[0] = 0;
                } else {
                    kprintln!("Unknown Mach A32 trap: {}", r12);
                }
            }
        }
        return;
    }

    // BSD Syscall
    let syscall_num = if r12 != 0 { r12 } else { r7 };

    match syscall_num {
        1 => sys_exit(),
        4 => sys_write(frame.x[0], frame.x[1], frame.x[2]),
        5 => {
            // open(path, flags, mode)
            let path_ptr = frame.x[0] as *const u8;
            // For now, return ENOENT (2)
            frame.x[0] = (-2i32) as u64;
            // Darwin returns carry bit set for error?
            frame.spsr |= 0x20000000; // Carry flag in CPSR (bit 29)
        }
        20 => {
            // getpid
            frame.x[0] = SCHEDULER.lock().current_pid();
        }
        116 => {
            // gettimeofday
            frame.x[0] = 0; // Success
        }
        327 => {
            // issetugid
            frame.x[0] = 0;
        }
        _ => {
            kprintln!(
                "Unknown A32 syscall: R12={} R7={} R0={}",
                r12,
                r7,
                frame.x[0]
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
