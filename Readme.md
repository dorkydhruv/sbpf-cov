# `sbpf-cov` 🎯
> Zero-Runtime LLVM Source Coverage Toolchain for Solana SBPF Programs

`sbpf-cov` provides native, zero-runtime-overhead LLVM source coverage (`llvm-cov` / `.profraw` / HTML reports) for Solana SBPF (Solana Bytecode Format) smart contracts written in **Rust**, **C**, or **Javalana / JavaCPP**.

---

## 🌟 Key Features

- **Zero-Runtime Overhead**: Mutates LLVM execution counters directly in `.rodata` during SBPF VM execution with zero additional syscalls or runtime libraries.
- **Native Linker Integration**: `sbpf-linker` merges `__llvm_prf_cnts` into SBPF `.rodata` and automatically patches ELF OSABI flags (`ELFOSABI_NONE`).
- **One-Command CLI Experience**: `cargo sbpf-cov` runs ELF fixup, test execution under dynamic interposer, dump conversion, and interactive HTML report rendering in a single command.
- **DWARF Line-Mapping Engine**: Built-in `gimli`-based DWARF `.debug_line` parser extracts exact source line tables for Rust SBPF programs.
- **Multi-Language Examples**: Includes complete, working end-to-end coverage examples for **C**, **Rust**, and **Javalana / JavaCPP** Solana programs.
- **Standard LLVM Formats**: Converts raw counter dumps to Version 10 `.profraw` binary profiles, compatible with `llvm-profdata`, `llvm-cov`, and LCOV / Codecov.

---

## 🚀 Quickstart

### 1. Installation

Build `sbpf-cov` and `sbpf-linker` binaries:

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### 2. One-Command Coverage (`cargo sbpf-cov`)

Run your test suite with full coverage instrumentation and generate interactive HTML and LCOV reports:

```bash
cargo sbpf-cov \
    --raw-elf target/prog_raw.so \
    --fixed-elf target/prog.so \
    --elf target/program.o \
    --html target/coverage_html
```

Open `./target/coverage_html/index.html` in your browser to view line-by-line covered vs missed lines and branches!

---

## 📁 Examples & Reference Pipelines

| Example Directory | Language | Pipeline Script | Description |
| :--- | :--- | :--- | :--- |
| [`examples/c-example`](examples/c-example) | **C** | [`build.sh`](examples/c-example/build.sh) | SBPF program compiled with Upstream Clang (`-fprofile-instr-generate -fcoverage-mapping`). |
| [`examples/rust-example`](examples/rust-example) | **Rust** | [`build.sh`](examples/rust-example/build.sh) | Native Rust SBPF program compiled with upstream `rustc` (`-C instrument-coverage`). |
| [`examples/java-example`](examples/java-example) | **Javalana / JavaCPP** | [`build.sh`](examples/java-example/build.sh) | Java smart contract compiled to SBPF bitcode via JavaCPP / Javalana bridge. |

To run any example and generate its HTML coverage report:

```bash
./examples/c-example/build.sh
./examples/rust-example/build.sh
./examples/java-example/build.sh
```

---

## 📐 Architecture Overview

```
                      +------------------------------------------+
                      | Rust / C / Javalana SBPF Source Code     |
                      +------------------------------------------+
                                           |
                               Clang / Rustc (-fprofile-instr-generate)
                                           v
                      +------------------------------------------+
                      | Intermediate Object (.o) & Bitcode (.bc) |
                      +------------------------------------------+
                                           |
                               sbpf-linker (Native Merging)
                                           v
                      +------------------------------------------+
                      | Raw SBPF ELF (*_raw.so)                  |
                      +------------------------------------------+
                                           |
                               sbpf-cov (One-Shot Pipeline)
                     [1/4] Fixup (.rodata + __llvm_prf_cnts)
                     [2/4] solana_rbpf Interposed Test Execution
                     [3/4] Synthesize Version 10 .profraw Profile
                     [4/4] Merge & Render HTML / LCOV Reports
                                           v
                +------------------------------------------------------+
                | Terminal Summary  |  Interactive HTML  |  LCOV File  |
                +------------------------------------------------------+
```

---

## 📄 License

MIT License. See [LICENSE](LICENSE) for details.
