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

TARGET="bpfel-unknown-none"

# 1. LLVM Build (Emits LLVM IR & object with upstream rustc and Clang)
echo -e "\n[1/3] Emitting LLVM IR and object file from Rust source with upstream rustc & Clang..."
cargo +nightly rustc --manifest-path "${SCRIPT_DIR}/Cargo.toml" --target "$TARGET" -Z build-std=core -- -C instrument-coverage -C opt-level=2 -C panic=abort -C target-feature=-alu32,-v3,-v4 -g --emit=llvm-ir || true

RUST_LL="$(find "${ROOT_DIR}/target" "${SCRIPT_DIR}/target" -name "rust_example*.ll" 2>/dev/null | grep "/bpfel-unknown-none/" | head -n 1 || true)"
if [ -n "$RUST_LL" ] && [ -f "$RUST_LL" ]; then
    sed -i '' 's/captures(none)//g' "$RUST_LL"
    "$CLANG" -target "$TARGET" -mcpu=v1 -g -O1 -Xclang -fprofile-instrument=llvm -Xclang -fcoverage-mapping -c "$RUST_LL" -o "${OUT_DIR}/rust_program.o"
    "$CLANG" -target "$TARGET" -mcpu=v1 -g -O1 -Xclang -fprofile-instrument=llvm -Xclang -fcoverage-mapping -emit-llvm -c "$RUST_LL" -o "${OUT_DIR}/rust_program.bc"
else
    echo "Error: Could not locate generated rust_example LLVM IR file"
    exit 1
fi

# 2. Link via sbpf-linker
echo -e "\n[2/3] Linking SBPF binary via sbpf-linker..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-linker -- --output "${OUT_DIR}/rust_example_raw.so" "${OUT_DIR}/rust_program.bc"

# 3. One-Shot Coverage Generation via sbpf-cov (fixup, VM test execution under interposer, convert, and HTML report)
echo -e "\n[3/3] Running one-shot sbpf-cov pipeline..."
cargo run --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sbpf-cov -- \
    --raw-elf "${OUT_DIR}/rust_example_raw.so" \
    --fixed-elf "${OUT_DIR}/rust_example.so" \
    --elf "${OUT_DIR}/rust_program.o" \
    --manifest-path "${SCRIPT_DIR}/Cargo.toml" \
    --dump /tmp/rust_coverage_dump.json \
    --profraw "${OUT_DIR}/rust_example.profraw" \
    --profdata "${OUT_DIR}/rust_example.profdata" \
    --html "${OUT_DIR}/coverage_html"

echo -e "\n✅ Rust Solana SBPF Coverage Workflow Complete!"
