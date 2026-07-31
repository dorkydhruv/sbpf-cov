/**
 * Javalana / JavaCPP Solana SBPF Smart Contract Example.
 *
 * Demonstrates a Java smart contract program compiled to LLVM bitcode and SBPF byte code
 * with full LLVM source coverage instrumentation.
 */
public class Program {

    public static long processInstruction(long opcode, long depositAmount, long currentBalance) {
        if (opcode == 1) {
            // Deposit operation
            return deposit(currentBalance, depositAmount);
        } else if (opcode == 2) {
            // Withdraw operation
            return withdraw(currentBalance, depositAmount);
        } else {
            // Unknown instruction
            return -1;
        }
    }

    private static long deposit(long balance, long amount) {
        if (amount <= 0) {
            return balance;
        }
        return balance + amount;
    }

    private static long withdraw(long balance, long amount) {
        if (amount > balance) {
            return balance;
        }
        return balance - amount;
    }
}
