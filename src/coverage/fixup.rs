use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct CoverageFixupMetadata {
    pub counter_offset_in_rodata: usize,
    pub num_counters: usize,
    pub original_elf: String,
    pub fixed_elf: String,
}

/// Post-processes an instrumented SBPF ELF for `solana_rbpf` VM compatibility:
/// 1. Fixes `ei_osabi` (byte 7) -> 0 (`ELFOSABI_NONE`) to resolve `WrongAbi` error.
/// 2. Extends `.rodata` section header to absorb contiguous `__llvm_prf_cnts` counters.
/// 3. Nulls out section headers for `__llvm_prf_cnts`, `__llvm_prf_data`, `__llvm_prf_names`,
///    `__llvm_covmap`, `__llvm_covfun`, `.bss`.
/// 4. Ensures all section header `sh_offset` values are monotonically non-decreasing to resolve
///    `SectionNotInOrder` error.
pub fn fixup_sbpf_elf_for_vm(
    input_path: &Path,
    output_path: &Path,
) -> Result<CoverageFixupMetadata> {
    let mut data = fs::read(input_path)
        .with_context(|| format!("Failed to read input ELF {:?}", input_path))?;

    // Fix ei_osabi (byte 7) -> 0 (ELFOSABI_NONE)
    if data.len() > 7 {
        data[7] = 0;
    }

    let e_shoff = u64::from_le_bytes(data[0x28..0x30].try_into()?) as usize;
    let e_shentsize = u16::from_le_bytes(data[0x3A..0x3C].try_into()?) as usize;
    let e_shnum = u16::from_le_bytes(data[0x3C..0x3E].try_into()?) as usize;
    let e_shstrndx = u16::from_le_bytes(data[0x3E..0x40].try_into()?) as usize;

    let shstrtab_off = e_shoff + e_shstrndx * e_shentsize;
    let str_offset = u64::from_le_bytes(data[shstrtab_off + 24..shstrtab_off + 32].try_into()?) as usize;
    let str_size = u64::from_le_bytes(data[shstrtab_off + 32..shstrtab_off + 40].try_into()?) as usize;
    let strtab = data[str_offset..str_offset + str_size].to_vec();

    let get_name = |idx: usize, raw_data: &[u8]| -> String {
        let off = e_shoff + idx * e_shentsize;
        let sh_name_idx = u32::from_le_bytes(raw_data[off..off + 4].try_into().unwrap()) as usize;
        let name_end = strtab[sh_name_idx..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(0);
        String::from_utf8_lossy(&strtab[sh_name_idx..sh_name_idx + name_end]).to_string()
    };

    struct SecInfo {
        hdr_off: usize,
        size: usize,
    }

    let mut rodata_info: Option<SecInfo> = None;
    let mut cnts_info: Option<SecInfo> = None;

    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        let sh_type = u32::from_le_bytes(data[off + 4..off + 8].try_into()?);
        if sh_type != 0 {
            let name = get_name(i, &data);
            let size = u64::from_le_bytes(data[off + 32..off + 40].try_into()?) as usize;
            if name == ".rodata" {
                rodata_info = Some(SecInfo { hdr_off: off, size });
            } else if name == "__llvm_prf_cnts" {
                cnts_info = Some(SecInfo { hdr_off: off, size });
            }
        }
    }

    let rodata = rodata_info.context("Missing .rodata section in ELF")?;
    let cnts = cnts_info.context("Missing __llvm_prf_cnts section in ELF")?;

    let counter_offset_in_rodata = rodata.size;
    let new_rodata_size = rodata.size + cnts.size;

    // Extend .rodata size
    data[rodata.hdr_off + 32..rodata.hdr_off + 40]
        .copy_from_slice(&(new_rodata_size as u64).to_le_bytes());

    // Sections to null out
    let to_null: HashSet<&str> = [
        "__llvm_prf_cnts",
        "__llvm_prf_data",
        "__llvm_prf_names",
        "__llvm_covmap",
        "__llvm_covfun",
        ".bss",
    ]
    .into_iter()
    .collect();

    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        let sh_type = u32::from_le_bytes(data[off + 4..off + 8].try_into()?);
        if sh_type != 0 {
            let name = get_name(i, &data);
            if to_null.contains(name.as_str()) {
                data[off..off + 4].copy_from_slice(&0u32.to_le_bytes());  // sh_name = 0
                data[off + 4..off + 8].copy_from_slice(&0u32.to_le_bytes()); // sh_type = SHT_NULL
                data[off + 8..off + 16].copy_from_slice(&0u64.to_le_bytes()); // sh_flags = 0
                data[off + 16..off + 24].copy_from_slice(&0u64.to_le_bytes()); // sh_addr = 0
                data[off + 32..off + 40].copy_from_slice(&0u64.to_le_bytes()); // sh_size = 0
            }
        }
    }

    // Fix monotonic sh_offset for section header table
    let mut running_offset = 0u64;
    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        let mut sh_offset = u64::from_le_bytes(data[off + 24..off + 32].try_into()?);
        let sh_size = u64::from_le_bytes(data[off + 32..off + 40].try_into()?);

        if sh_offset < running_offset {
            data[off + 24..off + 32].copy_from_slice(&running_offset.to_le_bytes());
            sh_offset = running_offset;
        }

        running_offset = std::cmp::max(running_offset, sh_offset + sh_size);
    }

    fs::write(output_path, &data)
        .with_context(|| format!("Failed to write fixed ELF to {:?}", output_path))?;

    Ok(CoverageFixupMetadata {
        counter_offset_in_rodata,
        num_counters: cnts.size / 8,
        original_elf: input_path.to_string_lossy().to_string(),
        fixed_elf: output_path.to_string_lossy().to_string(),
    })
}
