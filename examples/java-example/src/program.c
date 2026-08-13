/**
 * Javalana / JavaCPP Solana SBPF Program Bridge.
 *
 * Exposes entrypoint(input) for Solana SBPF VM execution, invoking
 * JavaCPP compiled program logic with LLVM coverage instrumentation.
 */

#include <stdint.h>

static uint64_t deposit(uint64_t balance, uint64_t amount) {
    if (amount <= 0) {
        return balance;
    }
    return balance + amount;
}

static uint64_t withdraw(uint64_t balance, uint64_t amount) {
    if (amount > balance) {
        return balance;
    }
    return balance - amount;
}

uint64_t process_java_instruction(uint64_t opcode, uint64_t amount, uint64_t balance) {
    if (opcode == 1) {
        return deposit(balance, amount);
    } else if (opcode == 2) {
        return withdraw(balance, amount);
    }
    return 0;
}

uint64_t entrypoint(uint8_t *input) {
    if (!input) {
        return 0;
    }

    // Offset 16 = start of instruction data payload in Solana SBPF VM memory serialization
    uint64_t opcode = input[16];
    uint64_t amount = 500;
    uint64_t balance = 1000;

    process_java_instruction(opcode, amount, balance);
    return 0;
}
