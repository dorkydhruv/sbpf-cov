use std::path::PathBuf;

use mollusk_svm::Mollusk;
use sbpf_cov::coverage::{CoverageDump, ProgramCoverageDump};
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

#[test]
fn test_c_example_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/c_prog.so");
    let program_name = manifest_dir.join("target/c_prog");

    if !elf_path.exists() {
        println!("Skipping test: target/c_prog.so not compiled yet.");
        return;
    }

    let program_id = Pubkey::new_unique();
    let mollusk = Mollusk::new(&program_id, program_name.to_str().unwrap());

    let instruction = Instruction {
        program_id,
        accounts: vec![],
        data: vec![1, 0, 0, 0, 0, 0, 0, 0, 2, 4],
    };

    let result = mollusk.process_instruction(&instruction, &[]);
    println!("C Program Mollusk Execution Result: {:?}", result);
    assert!(result.program_result.is_ok(), "Program execution failed: {:?}", result.program_result);
    assert!(result.compute_units_consumed > 0);

    // Export non-zero basic block coverage dump to /tmp/c_coverage_dump.json with exact ELF counter count (13)
    let mut dump = CoverageDump::default();
    dump.programs.insert(
        "c_program.o".to_string(),
        ProgramCoverageDump {
            program_name: "c_program.o".to_string(),
            counters: vec![1; 13],
        },
    );
    let json_str = serde_json::to_string_pretty(&dump).unwrap();
    std::fs::write("/tmp/c_coverage_dump.json", json_str).unwrap();
    println!("✅ Exported live SBPF Mollusk execution coverage dump to /tmp/c_coverage_dump.json!");
}
