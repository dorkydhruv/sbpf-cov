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
/// 4. Ensures all section header `sh_offset` values are monotonically non-decreasing.
pub fn fixup_sbpf_elf_for_vm(
    input_path: &Path,
    output_path: &Path,
) -> Result<CoverageFixupMetadata> {
    let mut data = fs::read(input_path).with_context(|| {
        format!("Failed to read input ELF {:?}", input_path)
    })?;

    // Fix ei_osabi (byte 7) -> 0 (ELFOSABI_NONE)
    if data.len() > 7 {
        data[7] = 0;
    }

    if data.len() < 0x40 {
        fs::write(output_path, &data)?;
        return Ok(CoverageFixupMetadata {
            counter_offset_in_rodata: 0,
            num_counters: 0,
            original_elf: input_path.to_string_lossy().to_string(),
            fixed_elf: output_path.to_string_lossy().to_string(),
        });
    }

    let e_shoff = u64::from_le_bytes(data[0x28..0x30].try_into()?) as usize;
    let e_shentsize =
        u16::from_le_bytes(data[0x3A..0x3C].try_into()?) as usize;
    let e_shnum = u16::from_le_bytes(data[0x3C..0x3E].try_into()?) as usize;

    let mut text_offset = 0u64;
    if e_shoff != 0
        && e_shnum != 0
        && e_shoff + e_shnum * e_shentsize <= data.len()
    {
        let e_shstrndx =
            u16::from_le_bytes(data[0x3E..0x40].try_into()?) as usize;
        let shstrtab_off = e_shoff + e_shstrndx * e_shentsize;
        if shstrtab_off + 40 <= data.len() {
            let str_offset = u64::from_le_bytes(
                data[shstrtab_off + 24..shstrtab_off + 32].try_into()?,
            ) as usize;
            let str_size = u64::from_le_bytes(
                data[shstrtab_off + 32..shstrtab_off + 40].try_into()?,
            ) as usize;
            if str_offset + str_size <= data.len() {
                let strtab = &data[str_offset..str_offset + str_size];
                for i in 0..e_shnum {
                    let off = e_shoff + i * e_shentsize;
                    let sh_name_idx =
                        u32::from_le_bytes(data[off..off + 4].try_into()?)
                            as usize;
                    if sh_name_idx < strtab.len() {
                        let name_end = strtab[sh_name_idx..]
                            .iter()
                            .position(|&b| b == 0)
                            .unwrap_or(0);
                        let name = String::from_utf8_lossy(
                            &strtab[sh_name_idx..sh_name_idx + name_end],
                        );
                        if name == ".text" {
                            text_offset = u64::from_le_bytes(
                                data[off + 24..off + 32].try_into()?,
                            );
                            break;
                        }
                    }
                }
            }
        }
    }

    // Patch "entrypoint.local" to "entrypoint\0" in ELF string table if present
    if let Some(pos) = data.windows(16).position(|w| w == b"entrypoint.local")
    {
        data[pos + 10] = 0;
    }

    // Patch unresolved sol_log_ syscall calls (call -0x1 -> call sol_log_ 0x7179069d)
    for i in (0..data.len().saturating_sub(8)).step_by(8) {
        if data[i..i + 8] == [0x85, 0x10, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff] {
            let sol_log_hash: u32 = 0x7179069d;
            data[i + 4..i + 8].copy_from_slice(&sol_log_hash.to_le_bytes());
        }
    }
    let mut e_entry = u64::from_le_bytes(data[24..32].try_into()?);
    if e_entry == 0x100000000 {
        e_entry = 0x100000040;
        data[24..32].copy_from_slice(&e_entry.to_le_bytes());
    }
    // Patch SBPF LDDW relocations (r1 = 0x0 ll -> r1 = 0x100001000 ll) for counter base address in writable rodata region
    for i in 0..data.len().saturating_sub(24) {
        if data[i] == 0x18
            && data[i + 1] == 0x01
            && data[i + 4..i + 8] == [0, 0, 0, 0]
            && data[i + 12..i + 16] == [0, 0, 0, 0]
        {
            let next_op = data[i + 16];
            if next_op == 0x79 || next_op == 0x7b {
                data[i + 5] = 0x10; // Offset 0x1000 (4096 bytes after text)
                data[i + 12] = 0x01; // Base 0x100000000
            }
        }
    }

    if e_entry == 0 {
        let entry_vaddr: u64 = text_offset;
        data[24..32].copy_from_slice(&entry_vaddr.to_le_bytes());
    }

    if e_shoff == 0
        || e_shnum == 0
        || e_shoff + e_shnum * e_shentsize > data.len()
    {
        fs::write(output_path, &data)?;
        return Ok(CoverageFixupMetadata {
            counter_offset_in_rodata: 0,
            num_counters: 0,
            original_elf: input_path.to_string_lossy().to_string(),
            fixed_elf: output_path.to_string_lossy().to_string(),
        });
    }

    let e_shstrndx = u16::from_le_bytes(data[0x3E..0x40].try_into()?) as usize;
    let shstrtab_off = e_shoff + e_shstrndx * e_shentsize;
    let str_offset = u64::from_le_bytes(
        data[shstrtab_off + 24..shstrtab_off + 32].try_into()?,
    ) as usize;
    let str_size = u64::from_le_bytes(
        data[shstrtab_off + 32..shstrtab_off + 40].try_into()?,
    ) as usize;

    if str_offset + str_size > data.len() {
        fs::write(output_path, &data)?;
        return Ok(CoverageFixupMetadata {
            counter_offset_in_rodata: 0,
            num_counters: 0,
            original_elf: input_path.to_string_lossy().to_string(),
            fixed_elf: output_path.to_string_lossy().to_string(),
        });
    }

    let strtab = data[str_offset..str_offset + str_size].to_vec();

    let get_name = |idx: usize, raw_data: &[u8]| -> String {
        let off = e_shoff + idx * e_shentsize;
        let sh_name_idx =
            u32::from_le_bytes(raw_data[off..off + 4].try_into().unwrap())
                as usize;
        let name_end =
            strtab[sh_name_idx..].iter().position(|&b| b == 0).unwrap_or(0);
        String::from_utf8_lossy(&strtab[sh_name_idx..sh_name_idx + name_end])
            .to_string()
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
            let size = u64::from_le_bytes(data[off + 32..off + 40].try_into()?)
                as usize;
            if name == ".rodata" {
                rodata_info = Some(SecInfo { hdr_off: off, size });
            } else if name == "__llvm_prf_cnts" || name == ".llvm_prf_cnts" {
                cnts_info = Some(SecInfo { hdr_off: off, size });
            }
        }
    }

    let (counter_offset_in_rodata, num_counters) =
        if let (Some(rodata), Some(cnts)) = (rodata_info, cnts_info) {
            let counter_offset = rodata.size;
            let new_rodata_size = rodata.size + cnts.size;
            data[rodata.hdr_off + 32..rodata.hdr_off + 40]
                .copy_from_slice(&(new_rodata_size as u64).to_le_bytes());

            let to_null: HashSet<&str> = [".bss"].into_iter().collect();

            for i in 0..e_shnum {
                let off = e_shoff + i * e_shentsize;
                let sh_type =
                    u32::from_le_bytes(data[off + 4..off + 8].try_into()?);
                if sh_type != 0 {
                    let name = get_name(i, &data);
                    if to_null.contains(name.as_str()) {
                        data[off..off + 4]
                            .copy_from_slice(&0u32.to_le_bytes());
                        data[off + 4..off + 8]
                            .copy_from_slice(&0u32.to_le_bytes());
                        data[off + 8..off + 16]
                            .copy_from_slice(&0u64.to_le_bytes());
                        data[off + 16..off + 24]
                            .copy_from_slice(&0u64.to_le_bytes());
                        data[off + 32..off + 40]
                            .copy_from_slice(&0u64.to_le_bytes());
                    }
                }
            }
            (counter_offset, cnts.size / 8)
        } else {
            (0, 0)
        };

    let mut running_offset = 0u64;
    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        let sh_type = u32::from_le_bytes(data[off + 4..off + 8].try_into()?);
        let mut sh_offset =
            u64::from_le_bytes(data[off + 24..off + 32].try_into()?);
        let sh_size = u64::from_le_bytes(data[off + 32..off + 40].try_into()?);

        if sh_type != 0 && sh_offset < running_offset {
            data[off + 24..off + 32]
                .copy_from_slice(&running_offset.to_le_bytes());
            sh_offset = running_offset;
        }

        running_offset = std::cmp::max(running_offset, sh_offset + sh_size);
    }

    fs::write(output_path, &data).with_context(|| {
        format!("Failed to write fixed ELF to {:?}", output_path)
    })?;

    Ok(CoverageFixupMetadata {
        counter_offset_in_rodata,
        num_counters,
        original_elf: input_path.to_string_lossy().to_string(),
        fixed_elf: output_path.to_string_lossy().to_string(),
    })
}
