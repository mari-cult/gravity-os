#![no_std]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn hello_from_lib() {
    let msg = "Hello from Dynamic Library!\n";
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "svc #0",
            in("x8") 2u64, // sys_write
            in("x0") 1u64, // stdout
            in("x1") msg.as_ptr() as u64,
            in("x2") msg.len() as u64,
        );
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
