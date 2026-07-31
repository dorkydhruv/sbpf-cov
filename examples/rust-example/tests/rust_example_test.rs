use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use sbpf_cov::coverage::CoverageTracker;
use solana_rbpf::elf::Executable;
use solana_rbpf::memory_region::{MemoryMapping, MemoryRegion};
use solana_rbpf::program::BuiltinProgram;
use solana_rbpf::vm::{EbpfVm, TestContextObject};

fn sol_log_stub(
    _vm: &mut EbpfVm<TestContextObject>,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> Result<u64, solana_rbpf::error::EbpfError> {
    Ok(0)
}

#[test]
fn test_rust_example_vm_execution_and_coverage() {
    let elf_path = match std::env::var("RUST_EXAMPLE_SO") {
        Ok(val) => PathBuf::from(val),
        Err(_) => {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let candidate1 =
                manifest_dir.join("target/deploy/rust_example.so");
            let candidate2 =
                manifest_dir.join("../target/deploy/rust_example.so");
            if candidate1.exists() {
                candidate1
            } else if candidate2.exists() {
                candidate2
            } else {
                println!(
                    "Skipping VM test: rust_example.so not compiled yet."
                );
                return;
            }
        }
    };

    println!("Loading ELF for VM test from: {:?}", elf_path);
    let elf_bytes =
        fs::read(&elf_path).expect("Failed to read rust_example.so");
    println!(
        "ELF bytes length: {}, e_entry = {:#x}",
        elf_bytes.len(),
        u64::from_le_bytes(elf_bytes[24..32].try_into().unwrap())
    );
    let loader = Arc::new(BuiltinProgram::new_mock());
    let mut executable = match Executable::<TestContextObject>::from_elf(
        &elf_bytes,
        loader.clone(),
    ) {
        Ok(exe) => exe,
        Err(e) => {
            let e_entry =
                u64::from_le_bytes(elf_bytes[24..32].try_into().unwrap());
            println!(
                "Rust SBPF Load error: {:?}, e_entry in file = {:#x}",
                e, e_entry
            );
            panic!("Failed to load ELF in solana_rbpf: {:?}", e);
        }
    };
    executable
        .verify::<solana_rbpf::verifier::RequisiteVerifier>()
        .expect("Failed to verify ELF");

    let mut ro_data = executable.get_ro_section().to_vec();
    if ro_data.len() < 8192 {
        ro_data.resize(8192, 0);
    }
    let ro_region = executable.get_ro_region();

    // Execute instruction in solana_rbpf VM
    let mut context = TestContextObject::new(1_000_000);
    let sbpf_version = executable.get_sbpf_version();

    let mut stack = vec![0u8; executable.get_config().stack_size()];
    let mut heap = Vec::new();
    let mut input = vec![0u8; 128];
    // num_accounts = 0
    input[0..8].copy_from_slice(&(0u64).to_le_bytes());
    // instruction_data_len = 9
    input[8..16].copy_from_slice(&(9u64).to_le_bytes());
    // instruction opcode 1 (Deposit)
    input[16] = 1;
    // deposit amount 500
    input[17..25].copy_from_slice(&(500u64).to_le_bytes());

    let regions = vec![
        MemoryRegion::new_writable(&mut ro_data, ro_region.vm_addr),
        MemoryRegion::new_writable(
            &mut stack,
            solana_rbpf::ebpf::MM_STACK_START,
        ),
        MemoryRegion::new_writable(
            &mut heap,
            solana_rbpf::ebpf::MM_HEAP_START,
        ),
        MemoryRegion::new_writable(
            &mut input,
            solana_rbpf::ebpf::MM_INPUT_START,
        ),
    ];

    let memory_mapping =
        MemoryMapping::new(regions, executable.get_config(), sbpf_version)
            .unwrap();
    let mut vm = EbpfVm::new(
        loader.clone(),
        &sbpf_version,
        &mut context,
        memory_mapping,
        stack.len(),
    );
    vm.registers[1] = solana_rbpf::ebpf::MM_INPUT_START;

    let (_insn_count, result) = vm.execute_program(&executable, true);
    println!("Rust SBPF VM Execution Result: {:?}", result);

    for (i, chunk) in ro_data.chunks(8).enumerate() {
        if chunk.len() == 8 {
            let val = u64::from_le_bytes(chunk.try_into().unwrap());
            if val != 0 {
                println!(
                    "ro_data[offset {:#x} / index {}] = {}",
                    i * 8,
                    i,
                    val
                );
            }
        }
    }

    // Automatic counter extraction from ELF + rodata
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rust_elf_obj = manifest_dir.join("target/rust_program.o");
    let mut tracker = CoverageTracker::new();
    let ro_counters = &ro_data[4032..];
    tracker
        .sync_from_elf_and_rodata(
            "rust_program.o",
            &rust_elf_obj,
            ro_counters,
            None,
        )
        .expect("Failed to auto-detect counter count from ELF");
    tracker
        .export_json(std::path::Path::new("/tmp/rust_coverage_dump.json"))
        .expect("Failed to export JSON dump");
}
