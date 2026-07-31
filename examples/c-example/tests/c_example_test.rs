use std::fs;
use std::sync::Arc;

use sbpf_cov::coverage::{
    convert_dump_to_profraw, merge_profraw_to_profdata, CoverageTracker,
};
use solana_rbpf::elf::Executable;
use solana_rbpf::memory_region::{MemoryMapping, MemoryRegion};
use solana_rbpf::program::BuiltinProgram;
use solana_rbpf::vm::{EbpfVm, TestContextObject};

#[test]
fn test_c_example_execution_and_end_to_end_coverage() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let elf_path = manifest_dir.join("target/c_prog.so");
    let _c_obj_path = manifest_dir.join("src/program.c");

    if !elf_path.exists() {
        return; // Skip if test fixture not present
    }

    let elf_bytes = fs::read(&elf_path).expect("Failed to read ELF");
    let loader = Arc::new(BuiltinProgram::new_mock());
    let executable = match Executable::<TestContextObject>::from_elf(
        &elf_bytes,
        loader.clone(),
    ) {
        Ok(exe) => exe,
        Err(e) => {
            let e_entry =
                u64::from_le_bytes(elf_bytes[24..32].try_into().unwrap());
            println!("Load error: {:?}, e_entry in file = {:#x}", e, e_entry);
            panic!("Failed to load ELF in solana_rbpf: {:?}", e);
        }
    };

    let mut ro_data = executable.get_ro_section().to_vec();
    let ro_region = executable.get_ro_region();

    // Execute Deposit (Opcode 1)
    let mut context = TestContextObject::new(1_000_000);
    let sbpf_version = executable.get_sbpf_version();

    let mut stack = vec![0u8; executable.get_config().stack_size()];
    let mut heap = Vec::new();
    let mut input =
        vec![1u8, 0, 0, 0, 0, 0, 0, 0, 0, 10, 20, 30, 40, 50, 60, 70, 80];

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
    println!("C SBPF VM Execution Result: {:?}", result);

    // 1. Automatic counter extraction from ELF + rodata
    let c_elf_obj = manifest_dir.join("target/c_program.o");
    let mut tracker = CoverageTracker::new();
    tracker
        .sync_from_elf_and_rodata("c_program.o", &c_elf_obj, &ro_data, None)
        .expect("Failed to auto-detect counter count from ELF");
    assert!(tracker.program_counters.contains_key("c_program.o"));

    // 2. Export JSON dump & convert to profraw
    let dump_path = std::path::PathBuf::from("/tmp/c_coverage_dump.json");
    tracker.export_json(&dump_path).expect("Failed to export JSON dump");

    let profraw_path = std::env::temp_dir().join("c_example.profraw");
    convert_dump_to_profraw(&dump_path, &c_elf_obj, &profraw_path)
        .expect("convert_dump_to_profraw failed");

    let profdata_path = std::env::temp_dir().join("c_example.profdata");
    merge_profraw_to_profdata(&profraw_path, &profdata_path)
        .expect("merge_profraw_to_profdata failed");
}
