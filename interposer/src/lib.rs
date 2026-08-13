use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Mutex;

use ctor::{ctor, dtor};
use lazy_static::lazy_static;
use sbpf_cov::coverage::{CoverageDump, ProgramCoverageDump};

struct RegisteredProgram {
    _name: String,
    rodata_ptr: usize,
    rodata_len: usize,
    counter_offset: usize,
    num_counters: usize,
}

lazy_static! {
    static ref REGISTERED_PROGRAMS: Mutex<HashMap<String, RegisteredProgram>> =
        Mutex::new(HashMap::new());
    static ref OUTPUT_PATH: Mutex<PathBuf> =
        Mutex::new(PathBuf::from("coverage_dump.json"));
}

#[ctor]
fn on_interposer_load() {
    if let Ok(env_path) = std::env::var("SBPF_COV_DUMP_PATH") {
        if let Ok(mut path_guard) = OUTPUT_PATH.lock() {
            *path_guard = PathBuf::from(env_path);
        }
    }
    eprintln!("[sbpf-cov-interposer] Runtime coverage interposer loaded.");
}

#[dtor]
fn on_interposer_exit() {
    if let Ok(guard) = REGISTERED_PROGRAMS.lock() {
        if guard.is_empty() {
            return;
        }
    }
    eprintln!(
        "[sbpf-cov-interposer] Process exiting. Flushing coverage dump..."
    );
    flush_coverage_dump();
}

pub fn flush_coverage_dump() {
    let progs = match REGISTERED_PROGRAMS.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if progs.is_empty() {
        return;
    }

    let dump_path = match OUTPUT_PATH.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => PathBuf::from("coverage_dump.json"),
    };

    let mut dump = CoverageDump::default();

    for (name, prog) in progs.iter() {
        if prog.rodata_ptr == 0 || prog.rodata_len == 0 {
            continue;
        }

        let slice = unsafe {
            std::slice::from_raw_parts(
                prog.rodata_ptr as *const u8,
                prog.rodata_len,
            )
        };

        let mut counters = Vec::with_capacity(prog.num_counters);
        for i in 0..prog.num_counters {
            let start = prog.counter_offset + i * 8;
            if start + 8 <= slice.len() {
                let bytes: [u8; 8] =
                    slice[start..start + 8].try_into().unwrap_or([0; 8]);
                counters.push(u64::from_le_bytes(bytes));
            } else {
                counters.push(0);
            }
        }

        let is_all_zero = counters.iter().all(|&c| c == 0);
        if !counters.is_empty() && !is_all_zero {
            dump.programs.insert(
                name.clone(),
                ProgramCoverageDump { program_name: name.clone(), counters },
            );
        }
    }

    if dump.programs.is_empty() {
        return;
    }

    if let Ok(json_str) = serde_json::to_string_pretty(&dump) {
        if let Err(err) = std::fs::write(&dump_path, json_str) {
            eprintln!(
                "[sbpf-cov-interposer] Failed to write dump to {:?}: {}",
                dump_path, err
            );
        } else {
            eprintln!("[sbpf-cov-interposer] ✅ Successfully wrote coverage dump to {:?}", dump_path);
        }
    }
}

/// Registers an SBPF program's .rodata memory region with the interposer
#[no_mangle]
pub unsafe extern "C" fn sbpf_cov_register_program(
    name: *const c_char,
    rodata_ptr: *const u8,
    rodata_len: usize,
    counter_offset: usize,
    num_counters: usize,
) {
    if name.is_null() || rodata_ptr.is_null() {
        return;
    }

    let c_str = CStr::from_ptr(name);
    let prog_name = match c_str.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return,
    };

    if let Ok(mut progs) = REGISTERED_PROGRAMS.lock() {
        progs.insert(
            prog_name.clone(),
            RegisteredProgram {
                _name: prog_name,
                rodata_ptr: rodata_ptr as usize,
                rodata_len,
                counter_offset,
                num_counters,
            },
        );
    }
}

/// Manually triggers coverage dump export
#[no_mangle]
pub unsafe extern "C" fn sbpf_cov_dump_now(path: *const c_char) {
    if !path.is_null() {
        if let Ok(c_str) = CStr::from_ptr(path).to_str() {
            if let Ok(mut path_guard) = OUTPUT_PATH.lock() {
                *path_guard = PathBuf::from(c_str);
            }
        }
    }
    flush_coverage_dump();
}
