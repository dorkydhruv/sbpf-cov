use sbpf_cov::coverage::CoverageDump;
use sbpf_cov_interposer::{sbpf_cov_dump_now, sbpf_cov_register_program};
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_interposer_c_api_register_and_dump() {
    let temp_file = NamedTempFile::new().unwrap();
    let dump_path = temp_file.path().to_path_buf();

    // Create dummy rodata buffer in host memory
    // Offset 16: counter 0 = 100, counter 1 = 200
    let mut rodata = vec![0u8; 16];
    rodata.extend_from_slice(&100u64.to_le_bytes());
    rodata.extend_from_slice(&200u64.to_le_bytes());

    let prog_name = std::ffi::CString::new("my_test_program.so").unwrap();

    unsafe {
        sbpf_cov_register_program(
            prog_name.as_ptr(),
            rodata.as_ptr(),
            rodata.len(),
            16,
            2,
        );
    }

    // Trigger dump to temp path
    let path_c = std::ffi::CString::new(dump_path.to_str().unwrap()).unwrap();
    unsafe {
        sbpf_cov_dump_now(path_c.as_ptr());
    }

    // Read and verify JSON dump
    let json_str =
        fs::read_to_string(&dump_path).expect("Failed to read dump file");
    let dump: CoverageDump =
        serde_json::from_str(&json_str).expect("Failed to parse dump JSON");

    let prog = dump
        .programs
        .get("my_test_program.so")
        .expect("Program missing in dump");
    assert_eq!(prog.program_name, "my_test_program.so");
    assert_eq!(prog.counters, vec![100, 200]);
}

#[test]
fn test_interposer_multi_program_registration() {
    let temp_file = NamedTempFile::new().unwrap();
    let dump_path = temp_file.path().to_path_buf();

    let mut rodata_a = vec![0u8; 8];
    rodata_a.extend_from_slice(&111u64.to_le_bytes());

    let mut rodata_b = vec![0u8; 8];
    rodata_b.extend_from_slice(&222u64.to_le_bytes());

    let prog_a = std::ffi::CString::new("prog_a.so").unwrap();
    let prog_b = std::ffi::CString::new("prog_b.so").unwrap();

    unsafe {
        sbpf_cov_register_program(
            prog_a.as_ptr(),
            rodata_a.as_ptr(),
            rodata_a.len(),
            8,
            1,
        );
        sbpf_cov_register_program(
            prog_b.as_ptr(),
            rodata_b.as_ptr(),
            rodata_b.len(),
            8,
            1,
        );
    }

    let path_c = std::ffi::CString::new(dump_path.to_str().unwrap()).unwrap();
    unsafe {
        sbpf_cov_dump_now(path_c.as_ptr());
    }

    let json_str =
        fs::read_to_string(&dump_path).expect("Failed to read dump file");
    let dump: CoverageDump =
        serde_json::from_str(&json_str).expect("Failed to parse dump JSON");

    assert!(dump.programs.contains_key("prog_a.so"));
    assert!(dump.programs.contains_key("prog_b.so"));
    assert_eq!(dump.programs["prog_a.so"].counters, vec![111]);
    assert_eq!(dump.programs["prog_b.so"].counters, vec![222]);
}

#[test]
fn test_interposer_c_api_dump_now_null_path() {
    // Calling sbpf_cov_dump_now with std::ptr::null() should not panic
    unsafe {
        sbpf_cov_dump_now(std::ptr::null());
    }
}
