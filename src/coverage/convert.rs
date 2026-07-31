use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ProgramCoverageDump {
    pub program_name: String,
    pub counters: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct CoverageDump {
    pub programs: HashMap<String, ProgramCoverageDump>,
}

/// Magic number: bytes [0x81, 'r', 'f', 'o', 'r', 'p', 'l', 0xFF] read as u64 LE.
pub const INSTR_PROF_RAW_MAGIC_64: u64 = 0xFF6C70726F667281;

/// Raw profile format version emitted by rustc 1.99.0-nightly / 1.84.1-dev (LLVM 19/22).
pub const INSTR_PROF_RAW_VERSION: u64 = 10;

/// Size of one `__llvm_prf_data` record in Version 10 format.
pub const DATA_RECORD_SIZE: usize = 64;

/// Offset of `CounterPtr` field within a data record.
pub const COUNTER_PTR_OFFSET: usize = 16;

/// Offset of `NumCounters` field within a data record.
pub const NUM_COUNTERS_OFFSET: usize = 48;

/// IPVK_Last value for Version 10.
pub const IPVK_LAST: u64 = 2;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RawHeader {
    pub magic: u64,
    pub version: u64,
    pub binary_ids_size: u64,
    pub num_data: u64,
    pub padding_bytes_before_counters: u64,
    pub num_counters: u64,
    pub padding_bytes_after_counters: u64,
    pub num_bitmap_bytes: u64,
    pub padding_bytes_after_bitmap_bytes: u64,
    pub names_size: u64,
    pub counters_delta: i64,
    pub names_delta: u64,
    pub bitmap_delta: i64,
    pub num_vtables: u64,
    pub vnames_size: u64,
    pub value_kind_last: u64,
}

const _: () = assert!(std::mem::size_of::<RawHeader>() == 128);

pub fn convert_dump_to_profraw(
    dump_path: &Path,
    elf_path: &Path,
    output_path: &Path,
) -> Result<usize> {
    let dump_content = fs::read_to_string(dump_path).with_context(|| {
        format!("Failed to read dump file {:?}", dump_path)
    })?;
    let dump: CoverageDump = serde_json::from_str(&dump_content)?;

    let elf_bytes = fs::read(elf_path)
        .with_context(|| format!("Failed to read ELF file {:?}", elf_path))?;
    let elf_obj = object::File::parse(&*elf_bytes)?;

    let prf_data_bytes: Vec<u8> = elf_obj
        .sections()
        .filter(|s| s.name().ok() == Some("__llvm_prf_data"))
        .filter_map(|s| s.data().ok())
        .flatten()
        .copied()
        .collect();
    let prf_names_bytes: Vec<u8> = elf_obj
        .sections()
        .filter(|s| s.name().ok() == Some("__llvm_prf_names"))
        .filter_map(|s| s.data().ok())
        .flatten()
        .copied()
        .collect();

    if prf_data_bytes.is_empty() {
        bail!(
            "ELF file {:?} has no __llvm_prf_data section. Was it compiled with `-C instrument-coverage`?",
            elf_path
        );
    }
    if prf_names_bytes.is_empty() {
        bail!(
            "ELF file {:?} has no __llvm_prf_names section. Was it compiled with `-C instrument-coverage`?",
            elf_path
        );
    }

    if prf_data_bytes.len() % DATA_RECORD_SIZE != 0 {
        bail!(
            "__llvm_prf_data size ({} bytes) is not a multiple of record size ({} bytes)",
            prf_data_bytes.len(),
            DATA_RECORD_SIZE
        );
    }
    let num_data = prf_data_bytes.len() / DATA_RECORD_SIZE;

    let mut per_fn_num_counters: Vec<u32> = Vec::with_capacity(num_data);
    for i in 0..num_data {
        let offset = i * DATA_RECORD_SIZE + NUM_COUNTERS_OFFSET;
        let nc =
            u32::from_le_bytes(prf_data_bytes[offset..offset + 4].try_into()?);
        per_fn_num_counters.push(nc);
    }

    let total_counters_from_elf: u64 =
        per_fn_num_counters.iter().map(|&n| n as u64).sum();

    let file_name = elf_path.file_name().unwrap().to_str().unwrap();
    let file_stem = elf_path.file_stem().unwrap().to_str().unwrap();
    let counters = dump
        .programs
        .get(file_name)
        .or_else(|| dump.programs.get(file_stem))
        .or_else(|| {
            dump.programs.iter().find_map(|(k, v)| {
                if k.starts_with(file_stem)
                    || file_stem.starts_with(k.split('.').next().unwrap_or(""))
                {
                    Some(v)
                } else {
                    None
                }
            })
        })
        .or_else(|| dump.programs.values().next())
        .map(|p| p.counters.as_slice())
        .unwrap_or(&[]);

    let num_counters = if counters.is_empty() {
        total_counters_from_elf
    } else {
        if counters.len() as u64 != total_counters_from_elf {
            bail!(
                "Counter count mismatch: JSON dump has {} counters, but ELF expects {} total",
                counters.len(),
                total_counters_from_elf
            );
        }
        counters.len() as u64
    };

    let mut data_records = prf_data_bytes.to_vec();
    let mut cumulative_counter_bytes: i64 = 0;

    for i in 0..num_data {
        let rec_offset = i * DATA_RECORD_SIZE;
        let new_counter_ptr =
            cumulative_counter_bytes - (i as i64 * DATA_RECORD_SIZE as i64);
        data_records[rec_offset + COUNTER_PTR_OFFSET
            ..rec_offset + COUNTER_PTR_OFFSET + 8]
            .copy_from_slice(&new_counter_ptr.to_le_bytes());

        data_records[rec_offset + 24..rec_offset + 32]
            .copy_from_slice(&0i64.to_le_bytes());
        data_records[rec_offset + 32..rec_offset + 40]
            .copy_from_slice(&0u64.to_le_bytes());
        data_records[rec_offset + 40..rec_offset + 48]
            .copy_from_slice(&0u64.to_le_bytes());

        cumulative_counter_bytes += per_fn_num_counters[i] as i64 * 8;
    }

    let names_size = prf_names_bytes.len() as u64;

    let header = RawHeader {
        magic: INSTR_PROF_RAW_MAGIC_64,
        version: INSTR_PROF_RAW_VERSION,
        binary_ids_size: 0,
        num_data: num_data as u64,
        padding_bytes_before_counters: 0,
        num_counters,
        padding_bytes_after_counters: 0,
        num_bitmap_bytes: 0,
        padding_bytes_after_bitmap_bytes: 0,
        names_size,
        counters_delta: 0,
        names_delta: 0,
        bitmap_delta: 0,
        num_vtables: 0,
        vnames_size: 0,
        value_kind_last: IPVK_LAST,
    };

    let mut profraw = Vec::new();
    let header_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(
            &header as *const RawHeader as *const u8,
            std::mem::size_of::<RawHeader>(),
        )
    };
    profraw.extend_from_slice(header_bytes);
    profraw.extend_from_slice(&data_records);

    if counters.is_empty() {
        profraw.extend_from_slice(&vec![0u8; num_counters as usize * 8]);
    } else {
        for &cnt in counters {
            profraw.extend_from_slice(&cnt.to_le_bytes());
        }
    }

    profraw.extend_from_slice(&prf_names_bytes);
    let names_padding = (8 - (prf_names_bytes.len() % 8)) % 8;
    profraw.extend_from_slice(&vec![0u8; names_padding]);

    fs::write(output_path, &profraw).with_context(|| {
        format!("Failed to write output to {:?}", output_path)
    })?;

    Ok(profraw.len())
}
