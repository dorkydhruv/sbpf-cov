use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use object::{Object, ObjectSection};

use crate::trace::ExecutionTrace;

/// Base virtual memory address for SBPF text section execution in Solana VM.
pub const SBPF_TEXT_BASE_ADDR: u64 = 0x100000000;

/// Summary of coverage metrics for a single source file.
#[derive(Debug, Clone, Default)]
pub struct FileCoverageSummary {
    pub file_path: PathBuf,
    pub total_executable_lines: usize,
    pub covered_lines: usize,
    pub missed_lines: usize,
    pub line_coverage_percent: f64,
    /// Maps line number -> hit count (0 if unexecuted, >= 1 if executed)
    pub line_hits: BTreeMap<u32, u64>,
}

impl FileCoverageSummary {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            ..Default::default()
        }
    }

    /// Computes summary metrics based on current line hits map.
    pub fn recalculate_metrics(&mut self) {
        self.total_executable_lines = self.line_hits.len();
        self.covered_lines = self.line_hits.values().filter(|&&hits| hits > 0).count();
        self.missed_lines = self
            .total_executable_lines
            .saturating_sub(self.covered_lines);
        self.line_coverage_percent = if self.total_executable_lines > 0 {
            (self.covered_lines as f64 / self.total_executable_lines as f64) * 100.0
        } else {
            0.0
        };
    }
}

/// Returns true if the path looks like a Rust toolchain / compiler-internal file
fn is_toolchain_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".rustup/toolchains/") || s.contains("/rustc/") || s.contains("/rust/library/")
}

/// Normalizes execution PC addresses to match both relocatable `.o` (base 0x0)
/// and linked SBPF VM execution space (base 0x100000000).
fn is_pc_hit(trace: &ExecutionTrace, pc_addr: u64) -> bool {
    if trace.executed_pcs.is_empty() {
        return false;
    }

    // Direct match (e.g. if trace uses offset addresses or DWARF is linked)
    if trace.is_pc_executed(pc_addr) {
        return true;
    }

    // Map relocatable offset (0x0..) to VM virtual address (0x100000000..)
    if pc_addr < SBPF_TEXT_BASE_ADDR {
        let vm_pc = pc_addr.wrapping_add(SBPF_TEXT_BASE_ADDR);
        if trace.is_pc_executed(vm_pc) {
            return true;
        }
    } else {
        // Map VM virtual address to relocatable offset
        let offset_pc = pc_addr.wrapping_sub(SBPF_TEXT_BASE_ADDR);
        if trace.is_pc_executed(offset_pc) {
            return true;
        }
    }

    false
}

/// Parses DWARF `.debug_line` tables from an SBPF ELF object or shared library (.o / .so)
/// and computes line-by-line coverage using execution trace PCs.
pub fn extract_dwarf_coverage(
    elf_path: impl AsRef<Path>,
    trace: &ExecutionTrace,
) -> Result<HashMap<PathBuf, FileCoverageSummary>> {
    let elf_path = elf_path.as_ref();
    let elf_bytes = fs::read(elf_path)
        .with_context(|| format!("Failed to read ELF file {:?}", elf_path))?;
    let file = object::File::parse(&*elf_bytes)?;

    let endian = if file.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };

    let load_section = |id: gimli::SectionId| -> Result<
        gimli::EndianSlice<gimli::RunTimeEndian>,
    > {
        let name = id.name();
        if let Some(section) = file.section_by_name(name) {
            let data = section.data()?;
            Ok(gimli::EndianSlice::new(data, endian))
        } else {
            Ok(gimli::EndianSlice::new(&[], endian))
        }
    };

    let dwarf = gimli::Dwarf::load(&load_section)?;

    let mut summaries: HashMap<PathBuf, FileCoverageSummary> = HashMap::new();
    let mut iter = dwarf.units();

    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        if let Some(line_program) = unit.line_program.clone() {
            let mut rows = line_program.rows();
            while let Some((header, row)) = rows.next_row()? {
                if let Some(file_entry) = row.file(header) {
                    let mut path = PathBuf::new();
                    if let Some(dir) = file_entry.directory(header) {
                        let dir_str = dwarf.attr_string(&unit, dir)?;
                        path.push(dir_str.to_string_lossy().as_ref());
                    }
                    let filename =
                        dwarf.attr_string(&unit, file_entry.path_name())?;
                    path.push(filename.to_string_lossy().as_ref());

                    if let Ok(canonical) = path.canonicalize() {
                        path = canonical;
                    }

                    // Skip Rust toolchain / compiler-internal source files
                    if is_toolchain_path(&path) {
                        continue;
                    }

                    if let Some(line) = row.line() {
                        let line_u32 = line.get() as u32;
                        let pc_addr = row.address();
                        let is_hit = is_pc_hit(trace, pc_addr);

                        let summary = summaries
                            .entry(path)
                            .or_insert_with_key(|p| FileCoverageSummary::new(p.clone()));

                        let current_hits = summary.line_hits.entry(line_u32).or_insert(0);
                        if is_hit {
                            *current_hits += 1;
                        }
                    }
                }
            }
        }
    }

    // Compute metrics
    for summary in summaries.values_mut() {
        summary.recalculate_metrics();
    }

    Ok(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pc_hit_normalization() {
        let mut trace = ExecutionTrace::default();
        trace.executed_pcs.insert(0x100000008);

        // Relocatable offset 0x8 should match VM PC 0x100000008
        assert!(is_pc_hit(&trace, 0x8));
        assert!(!is_pc_hit(&trace, 0x10));
    }
}
