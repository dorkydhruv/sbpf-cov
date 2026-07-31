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

#[cfg(target_arch = "bpf")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8) -> u64 {
    sol_log("Executing Rust SBPF program with coverage instrumentation...");

    if input.is_null() {
        return 1;
    }

    let num_accounts = *(input as *const u64) as usize;
    let mut offset = 8;

    for _ in 0..num_accounts {
        let dup_info = (*(input.add(offset) as *const u64)) as u8;
        offset += 1;
        if dup_info == u8::MAX {
            offset += 3; // is_signer, is_writable, executable
            offset += 4; // padding
            offset += 32; // pubkey
            offset += 32; // owner
            offset += 8; // lamports
            let data_len = *(input.add(offset) as *const u64) as usize;
            offset += 8 + data_len + 10000 + 8;
        } else {
            offset += 7;
        }
    }

    let instruction_data_len = *(input.add(offset) as *const u64) as usize;
    offset += 8;

    if instruction_data_len == 0 {
        sol_log("Error: Invalid instruction data (empty)");
        return 1;
    }

    let op = (*(input.add(offset) as *const u64)) as u8;
    match op {
        1 => {
            sol_log("Instruction 1: Deposit action");
            if instruction_data_len < 9 {
                sol_log("Error: Instruction 1 payload too short");
                return 2;
            }
            let amount = *(input.add(offset + 1) as *const u64);
            if amount > 100_000 {
                sol_log("Error: Deposit amount exceeds limit");
                return 3;
            }
            0
        }
        2 => {
            sol_log("Instruction 2: Withdraw action");
            if instruction_data_len < 9 {
                sol_log("Error: Instruction 2 payload too short");
                return 2;
            }
            let amount = *(input.add(offset + 1) as *const u64);
            if amount > 50_000 {
                sol_log("Error: Insufficient funds for withdrawal");
                return 4;
            }
            0
        }
        3 => {
            sol_log("Instruction 3: Reset action");
            0
        }
        _ => {
            sol_log("Error: Unknown instruction byte");
            1
        }
    }
}
