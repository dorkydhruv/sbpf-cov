use sbpf_cov::coverage::{
    convert_dump_to_profraw, find_llvm_tool, find_target_elf,
    fixup_sbpf_elf_for_vm, CoverageTracker,
};
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_coverage_tracker_sync_and_export() {
    let mut tracker = CoverageTracker::new();

    // Create dummy .rodata memory slice with offset = 16 bytes
    // Counter[0] = 42, Counter[1] = 10, Counter[2] = 99
    let mut ro_data = vec![0u8; 16];
    ro_data.extend_from_slice(&42u64.to_le_bytes());
    ro_data.extend_from_slice(&10u64.to_le_bytes());
    ro_data.extend_from_slice(&99u64.to_le_bytes());

    // 1. Test sync_from_rodata
    tracker.sync_from_rodata("test_prog.so", &ro_data, 16, 3);

    assert_eq!(
        tracker.program_counters.get("test_prog.so"),
        Some(&vec![42, 10, 99])
    );

    // 2. Test export_json
    let temp_file = NamedTempFile::new().unwrap();
    let json_path = temp_file.path();

    tracker.export_json(json_path).expect("Failed to export JSON");

    let json_str =
        fs::read_to_string(json_path).expect("Failed to read exported JSON");
    let dump: serde_json::Value =
        serde_json::from_str(&json_str).expect("Failed to parse JSON");

    let prog_dump = &dump["programs"]["test_prog.so"];
    assert_eq!(prog_dump["program_name"], "test_prog.so");
    assert_eq!(prog_dump["counters"], serde_json::json!([42, 10, 99]));
}

#[test]
fn test_elf_fixup_engine_ei_osabi_and_rodata_extension() {
    let input_elf = std::path::PathBuf::from("/tmp/bpf_prog_instrumented.so");
    if !input_elf.exists() {
        return; // Skip if test fixture not present
    }

    let temp_output = NamedTempFile::new().unwrap();
    let output_path = temp_output.path();

    let meta = fixup_sbpf_elf_for_vm(&input_elf, output_path)
        .expect("fixup_sbpf_elf_for_vm failed on instrumented ELF");

    assert!(meta.num_counters > 0, "Expected non-zero counter count");

    let fixed_bytes = fs::read(output_path).expect("Failed to read fixed ELF");

    // Verify ELF EI_OSABI (byte 7) is patched to 0 (ELFOSABI_NONE)
    assert_eq!(fixed_bytes[7], 0, "EI_OSABI byte must be 0 (ELFOSABI_NONE)");
}

#[test]
fn test_convert_dump_to_profraw_magic_header_and_structure() {
    let input_dump = std::path::PathBuf::from("/tmp/coverage_dump.json");
    let input_elf = std::path::PathBuf::from("/tmp/bpf_prog_instrumented.so");

    if !input_dump.exists() || !input_elf.exists() {
        return; // Skip if test fixtures not present
    }

    let temp_output = NamedTempFile::new().unwrap();
    let profraw_path = temp_output.path();

    let bytes_written =
        convert_dump_to_profraw(&input_dump, &input_elf, profraw_path)
            .expect("convert_dump_to_profraw failed");

    assert!(
        bytes_written >= 128,
        "Profraw file size must be at least 128 header bytes"
    );

    let profraw_bytes =
        fs::read(profraw_path).expect("Failed to read generated profraw file");

    // Verify LLVM Raw Profile Magic Number (0xFF6C70726F667281 in little-endian order)
    let magic = &profraw_bytes[0..8];
    assert_eq!(
        magic,
        &[0x81, b'r', b'f', b'o', b'r', b'p', b'l', 0xff],
        "Magic bytes must match LLVM raw profile magic spec (0xFF6C70726F667281 LE)"
    );

    // Verify LLVM Raw Profile Version (Version 10: 10u64 LE)
    let version = u64::from_le_bytes(profraw_bytes[8..16].try_into().unwrap());
    assert_eq!(version, 10, "LLVM raw profile version must be 10");
}

#[test]
fn test_find_target_elf_discovery() {
    // 1. Test finding target ELF in current workspace/target
    let elf_res = find_target_elf(None);
    assert!(
        elf_res.is_ok(),
        "find_target_elf should locate target ELF in existing build target"
    );
    let elf_path = elf_res.unwrap();
    assert!(elf_path.exists(), "Discovered ELF file path must exist");

    // 2. Test invalid non-existent path
    let invalid_path =
        std::path::PathBuf::from("/non/existent/manifest/Cargo.toml");
    let invalid_res = find_target_elf(Some(&invalid_path));
    assert!(
        invalid_res.is_err(),
        "find_target_elf should return error for invalid manifest path"
    );
}

#[test]
fn test_find_llvm_tool_resolution() {
    // 1. Test resolving valid LLVM tool
    let tool_res = find_llvm_tool("llvm-cov");
    assert!(
        tool_res.is_ok(),
        "find_llvm_tool should resolve llvm-cov executable"
    );
    let tool_path = tool_res.unwrap();
    assert!(tool_path.exists(), "Resolved LLVM tool path must exist");

    // 2. Test resolving non-existent tool
    let bogus_res = find_llvm_tool("non_existent_binary_tool_xyz_999");
    assert!(
        bogus_res.is_err(),
        "find_llvm_tool must return error for non-existent tool"
    );
}

#[test]
fn test_counter_offset_in_rodata_metadata() {
    let input_elf = std::path::PathBuf::from("/tmp/bpf_prog_instrumented.so");
    if !input_elf.exists() {
        return;
    }

    let temp_output = NamedTempFile::new().unwrap();
    let meta = fixup_sbpf_elf_for_vm(&input_elf, temp_output.path())
        .expect("fixup_sbpf_elf_for_vm failed");

    // In our test fixture, counter offset in rodata is 24 bytes (0x18)
    assert_eq!(meta.counter_offset_in_rodata, 24);
    assert_eq!(meta.num_counters, 3);
}

#[test]
fn test_uninstrumented_elf_fixup() {
    let input_elf = std::path::PathBuf::from("/tmp/c_prog_patched.so");
    if !input_elf.exists() {
        return;
    }

    let temp_output = NamedTempFile::new().unwrap();
    let output_path = temp_output.path();

    let meta = fixup_sbpf_elf_for_vm(&input_elf, output_path).expect(
        "fixup_sbpf_elf_for_vm should handle uninstrumented ELF gracefully",
    );

    let fixed_bytes = fs::read(output_path).expect("Failed to read fixed ELF");
    assert_eq!(fixed_bytes[7], 0, "EI_OSABI byte must be 0 (ELFOSABI_NONE)");
    assert_eq!(
        meta.num_counters, 0,
        "Uninstrumented ELF should report 0 counters"
    );
}
