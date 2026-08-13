# Architectural Comparison & SBPF Coverage Technical Report

**Location**: `report/architectural_diff_review.md`  
**Baseline**: Upstream `sbpf-linker` commit [`b6230ae`](https://github.com/solana-labs/sbpf-linker/commit/b6230ae)  
**Target State**: Current `sbpf-cov` Working Tree

---

## 1. Codebase Changes & File Reference Map

Since forking from [`b6230ae`](https://github.com/solana-labs/sbpf-linker/commit/b6230ae), the repository has been upgraded into a complete **zero-runtime LLVM source coverage toolchain (`sbpf-cov`)**.

| Component | Key Files | Functionality & Code Anchors |
|:---|:---|:---|
| **Zero-Runtime LDDW Fixup** | [src/coverage/fixup.rs](file:///Users/dhruv/dev/sbpf-cov/src/coverage/fixup.rs#L204-L223) | Mutates 64-bit counter load immediates (`LDDW`, registers `r1`–`r9`) in `.text` from `.rodata` (`0x0`) to writable memory (`0x20001f000+`). |
| **LLVM `.profraw` v10 Synthesizer** | [src/coverage/convert.rs](file:///Users/dhruv/dev/sbpf-cov/src/coverage/convert.rs#L40-L120) | Converts raw JSON counter dumps to standard LLVM v10 binary profiles for `llvm-cov` / `llvm-profdata`. |
| **DWARF Line Mapper & Filter** | [src/coverage/dwarf_cov.rs](file:///Users/dhruv/dev/sbpf-cov/src/coverage/dwarf_cov.rs#L20-L100) | Uses `gimli` to extract `.debug_line` tables and filters out toolchain paths (`.rustup/`, `/rustc/`, `/library/`). |
| **Report Orchestration** | [src/coverage/report.rs](file:///Users/dhruv/dev/sbpf-cov/src/coverage/report.rs#L154-L210) | Generates native `llvm-cov` reports for C/Java and falls back to DWARF parsing for Rust objects. |
| **VM Execution Interposer** | [interposer/src/lib.rs](file:///Users/dhruv/dev/sbpf-cov/interposer/src/lib.rs#L26-L113) | `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES` dylib that flushes counters at process exit when execution occurs. |
| **Linker Upgrades** | [src/bin/sbpf-linker.rs](file:///Users/dhruv/dev/sbpf-cov/src/bin/sbpf-linker.rs#L120-L160) | Merges `__llvm_prf_cnts` sections into SBPF ELF `.rodata` and patches `ei_osabi` header flags. |
| **Atomic Opcode Byteparser** | [src/byteparser.rs](file:///Users/dhruv/dev/sbpf-cov/src/byteparser.rs#L50-L110) | Disassembles SBPF bytecode and maps `0xdb` (STXD atomic store) -> `0x7b` for SBPF v0 compatibility. |

---

## 2. Stack Memory vs. Heap Memory Clarification

> **User Question**: *"I thought we were using heap memory for storing counters right?"*

Currently, **we are storing counters in Stack memory, NOT Heap memory**.

In [src/coverage/fixup.rs](file:///Users/dhruv/dev/sbpf-cov/src/coverage/fixup.rs#L212):

```rust
let target_vaddr: u64 = 0x20001f000 + (counter_idx * 8);
```

### SBPF Virtual Memory Map:
- `0x000000000` — **`.rodata`** (Read-Only)
- `0x100000000` — **`.text`** (Executable Code)
- `0x200000000` — **Stack** (Writable, 64 frames × 4 KB = 256 KB max)
- `0x300000000` — **Heap** (Writable, 32 KB default, initialized by `sol_alloc_free_`)
- `0x400000000` — **Input Buffer** (Instruction payload & account vector)

`0x20001f000` resides at offset **124 KB** inside the **Stack** region (`0x200000000`).

### Why This Needs to Change:
- **Stack Collision Risk**: Placing counters at `0x20001f000` places them in **Stack Frame 31**. Small example programs (call depth 2–4) work fine. But real-world Anchor smart contracts with deep CPI call stacks (> 31 frames) will overwrite counter memory, causing data corruption or VM Access Violation panics.
- **Recommended Upgrade**: Shift counter virtual address base to **Input Buffer End** (`0x400010000+`) or **Heap Region** (`0x300000000+`).

---

## 3. How `solana_rbpf` / LiteSVM Enforces Read-Only Memory Regions

> **User Question**: *"In LiteSVM, I don't find any rbpf svm initialization where .rodata, .bss or any other are marked as read only, can you help me find that?"*

LiteSVM (and Mollusk SVM) do not explicitly initialize read-only flags in their own codebase because memory permissions are enforced **underneath by `solana_rbpf`**.

### How `solana_rbpf` Enforces Memory Permissions:
1. When LiteSVM loads an ELF binary, it delegates ELF loading to `solana_rbpf::elf::Executable::load()`.
2. `solana_rbpf` parses ELF section headers:
   - Sections marked `.rodata` or non-writable in program headers are added to `solana_rbpf::memory_region::MemoryRegion` as **read-only**.
   - Virtual address range `0x000000000..0x100000000` is assigned to `.rodata` with `read_only = true`.
3. During VM instruction execution (`solana_rbpf::vm::EbpfVm::execute_program()`), when an SBPF store instruction (`STXW`, `STB`, `STH`, `STXD`) attempts to write to a virtual address, `solana_rbpf` calls `MemoryMapping::map(AccessType::Store, vaddr, size)`:
   - If `vaddr` maps to `.rodata` (`< 0x100000000`), `solana_rbpf` raises `EbpfError::AccessViolation(AccessType::Store, vaddr, size)`.
4. Stack (`0x200000000`) and Heap (`0x300000000`) regions are passed into `MemoryRegion` with `read_only = false`, allowing store instructions to succeed without error.

---

## 4. Instruction-Based Coverage vs. Line-Based Coverage via DWARF

> **User Question**: *"Instead of line coverage, in a production program you care about instruction coverage. Can we use DWARF / gimli for per-instruction coverage reports?"*

### Line Coverage vs. Instruction / Bytecode Coverage

| Metric | Focus | Limitation / Advantage |
|:---|:---|:---|
| **Line Coverage** | Source lines hit in high-level language (Rust/C/Java) | A single source line can compile into 20 SBPF instructions and multiple branches. Line coverage obscures partial branch execution. |
| **Instruction Coverage** | Exact SBPF assembly instructions (PC addresses) executed in `.text` | Essential for smart contract security audits, verifying zero untested bytecode branches, and measuring exact Compute Unit (CU) consumption. |

### How DWARF + `byteparser.rs` Delivers Superior Instruction Coverage:

DWARF `.debug_line` tables contain a matrix mapping every **SBPF Program Counter (PC)** address in `.text` to its source line and column:

$$\text{DWARF Row: } \text{PC } 0x100000048 \longrightarrow (\text{src/lib.rs}, \text{Line } 42, \text{Col } 12)$$

By combining `gimli` DWARF parsing with our existing bytecode parser ([src/byteparser.rs](file:///Users/dhruv/dev/sbpf-cov/src/byteparser.rs)), `sbpf-cov` can generate **Instruction Coverage Reports**:

1. **Per-Instruction Execution Heatmap**:
   - List every SBPF opcode (`LDDW`, `ADD64`, `JEQ`, `STXD`) by PC address.
   - Mark exact execution count per instruction ($0$ vs $>0$).
2. **Branch & Basic-Block Coverage**:
   - Track conditional jumps (`JEQ`, `JNE`, `JGT`, `JGE`) at the SBPF assembly level to guarantee both true/false paths were tested.
3. **Source-Linked Instruction Breakdown**:
   - Render HTML reports where expanding a source line reveals the underlying SBPF instructions and their execution status.
