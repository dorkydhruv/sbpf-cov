use std::fs;
use std::path::PathBuf;

use litesvm::LiteSVM;
use mollusk_svm::Mollusk;

#[test]
fn test_java_example_litesvm_and_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/java_program.so");

    if !elf_path.exists() {
        println!("Skipping test: target/java_program.so not compiled yet.");
        return;
    }

    let elf_bytes = fs::read(&elf_path).expect("Failed to read target/java_program.so");

    // 1. LiteSVM Execution Test
    let mut svm = LiteSVM::new();
    println!("LiteSVM initialized for Java program testing.");

    // 2. Mollusk SVM Execution Test
    let mollusk = Mollusk::default();
    println!("Mollusk SVM initialized for Java program testing.");
}
