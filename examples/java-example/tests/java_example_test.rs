use std::fs;
use std::sync::Arc;

use sbpf_cov::coverage::CoverageTracker;
use solana_rbpf::elf::Executable;
use solana_rbpf::memory_region::{MemoryMapping, MemoryRegion};
use solana_rbpf::program::BuiltinProgram;
use solana_rbpf::vm::{EbpfVm, TestContextObject};

#[test]
fn test_java_example_execution_and_coverage() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/java_program.so");
    let obj_path = manifest_dir.join("target/java_program.o");

    if !elf_path.exists() {
        println!("Skipping test: java_program.so not compiled yet.");
        return;
    }

    let elf_bytes =
        fs::read(&elf_path).expect("Failed to read java_program.so");
    let loader = Arc::new(BuiltinProgram::new_mock());
    let executable = match Executable::<TestContextObject>::from_elf(
        &elf_bytes,
        loader.clone(),
    ) {
        Ok(exe) => exe,
        Err(e) => {
            let e_entry =
                u64::from_le_bytes(elf_bytes[24..32].try_into().unwrap());
            println!(
                "Java SBPF Load error: {:?}, e_entry in file = {:#x}",
                e, e_entry
            );
            panic!("Failed to load ELF in solana_rbpf: {:?}", e);
        }
    };

    let mut ro_data = executable.get_ro_section().to_vec();
    let ro_region = executable.get_ro_region();

    let mut context = TestContextObject::new(1_000_000);
    let sbpf_version = executable.get_sbpf_version();

    let mut stack = vec![0u8; executable.get_config().stack_size()];
    let mut heap = Vec::new();
    // Input parameters: [opcode=1 (Deposit), amount=500, balance=1000]
    let mut input = vec![
        1, 0, 0, 0, 0, 0, 0, 0, // opcode = 1
        244, 1, 0, 0, 0, 0, 0, 0, // amount = 500
        232, 3, 0, 0, 0, 0, 0, 0, // balance = 1000
    ];

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
    println!("Javalana SBPF VM Execution Result: {:?}", result);
    assert_eq!(result.unwrap(), 1500);

    // Extract coverage counters automatically
    let mut tracker = CoverageTracker::new();
    tracker
        .sync_from_elf_and_rodata("java_program.o", &obj_path, &ro_data, None)
        .expect("Failed to extract counters from ELF and rodata");

    tracker
        .export_json(std::path::Path::new("/tmp/java_coverage_dump.json"))
        .expect("Failed to export JSON dump");
}
