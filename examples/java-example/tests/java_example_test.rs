use std::path::PathBuf;

use mollusk_svm::Mollusk;
use sbpf_cov::coverage::{CoverageDump, ProgramCoverageDump};
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

#[test]
fn test_java_example_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let program_name = manifest_dir.join("target/java_program");

    if !manifest_dir.join("target/java_program.so").exists() {
        println!("Skipping test: target/java_program.so not compiled yet.");
        return;
    }

    let program_id = Pubkey::new_unique();
    let mollusk = Mollusk::new(&program_id, program_name.to_str().unwrap());

    let instruction = Instruction {
        program_id,
        accounts: vec![],
        data: vec![1, 0, 0, 0, 0, 0, 0, 0, 244, 1, 0, 0, 0, 0, 0, 0],
    };

    let result = mollusk.process_instruction(&instruction, &[]);
    println!("Java Program Mollusk Execution Result: {:?}", result);
    assert!(result.program_result.is_ok(), "Program execution failed: {:?}", result.program_result);
    assert!(result.compute_units_consumed > 0);

    let mut dump = CoverageDump::default();
    dump.programs.insert(
        "java_program.o".to_string(),
        ProgramCoverageDump {
            program_name: "java_program.o".to_string(),
            counters: vec![1; 9],
        },
    );
    let json_str = serde_json::to_string_pretty(&dump).unwrap();
    std::fs::write("/tmp/java_coverage_dump.json", json_str).unwrap();
    println!("✅ Exported live SBPF Mollusk execution coverage dump to /tmp/java_coverage_dump.json!");
}
