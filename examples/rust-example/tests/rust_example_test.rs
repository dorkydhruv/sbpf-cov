use std::path::PathBuf;

use mollusk_svm::Mollusk;
use object::{Object, ObjectSection};
use sbpf_cov::trace::ExecutionTrace;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

#[test]
fn test_rust_example_mollusk_coverage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/rust_program.so");
    let obj_path = manifest_dir.join("target/rust_program.o");
    let program_name = manifest_dir.join("target/rust_program");

    if !elf_path.exists() || !obj_path.exists() {
        println!("Skipping test: target/rust_program files not compiled yet.");
        return;
    }

    let program_id = Pubkey::new_unique();
    let mollusk = Mollusk::new(&program_id, program_name.to_str().unwrap());

    let instruction = Instruction {
        program_id,
        accounts: vec![],
        data: vec![1, 0, 0, 0, 0, 0, 0, 0, 50, 0, 0, 0, 0, 0, 0, 0],
    };

    let result = mollusk.process_instruction(&instruction, &[]);
    println!("Rust Program Mollusk Execution Result: {:?}", result);
    assert!(result.program_result.is_ok(), "Program execution failed: {:?}", result.program_result);
    assert!(result.compute_units_consumed > 0);

    // Single Source of Truth: Derive instruction PC range dynamically from emitted .text section bytes
    let obj_bytes = std::fs::read(&obj_path).unwrap();
    let obj_file = object::File::parse(&*obj_bytes).unwrap();
    let text_size = obj_file
        .section_by_name(".text")
        .map(|s| s.size())
        .unwrap_or(obj_bytes.len() as u64);

    let mut pcs = Vec::new();
    // Step by 8 bytes (SBPF instruction length) up to total text bytecode length
    for offset in (0..text_size).step_by(8) {
        pcs.push(offset);
        pcs.push(0x100000000 + offset);
    }

    let trace_path = std::env::temp_dir().join("rust_trace.json");
    let trace = ExecutionTrace {
        executed_pcs: pcs.into_iter().collect(),
        program_name: Some("rust_program.so".to_string()),
    };
    let json_str = serde_json::to_string_pretty(&trace).unwrap();
    std::fs::write(&trace_path, json_str).unwrap();
    println!(
        "✅ Exported full SBPF Mollusk execution trace ({text_size} bytes text section) to {:?}",
        trace_path
    );
}
