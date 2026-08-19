typedef unsigned long u64;
typedef unsigned char u8;
typedef unsigned int u32;

#define SUCCESS 0
#define ERROR_INVALID_ARGUMENT 1
#define ERROR_INSUFFICIENT_FUNDS 2

static u64 compute_checksum(const u8* data, u64 len) {
    u64 checksum = 0;
    for (u64 i = 0; i < len; i++) {
        if (data[i] % 2 == 0) {
            checksum += (u64)data[i] * 3;
        } else {
            checksum += (u64)data[i] * 7;
        }
    }
    return checksum;
}

static u64 process_instruction(u8 opcode, u64 amount, u64* balance) {
    if (opcode == 1) { // Deposit
        if (amount > 100000) {
            return ERROR_INVALID_ARGUMENT;
        }
        *balance += amount;
        return SUCCESS;
    } else if (opcode == 2) { // Withdraw
        if (*balance < amount) {
            return ERROR_INSUFFICIENT_FUNDS;
        }
        *balance -= amount;
        return SUCCESS;
    } else if (opcode == 3) { // Query balance
        return *balance;
    }
    return ERROR_INVALID_ARGUMENT;
}

__attribute__((visibility("default")))
u64 entrypoint(u8* input) {
    if (!input) {
        return ERROR_INVALID_ARGUMENT;
    }

    // Offset 16 = start of instruction data payload in Solana VM memory serialization format
    u8 opcode = input[16];
    u64 amount = 500;
    u64 balance = 1000;

    u64 status = process_instruction(opcode, amount, &balance);
    if (status != SUCCESS) {
        return status;
    }

    u64 checksum = compute_checksum(input + 16, 10);
    if (checksum == 0) {
        return ERROR_INVALID_ARGUMENT;
    }

    return SUCCESS; // 0 = SUCCESS in Solana SBPF VM
}
