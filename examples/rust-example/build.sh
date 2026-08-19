#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

echo "======================================================="
echo "   Rust Solana Program SBPF Coverage Pipeline Example  "
echo "======================================================="

export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/opt/llvm/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}"

CLANG="${CLANG:-/opt/homebrew/opt/llvm/bin/clang}"
if [ ! -f "$CLANG" ]; then
    CLANG="$(which clang)"
fi

OUT_DIR="${SCRIPT_DIR}/target"
mkdir -p "${OUT_DIR}"
rm -rf "${ROOT_DIR}/target/bpfel-unknown-none"

TMP_DIR="${TMPDIR:-/tmp}"
TRACE_FILE="${TMP_DIR%/}/rust_trace.json"

TARGET="bpfel-unknown-none"

# 1. Compiles Rust source to LLVM IR & ELF object with DWARF debug info
echo -e "\n[1/3] Emitting LLVM IR & SBPF ELF object from Rust source with DWARF debug info..."
cargo +nightly rustc --manifest-path "${SCRIPT_DIR}/Cargo.toml" --target "$TARGET" -Z build-std=core -- -C debuginfo=2 -C opt-level=0 -C panic=abort -C target-cpu=generic -C linker=true --emit=llvm-ir

RUST_LL="$(find "${ROOT_DIR}/target" "${SCRIPT_DIR}/target" -name "rust_example*.ll" 2>/dev/null | grep "/bpfel-unknown-none/" | head -n 1 || true)"
if [ -n "$RUST_LL" ] && [ -f "$RUST_LL" ]; then
    "$CLANG" -target "$TARGET" -O0 -emit-llvm -c "$RUST_LL" -o "${OUT_DIR}/rust_program.bc"
    "$CLANG" -target "$TARGET" -mcpu=v1 -O0 -g -c "$RUST_LL" -o "${OUT_DIR}/rust_program.o"
    DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/opt/llvm/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}" cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-linker -- --arch v0 --export entrypoint -o "${OUT_DIR}/rust_program.so" "${OUT_DIR}/rust_program.bc"
else
    echo "Error: Could not locate generated rust_example LLVM IR file"
    exit 1
fi

# 2. Execute Mollusk VM test suite
echo -e "\n[2/3] Executing Mollusk SBPF VM test suite..."
cargo test --manifest-path "${SCRIPT_DIR}/Cargo.toml" -- --nocapture

# 3. Generate DWARF & Trace Source Coverage Reports
echo -e "\n[3/3] Generating PC Trace & DWARF Source Coverage Reports..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" -p sbpf-cov -- \
    --elf "${OUT_DIR}/rust_program.o" \
    --trace "${TRACE_FILE}" \
    --html "${OUT_DIR}/coverage_html" \
    --lcov "${OUT_DIR}/lcov.info"

echo -e "\n✅ Rust Solana SBPF Coverage Workflow Complete!"
