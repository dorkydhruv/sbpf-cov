use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::dwarf::FileCoverageSummary;

/// Escapes HTML special characters to prevent XSS / rendering issues.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Renders a clean terminal summary table and interactive HTML report.
pub fn render_coverage_report(
    summaries: &HashMap<PathBuf, FileCoverageSummary>,
    html_output_dir: Option<&Path>,
    lcov_output_file: Option<&Path>,
) -> Result<()> {
    if summaries.is_empty() {
        println!("No DWARF coverage records extracted.");
        return Ok(());
    }

    // Sort by file path for deterministic output across runs
    let mut sorted: Vec<_> = summaries.values().collect();
    sorted.sort_by_key(|s| &s.file_path);

    let mut total_lines = 0;
    let mut total_covered = 0;

    println!("Filename                                                       Lines    Missed Lines     Cover");
    println!("------------------------------------------------------------------------------------------------");

    for summary in &sorted {
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
        "{:<60} {:>8} {:>15} {:>8.2}%\n",
        "TOTAL",
        total_lines,
        total_lines.saturating_sub(total_covered),
        overall_percent
    );

    // Interactive HTML Report
    if let Some(output_dir) = html_output_dir {
        fs::create_dir_all(output_dir)?;
        let index_html_path = output_dir.join("index.html");

        let mut html = String::with_capacity(64 * 1024);
        html.push_str("<!DOCTYPE html><html><head><meta charset='utf-8'><title>SBPF Coverage Report</title>");
        html.push_str("<style>");
        html.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f172a; color: #f8fafc; padding: 2rem; }");
        html.push_str("h1 { color: #38bdf8; margin-bottom: 0.5rem; }");
        html.push_str(".card { background: #1e293b; border-radius: 12px; padding: 1.5rem; box-shadow: 0 4px 6px -1px rgba(0,0,0,0.3); margin-bottom: 2rem; }");
        html.push_str("table { width: 100%; border-collapse: collapse; margin-top: 1rem; }");
        html.push_str("th, td { text-align: left; padding: 12px 16px; border-bottom: 1px solid #334155; }");
        html.push_str("th { background: #0f172a; color: #94a3b8; font-weight: 600; }");
        html.push_str(".badge { background: #10b981; color: #022c22; font-weight: bold; padding: 4px 8px; border-radius: 6px; }");
        html.push_str(".source-code { font-family: monospace; background: #090d16; padding: 1rem; border-radius: 8px; overflow-x: auto; white-space: pre; }");
        html.push_str(".hit { background: #064e3b; color: #6ee7b7; }");
        html.push_str(".miss { background: #7f1d1d; color: #fca5a5; }");
        html.push_str("</style></head><body>");

        write!(
            html,
            "<div class='card'><h1>⚡ SBPF Program Code Coverage Report</h1>\
             <p>Overall Line Coverage: <span class='badge'>{:.2}%</span></p></div>",
            overall_percent
        )?;

        for summary in &sorted {
            write!(
                html,
                "<div class='card'><h2>📄 {}</h2>\
                 <p>Covered Lines: {} / {} ({:.2}%)</p>",
                html_escape(&summary.file_path.display().to_string()),
                summary.covered_lines,
                summary.total_executable_lines,
                summary.line_coverage_percent
            )?;

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
                    write!(
                        html,
                        "<div class='{}'>{:4} | {}{}</div>",
                        cls, line_num, hit_str, html_escape(line_text)
                    )?;
                }
                html.push_str("</div>");
            }
            html.push_str("</div>");
        }

        html.push_str("</body></html>");
        fs::write(&index_html_path, html)?;
        println!(
            "✅ Generated interactive HTML coverage report at: {}",
            index_html_path.display()
        );
    }

    // LCOV File Output
    if let Some(lcov_path) = lcov_output_file {
        let mut lcov = String::new();
        for summary in &sorted {
            write!(lcov, "TN:\nSF:{}\n", summary.file_path.display())?;
            for (&line, &hits) in &summary.line_hits {
                write!(lcov, "DA:{},{}\n", line, hits)?;
            }
            write!(lcov, "LF:{}\n", summary.total_executable_lines)?;
            write!(lcov, "LH:{}\n", summary.covered_lines)?;
            lcov.push_str("end_of_record\n");
        }
        fs::write(lcov_path, lcov)?;
        println!("✅ Exported LCOV coverage file to: {}", lcov_path.display());
    }

    Ok(())
}
