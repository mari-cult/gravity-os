#![no_std]
#![no_main]

use core::panic::PanicInfo;

extern "C" {
    fn hello_from_lib();
}

#[no_mangle]
pub extern "C" fn main() {
    print("Hello from Dynamic App!\n");
    unsafe {
        hello_from_lib();
    }
    print("Goodbye from Dynamic App!\n");

    #[cfg(target_arch = "aarch64")]
    loop {
        unsafe {
            core::arch::asm!("svc #0", in("x8") 0u64);
        }
    }
}

fn print(s: &str) {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "svc #0",
            in("x8") 2u64, // sys_write
            in("x0") 1u64, // stdout
            in("x1") s.as_ptr() as u64,
            in("x2") s.len() as u64,
        );
    }
}

#[no_mangle]
pub extern "C" fn dyld_stub_binder() {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
