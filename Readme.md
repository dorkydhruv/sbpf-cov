hey man, how are you? I was messing with llvm coverage for solana sbpf programs recently and wanted to get your thoughts on a prototype I built.
https://github.com/dorkydhruv/sbpf-cov
since the sbpf vm has no filesystem to flush counters, I forked sbpf-linker to merge __llvm_prf_cnts into .rodata and patched SBPF LDDW
relocations so bytecode increments counters in .rodata directly during vm execution. after tests run, I read .rodata from memory and
convert it into a .profraw file.
it works, but I hit a couple of pain points:
1. Harnesses like LiteSVM and Mollusk enforce strict read-only .rodata (PF_R), so writing counters to .rodata throws access violations
unless segment permissions are patched.
2. Rust bpf targets (bpfel-unknown-none) omit AST `__llvm_covfun` mapping sections, so standard llvm-cov fails on Rust binaries. I had to
use gimli to parse dwarf .debug_line tables to get line coverage.
also wanted to get your take on 2 questions:
1. Is there a cleaner zero-runtime way to store counter dumps in sbpf without needing to make .rodata mutable across harnesses?
2. Given LLVM optimizations like inlining and loop unrolling, do you think source line coverage is enough for solana programs, or should
coverage be more instruction-oriented?