use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use sbpf_cov::dwarf::extract_dwarf_coverage;
use sbpf_cov::report::render_coverage_report;
use sbpf_cov::trace::ExecutionTrace;

#[derive(Parser, Debug)]
#[command(
    name = "sbpf-cov",
    about = "Standalone SBPF Instruction Trace & DWARF Source Coverage Tool"
)]
struct Cli {
    /// Path to compiled SBPF ELF binary (.so or .o with DWARF / debuginfo)
    #[arg(long, short)]
    elf: PathBuf,

    /// Path to execution trace JSON file containing executed PC addresses (optional)
    #[arg(long, short)]
    trace: Option<PathBuf>,

    /// Path to directory where interactive HTML report will be generated (optional)
    #[arg(long)]
    html: Option<PathBuf>,

    /// Path to output LCOV report file (optional)
    #[arg(long)]
    lcov: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=======================================================");
    println!("  sbpf-cov: SBPF Trace & DWARF Source Coverage Suite   ");
    println!("=======================================================\n");

    let trace = if let Some(trace_path) = &cli.trace {
        println!("Loading execution trace from {:?}...", trace_path);
        ExecutionTrace::load_from_file(trace_path)?
    } else {
        println!("No execution trace provided. Extracting static DWARF line table mapping...");
        ExecutionTrace::default()
    };

    println!("Extracting DWARF debug coverage from {:?}...", cli.elf);
    let summaries = extract_dwarf_coverage(&cli.elf, &trace)?;

    render_coverage_report(&summaries, cli.html.as_deref(), cli.lcov.as_deref())?;

    println!("✅ Coverage workflow complete!");
    Ok(())
}
