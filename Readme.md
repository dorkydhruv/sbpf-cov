# `sbpf-cov` 🎯
> Zero-Runtime LLVM Source Coverage Toolchain for Solana SBPF Programs

`sbpf-cov` provides native, zero-runtime-overhead LLVM source coverage (`llvm-cov` / `.profraw` / HTML reports) for Solana SBPF (Solana Bytecode Format) smart contracts.

---

## 🌟 Key Features

- **Zero-Runtime Overhead**: Mutates LLVM execution counters directly in `.rodata` during SBPF VM execution with zero additional syscalls.
- **Native Linker Support**: `sbpf-linker` automatically merges `__llvm_prf_cnts` into SBPF `.rodata` and patches ELF OSABI (`ELFOSABI_NONE`).
- **Dynamic Interposer (`sbpf-cov-interposer`)**: Transparently hooks test execution under **LiteSVM**, **Mollusk**, **`solana-program-test`**, and standard `cargo test` / `cargo test-sbf`.
- **Standard LLVM Formats**: Converts raw counter dumps to Version 10 `.profraw` binary profiles, compatible with `llvm-profdata` and `llvm-cov`.
- **Comprehensive Reporting**: Supports terminal summaries, interactive line-by-line HTML reports, and LCOV export for **Codecov** and **GitHub Actions**.

---

## 🚀 Quickstart

### 1. Installation

Build `sbpf-cov` and `sbpf-linker` binaries:

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### 2. One-Command Coverage Suite (`sbpf-cov test`)

Run your test suite with full coverage instrumentation and generate interactive HTML and LCOV reports:

```bash
sbpf-cov test --html ./coverage_html --lcov ./lcov.info
```

Open `./coverage_html/index.html` in your browser to view line-by-line covered vs missed branches!

---

## 🛠️ CLI Subcommand Reference

### `sbpf-cov fixup`
Post-processes an instrumented SBPF ELF `.so` to embed coverage counters in `.rodata` and patch `ei_osabi` for `solana_rbpf` VM compatibility:

```bash
sbpf-cov fixup --input build/program.so --output build/program_vm.so
```

### `sbpf-cov convert`
Converts `coverage_dump.json` exported by the VM test harness into an LLVM `.profraw` file:

```bash
sbpf-cov convert --dump coverage_dump.json --elf build/program.so --output default.profraw
```

### `sbpf-cov report`
Merges `.profraw` into `.profdata` and generates coverage reports:

```bash
# Terminal summary
sbpf-cov report --profraw default.profraw --elf build/program.so

# Interactive HTML report
sbpf-cov report --profraw default.profraw --elf build/program.so --html ./coverage_html

# LCOV export (Codecov / CI)
sbpf-cov report --profraw default.profraw --elf build/program.so --lcov ./lcov.info
```

---

## 📐 Architecture Overview

```
                      +-----------------------------+
                      |   Rust SBPF Source Code     |
                      +-----------------------------+
                                     |
                         solana-rustc -C instrument-coverage
                                     v
                      +-----------------------------+
                      | Intermediate Object (.o)    |
                      +-----------------------------+
                                     |
                         sbpf-linker (Native Merging)
                                     v
                      +-----------------------------+
                      | Fixed SBPF ELF (.so)        |
                      | (.rodata + __llvm_prf_cnts) |
                      +-----------------------------+
                                     |
                       solana_rbpf VM Test Execution
                   (interposer hooks .rodata & dumps json)
                                     v
                      +-----------------------------+
                      |     coverage_dump.json      |
                      +-----------------------------+
                                     |
                              sbpf-cov convert
                                     v
                      +-----------------------------+
                      |       default.profraw       |
                      +-----------------------------+
                                     |
                               sbpf-cov report
                                     v
               +-------------------------------------------+
               | Terminal Summary | HTML View | LCOV File  |
               +-------------------------------------------+
```

---

## 📄 License

MIT License. See [LICENSE](LICENSE) for details.
