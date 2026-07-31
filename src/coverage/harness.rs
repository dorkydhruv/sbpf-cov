use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProgramCoverageDump {
    pub program_name: String,
    pub counters: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoverageDump {
    pub programs: HashMap<String, ProgramCoverageDump>,
}

pub struct CoverageTracker {
    pub program_counters: HashMap<String, Vec<u64>>,
}

impl Default for CoverageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverageTracker {
    pub fn new() -> Self {
        Self { program_counters: HashMap::new() }
    }

    /// Automatically detects counter count from instrumented ELF file and syncs from `ro_data`
    pub fn sync_from_elf_and_rodata(
        &mut self,
        program_name: &str,
        elf_path: &Path,
        ro_data: &[u8],
        counter_offset: Option<usize>,
    ) -> Result<usize> {
        let num_counters = detect_num_counters_from_elf(elf_path)?;
        let offset = counter_offset.unwrap_or(0);
        self.sync_from_rodata(program_name, ro_data, offset, num_counters);
        Ok(num_counters)
    }

    /// Synchronizes counter values from executed .rodata slice at the specified offset
    pub fn sync_from_rodata(
        &mut self,
        program_name: &str,
        ro_data: &[u8],
        counter_offset: usize,
        num_counters: usize,
    ) {
        let mut counters = Vec::with_capacity(num_counters);
        for i in 0..num_counters {
            let off = counter_offset + i * 8;
            if off + 8 <= ro_data.len() {
                let val = u64::from_le_bytes(
                    ro_data[off..off + 8].try_into().unwrap(),
                );
                counters.push(val);
            } else {
                counters.push(0);
            }
        }
        self.program_counters.insert(program_name.to_string(), counters);
    }

    /// Exports counter state to JSON format consumed by sbpf-cov convert
    pub fn export_json(&self, path: &Path) -> Result<()> {
        let mut dump = CoverageDump { programs: HashMap::new() };

        for (name, counters) in &self.program_counters {
            dump.programs.insert(
                name.clone(),
                ProgramCoverageDump {
                    program_name: name.clone(),
                    counters: counters.clone(),
                },
            );
        }

        let json = serde_json::to_string_pretty(&dump)?;
        fs::write(path, json).with_context(|| {
            format!("Failed to write coverage dump to {:?}", path)
        })?;
        Ok(())
    }
}

/// Automatically detects the total LLVM counter count from an instrumented ELF file (.o)
pub fn detect_num_counters_from_elf(elf_path: &Path) -> Result<usize> {
    let elf_bytes = fs::read(elf_path)
        .with_context(|| format!("Failed to read ELF file {:?}", elf_path))?;
    let elf_obj = object::File::parse(&*elf_bytes)?;

    // 1. Sum up sizes of all __llvm_prf_cnts sections if present
    let cnts_bytes_total: usize = elf_obj
        .sections()
        .filter(|s| s.name().ok() == Some("__llvm_prf_cnts"))
        .filter_map(|s| s.data().ok())
        .map(|d| d.len())
        .sum();

    if cnts_bytes_total > 0 {
        return Ok(cnts_bytes_total / 8);
    }

    // 2. Fallback: sum NumCounters across all __llvm_prf_data records
    let prf_data_bytes: Vec<u8> = elf_obj
        .sections()
        .filter(|s| s.name().ok() == Some("__llvm_prf_data"))
        .filter_map(|s| s.data().ok())
        .flatten()
        .copied()
        .collect();

    if prf_data_bytes.len() >= 64 && prf_data_bytes.len() % 64 == 0 {
        let num_records = prf_data_bytes.len() / 64;
        let mut total_counters = 0usize;
        for i in 0..num_records {
            let nc_offset = i * 64 + 48; // NumCounters offset in __llvm_prf_data
            if nc_offset + 4 <= prf_data_bytes.len() {
                let nc = u32::from_le_bytes(
                    prf_data_bytes[nc_offset..nc_offset + 4].try_into()?,
                ) as usize;
                total_counters += nc;
            }
        }
        if total_counters > 0 {
            return Ok(total_counters);
        }
    }

    bail!(
        "Could not detect __llvm_prf_cnts or __llvm_prf_data in ELF {:?}",
        elf_path
    );
}
