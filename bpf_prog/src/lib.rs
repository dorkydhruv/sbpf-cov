#![no_std]
#![no_main]

#[no_mangle]
#[link_section = ".rodata"]
pub static mut COUNTER: u64 = 0;

#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
    let val = if input.is_null() { 0 } else { *input };

    if val > 10 {
        COUNTER = COUNTER.wrapping_add(10);
    } else {
        COUNTER = COUNTER.wrapping_add(1);
    }

    COUNTER
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
