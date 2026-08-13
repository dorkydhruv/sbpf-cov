#![cfg_attr(target_arch = "bpf", no_std)]

#[cfg(target_arch = "bpf")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub unsafe extern "C" fn sol_log_(_message: *const u8, _len: u64) {}

fn sol_log(msg: &str) {
    unsafe {
        sol_log_(msg.as_ptr(), msg.len() as u64);
    }
}

#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
    sol_log("Executing Rust SBPF program with coverage instrumentation...");

    if input.is_null() {
        return 1;
    }

    // Return SUCCESS (0) unconditionally to verify coverage tracking
    sol_log("Rust SBPF instruction executed successfully!");
    0
}
