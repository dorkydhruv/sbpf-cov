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

TMP_DIR="${TMPDIR:-/tmp}"
TRACE_FILE="${TMP_DIR%/}/c_trace.json"

# 1. Compile C source → LLVM bitcode (for sbpf-linker) and → ELF .o with DWARF (for coverage)
echo -e "\n[1/3] Compiling C source with Upstream Clang & DWARF debug info..."
"$CLANG" -target bpfel-unknown-none -O0 -fno-inline -g -emit-llvm -c "${SCRIPT_DIR}/src/program.c" -o "${OUT_DIR}/c_program.bc"
"$CLANG" -target bpfel-unknown-none -mcpu=v1 -O0 -g -c "${SCRIPT_DIR}/src/program.c" -o "${OUT_DIR}/c_program.o"

# Link SBPF ELF shared library for Mollusk SVM loader (no --btf; v0 --btf produces truncated ELF)
DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/opt/llvm/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}" cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-linker -- --arch v0 --export entrypoint -o "${OUT_DIR}/c_program.so" "${OUT_DIR}/c_program.bc"

# 2. Execute Mollusk VM test suite
echo -e "\n[2/3] Executing Mollusk SBPF VM test suite..."
cargo test --manifest-path "${SCRIPT_DIR}/Cargo.toml" -- --nocapture

# 3. Generate DWARF & Trace Source Coverage Reports
echo -e "\n[3/3] Generating PC Trace & DWARF Source Coverage Reports..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" -p sbpf-cov -- \
    --elf "${OUT_DIR}/c_program.o" \
    --trace "${TRACE_FILE}" \
    --html "${OUT_DIR}/coverage_html" \
    --lcov "${OUT_DIR}/lcov.info"

echo -e "\n✅ C Solana SBPF Coverage Workflow Complete!"
