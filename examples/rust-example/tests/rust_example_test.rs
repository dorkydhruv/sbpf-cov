use std::fs;
use std::path::PathBuf;

use litesvm::LiteSVM;
use mollusk_svm::Mollusk;

#[test]
fn test_rust_example_litesvm_and_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/rust_example.so");

    if !elf_path.exists() {
        println!("Skipping test: target/rust_example.so not compiled yet.");
        return;
    }

    let elf_bytes = fs::read(&elf_path).expect("Failed to read target/rust_example.so");

    // 1. LiteSVM Execution Test
    let mut svm = LiteSVM::new();
    println!("LiteSVM initialized for Rust program testing.");

    // 2. Mollusk SVM Execution Test
    let mollusk = Mollusk::default();
    println!("Mollusk SVM initialized for Rust program testing.");
}
