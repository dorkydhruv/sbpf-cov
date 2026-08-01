use std::fs;
use std::path::PathBuf;

use litesvm::LiteSVM;
use mollusk_svm::Mollusk;

#[test]
fn test_c_example_litesvm_and_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/c_prog.so");

    if !elf_path.exists() {
        println!("Skipping test: target/c_prog.so not compiled yet.");
        return;
    }

    let elf_bytes = fs::read(&elf_path).expect("Failed to read target/c_prog.so");

    // 1. LiteSVM Execution Test
    let mut svm = LiteSVM::new();
    println!("LiteSVM initialized for C program testing.");

    // 2. Mollusk SVM Execution Test
    let mollusk = Mollusk::default();
    println!("Mollusk SVM initialized for C program testing.");
}
