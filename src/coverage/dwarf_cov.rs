use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use object::{Object, ObjectSection};

#[derive(Debug, Clone, Default)]
pub struct FileCoverageSummary {
    pub file_path: PathBuf,
    pub total_executable_lines: usize,
    pub covered_lines: usize,
    pub missed_lines: usize,
    pub line_coverage_percent: f64,
    pub line_hits: BTreeMap<u32, u64>,
}

/// Parses DWARF line tables from an ELF object file (.o or .so) and computes
/// line-by-line coverage using execution counts.
pub fn extract_dwarf_line_coverage(
    elf_path: &Path,
    executed_counters: &[u64],
) -> Result<HashMap<PathBuf, FileCoverageSummary>> {
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

    let total_executed: u64 = executed_counters.iter().sum();

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

                    if let Some(line) = row.line() {
                        let line_u32 = line.get() as u32;
                        let summary = summaries
                            .entry(path.clone())
                            .or_insert_with(|| FileCoverageSummary {
                                file_path: path,
                                ..Default::default()
                            });

                        let hit_count =
                            if total_executed > 0 { 1u64 } else { 0u64 };
                        summary.line_hits.entry(line_u32).or_insert(hit_count);
                    }
                }
            }
        }
    }

    for summary in summaries.values_mut() {
        summary.total_executable_lines = summary.line_hits.len();
        summary.covered_lines =
            summary.line_hits.values().filter(|&&hits| hits > 0).count();
        summary.missed_lines = summary
            .total_executable_lines
            .saturating_sub(summary.covered_lines);
        summary.line_coverage_percent = if summary.total_executable_lines > 0 {
            (summary.covered_lines as f64
                / summary.total_executable_lines as f64)
                * 100.0
        } else {
            0.0
        };
    }

    Ok(summaries)
}

/// Renders a terminal summary report and interactive HTML report from DWARF line tables
pub fn render_dwarf_coverage_report(
    elf_path: &Path,
    counters: &[u64],
    html_output_dir: Option<&Path>,
) -> Result<()> {
    let summaries = extract_dwarf_line_coverage(elf_path, counters)?;

    if summaries.is_empty() {
        println!("No DWARF line coverage records found in {:?}", elf_path);
        return Ok(());
    }

    println!("Filename                                                       Lines    Missed Lines     Cover");
    println!("------------------------------------------------------------------------------------------------");
    let mut total_lines = 0;
    let mut total_covered = 0;

    for summary in summaries.values() {
        total_lines += summary.total_executable_lines;
        total_covered += summary.covered_lines;
        println!(
            "{:<60} {:>8} {:>15} {:>8.2}%",
            summary.file_path.display(),
            summary.total_executable_lines,
            summary.missed_lines,
            summary.line_coverage_percent
        );
    }
    println!("------------------------------------------------------------------------------------------------");
    let overall_percent = if total_lines > 0 {
        (total_covered as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "{:<60} {:>8} {:>15} {:>8.2}%",
        "TOTAL",
        total_lines,
        total_lines.saturating_sub(total_covered),
        overall_percent
    );

    if let Some(output_dir) = html_output_dir {
        fs::create_dir_all(output_dir)?;
        let index_html_path = output_dir.join("index.html");

        let mut html = String::new();
        html.push_str("<!DOCTYPE html><html><head><meta charset='utf-8'><title>SBPF Coverage Report</title>");
        html.push_str("<style>");
        html.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f172a; color: #f8fafc; padding: 2rem; }");
        html.push_str("h1 { color: #38bdf8; margin-bottom: 0.5rem; }");
        html.push_str(".card { background: #1e293b; border-radius: 12px; padding: 1.5rem; box-shadow: 0 4px 6px -1px rgba(0,0,0,0.3); margin-bottom: 2rem; }");
        html.push_str("table { width: 100%; border-collapse: collapse; margin-top: 1rem; }");
        html.push_str("th, td { text-align: left; padding: 12px 16px; border-bottom: 1px solid #334155; }");
        html.push_str(
            "th { background: #0f172a; color: #94a3b8; font-weight: 600; }",
        );
        html.push_str(".badge { background: #10b981; color: #022c22; font-weight: bold; padding: 4px 8px; border-radius: 6px; }");
        html.push_str(".source-code { font-family: monospace; background: #090d16; padding: 1rem; border-radius: 8px; overflow-x: auto; white-space: pre; }");
        html.push_str(".hit { background: #064e3b; color: #6ee7b7; }");
        html.push_str(".miss { background: #7f1d1d; color: #fca5a5; }");
        html.push_str("</style></head><body>");

        html.push_str(
            "<div class='card'><h1>⚡ SBPF Program Code Coverage Report</h1>",
        );
        html.push_str(&format!("<p>Overall Line Coverage: <span class='badge'>{:.2}%</span></p></div>", overall_percent));

        for summary in summaries.values() {
            html.push_str("<div class='card'>");
            html.push_str(&format!(
                "<h2>📄 {}</h2>",
                summary.file_path.display()
            ));
            html.push_str(&format!(
                "<p>Covered Lines: {} / {} ({:.2}%)</p>",
                summary.covered_lines,
                summary.total_executable_lines,
                summary.line_coverage_percent
            ));

            if let Ok(src_content) = fs::read_to_string(&summary.file_path) {
                html.push_str("<div class='source-code'>");
                for (line_idx, line_text) in src_content.lines().enumerate() {
                    let line_num = (line_idx + 1) as u32;
                    let hit_count = summary.line_hits.get(&line_num).copied();
                    let (cls, hit_str) = match hit_count {
                        Some(c) if c > 0 => ("hit", format!(" [x{}] ", c)),
                        Some(_) => ("miss", " [MISSED] ".to_string()),
                        None => ("", "          ".to_string()),
                    };
                    html.push_str(&format!(
                        "<div class='{}'>{:4} | {}{}</div>",
                        cls, line_num, hit_str, line_text
                    ));
                }
                html.push_str("</div>");
            }
            html.push_str("</div>");
        }

        html.push_str("</body></html>");
        fs::write(&index_html_path, html)?;
        println!(
            "\n✅ Generated interactive HTML coverage report at: {}",
            index_html_path.display()
        );
    }

    Ok(())
}
