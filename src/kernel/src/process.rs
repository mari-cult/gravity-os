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

    let syscall_num = if r12 != 0 { r12 } else { r7 };

    if syscall_num != 4 {
        // Don't spam write

        kprintln!(
            "A32 Syscall: num={} R0={:x} R1={:x} R2={:x} R3={:x} PC={:x}",
            syscall_num,
            frame.x[0],
            frame.x[1],
            frame.x[2],
            frame.x[3],
            frame.elr
        );
    }

    if syscall_num < 0 || syscall_num as u32 == 0x80000000 {
        // Mach trap

        match syscall_num {
            -26 => {
                // mach_reply_port

                let mut sched = SCHEDULER.lock();

                if let Some(space) = sched.current_ipc_space() {
                    frame.x[0] = space.allocate_port() as u64;
                } else {
                    frame.x[0] = 0;
                }
            }

            -27 => {
                // thread_self_trap

                frame.x[0] = 0x200; // Dummy thread port
            }

            -28 => {
                // host_self_trap

                frame.x[0] = 0x300; // Dummy host port
            }

            -31 => {
                // mach_msg_trap(msg, option, send_size, rcv_size, rcv_name, timeout, notify)

                let msg = frame.x[0] as *mut crate::ipc::MachMsgHeader;

                let option = frame.x[1] as u32;

                let send_size = frame.x[2] as u32;

                let rcv_size = frame.x[3] as u32;

                let rcv_name = frame.x[4] as u32;

                let timeout = frame.x[5] as u32;

                let mut sched = SCHEDULER.lock();

                if let Some(space) = sched.current_ipc_space() {
                    frame.x[0] = crate::ipc::mach_msg(
                        msg, option, send_size, rcv_size, rcv_name, timeout, 0, space,
                    ) as u64;
                } else {
                    frame.x[0] = 0x10000003 as u64; // MACH_PORT_UNKNOWN
                }
            }

            -3 => {
                // mach_absolute_time

                static mut TIME: u64 = 0;

                unsafe {
                    TIME += 100;

                    frame.x[0] = (TIME & 0xFFFFFFFF) as u64;

                    frame.x[1] = (TIME >> 32) as u64;
                }
            }

            _ => {
                if syscall_num as u32 == 0x80000000 {
                    frame.x[0] = 0;
                } else {
                    kprintln!("Unknown Mach A32 trap: {}", syscall_num);
                }
            }
        }

        return;
    }

    match syscall_num {
        1 => sys_exit(),

        3 => {
            // read(fd, buf, len)

            let fd = frame.x[0] as usize;

            let buf_ptr = frame.x[1] as *mut u8;

            let len = frame.x[2] as usize;

            let mut sched = SCHEDULER.lock();

            let mut read_len = 0;

            if let Some(proc) = sched.current_process.as_mut() {
                if fd < proc.files.len() {
                    if let Some(handle) = &mut proc.files[fd] {
                        let slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

                        read_len = handle.read(slice);
                    }
                }
            }

            frame.x[0] = read_len as u64;

            frame.spsr &= !0x20000000;
        }

        4 => sys_write(frame.x[0], frame.x[1], frame.x[2]),

        5 => {
            // open(path, flags, mode)

            let path_ptr = frame.x[0] as *const u8;

            let mut path_buf = [0u8; 128];

            let mut i = 0;

            while i < 127 {
                let c = unsafe { core::ptr::read(path_ptr.add(i)) };

                if c == 0 {
                    break;
                }

                path_buf[i] = c;

                i += 1;
            }

            let path_str = core::str::from_utf8(&path_buf[..i]).unwrap_or("invalid");

            kprintln!("sys_open: {}", path_str);

            if let Some(handle) = crate::vfs::open(path_str) {
                let mut sched = SCHEDULER.lock();

                if let Some(proc) = sched.current_process.as_mut() {
                    let mut found_fd = None;

                    for (fd, slot) in proc.files.iter_mut().enumerate() {
                        if slot.is_none() {
                            *slot = Some(handle);

                            found_fd = Some(fd);

                            break;
                        }
                    }

                    if let Some(fd) = found_fd {
                        frame.x[0] = fd as u64;

                        frame.spsr &= !0x20000000;
                    } else {
                        frame.x[0] = 24; // EMFILE

                        frame.spsr |= 0x20000000;
                    }
                }
            } else {
                frame.x[0] = 2; // ENOENT

                frame.spsr |= 0x20000000;
            }
        }

        6 => {
            // close(fd)

            let fd = frame.x[0] as usize;

            let mut sched = SCHEDULER.lock();

            if let Some(proc) = sched.current_process.as_mut() {
                if fd < proc.files.len() {
                    proc.files[fd] = None;
                }
            }

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        20 => {
            // getpid

            frame.x[0] = SCHEDULER.lock().current_pid();

            frame.spsr &= !0x20000000;
        }

        24 => {
            frame.x[0] = 0;
            frame.spsr &= !0x20000000;
        } // getuid

        25 => {
            frame.x[0] = 0;
            frame.spsr &= !0x20000000;
        } // geteuid

        26 => {
            frame.x[0] = 0;
            frame.spsr &= !0x20000000;
        } // getgid

        33 => {
            // access(path, mode)

            frame.x[0] = 2; // ENOENT

            frame.spsr |= 0x20000000;
        }

        43 => {
            frame.x[0] = 0;
            frame.spsr &= !0x20000000;
        } // getegid

        46 => {
            // sigaction

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        48 => {
            // sigprocmask

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        54 => {
            // ioctl

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        73 => {
            // munmap(addr, len)

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        74 => {
            // mprotect(addr, len, prot)

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        116 => {
            // gettimeofday

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        197 => {
            // mmap(addr, len, prot, flags, fd, offset)

            let addr = frame.x[0];

            let len = frame.x[1];

            let fd = frame.x[4] as i32;

            let _offset = frame.x[5];

            let map_addr = if addr == 0 {
                static mut NEXT_MMAP: u64 = 0x70000000;

                unsafe {
                    let res = NEXT_MMAP;

                    NEXT_MMAP += (len + 0xFFF) & !0xFFF;

                    res
                }
            } else {
                addr
            };

            crate::mmu::map_range(map_addr, map_addr, len, crate::mmu::MapPermission::UserRWX);

            if fd != -1 {
                let mut sched = SCHEDULER.lock();

                if let Some(proc) = sched.current_process.as_mut() {
                    let fd = fd as usize;

                    if fd < proc.files.len() {
                        if let Some(handle) = &mut proc.files[fd] {
                            let slice = unsafe {
                                core::slice::from_raw_parts_mut(map_addr as *mut u8, len as usize)
                            };

                            handle.read(slice);
                        }
                    }
                }
            }

            frame.x[0] = map_addr;

            frame.spsr &= !0x20000000;
        }

        202 => {
            // sysctl

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        220 => {
            // getattrlist

            frame.x[0] = 2; // ENOENT

            frame.spsr |= 0x20000000;
        }

        327 => {
            // issetugid

            frame.x[0] = 0;

            frame.spsr &= !0x20000000;
        }

        338 => {
            // stat64

            frame.x[0] = 2; // ENOENT

            frame.spsr |= 0x20000000;
        }

        339 => {
            // fstat64

            frame.x[0] = 2; // ENOENT

            frame.spsr |= 0x20000000;
        }

        340 => {
            // lstat64

            frame.x[0] = 2; // ENOENT

            frame.spsr |= 0x20000000;
        }

        _ => {
            kprintln!(
                "Unknown A32 syscall: num={} R0={:x} PC={:x}",
                syscall_num,
                frame.x[0],
                frame.elr
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
