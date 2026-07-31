#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

echo "======================================================="
echo "   C Solana Program SBPF Coverage Pipeline Example     "
echo "======================================================="

export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/opt/llvm/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}"

CLANG="${CLANG:-/opt/homebrew/opt/llvm/bin/clang}"
if [ ! -f "$CLANG" ]; then
    CLANG="$(which clang)"
fi

OUT_DIR="${SCRIPT_DIR}/target"
mkdir -p "${OUT_DIR}"

# 1. LLVM Build (Compiles C source & LLVM bitcode)
echo -e "\n[1/3] Compiling C source to LLVM bitcode with Upstream Clang..."
"$CLANG" -target bpfel-unknown-none -O0 -fno-inline -fprofile-instr-generate -fcoverage-mapping -c "${SCRIPT_DIR}/src/program.c" -o "${OUT_DIR}/c_program.o"
"$CLANG" -target bpfel-unknown-none -O0 -fno-inline -fprofile-instr-generate -fcoverage-mapping -emit-llvm -c "${SCRIPT_DIR}/src/program.c" -o "${OUT_DIR}/c_program.bc"

# 2. Link via sbpf-linker
echo -e "\n[2/3] Linking SBPF binary via sbpf-linker..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-linker -- --arch v0 --export entrypoint -o "${OUT_DIR}/c_prog_raw.so" "${OUT_DIR}/c_program.bc"

# 3. One-Shot Coverage Generation via sbpf-cov (fixup, VM test execution under interposer, convert, and HTML report)
echo -e "\n[3/3] Running one-shot sbpf-cov pipeline..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-cov -- \
    --raw-elf "${OUT_DIR}/c_prog_raw.so" \
    --fixed-elf "${OUT_DIR}/c_prog.so" \
    --elf "${OUT_DIR}/c_program.o" \
    --manifest-path "${SCRIPT_DIR}/Cargo.toml" \
    --dump /tmp/c_coverage_dump.json \
    --profraw "${OUT_DIR}/c_example.profraw" \
    --profdata "${OUT_DIR}/c_example.profdata" \
    --html "${OUT_DIR}/coverage_html"

echo -e "\n✅ C Solana SBPF Coverage Workflow Complete!"
