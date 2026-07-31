use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Parser;
use sbpf_cov::coverage::{
    convert_dump_to_profraw, fixup_sbpf_elf_for_vm, generate_coverage_report,
    merge_profraw_to_profdata,
};

#[derive(Parser, Debug)]
#[command(
    name = "sbpf-cov",
    author,
    version,
    about = "Zero-runtime LLVM source coverage toolchain for Solana SBPF programs"
)]
struct Cli {
    /// Raw linked SBPF ELF file to post-process via fixup
    #[arg(long, alias = "input")]
    raw_elf: Option<PathBuf>,

    /// Fixed VM-ready output SBPF ELF file path
    #[arg(long, alias = "output")]
    fixed_elf: Option<PathBuf>,

    /// Export interactive HTML coverage report to directory
    #[arg(long)]
    html: Option<PathBuf>,

    /// Path to coverage dump JSON file (auto-detected if omitted)
    #[arg(long)]
    dump: Option<PathBuf>,

    /// Path to instrumented SBPF ELF / object file (.o or .so, auto-detected if omitted)
    #[arg(long)]
    elf: Option<PathBuf>,

    /// Output path for .profraw file
    #[arg(long, default_value = "default.profraw")]
    profraw: PathBuf,

    /// Output path for merged .profdata file
    #[arg(long, default_value = "output.profdata")]
    profdata: PathBuf,

    /// Export LCOV format file for CI/CD
    #[arg(long)]
    lcov: Option<PathBuf>,

    /// Path to Cargo.toml manifest for test execution
    #[arg(long)]
    manifest_path: Option<PathBuf>,

    /// Skip running cargo test harness step
    #[arg(long)]
    skip_test: bool,
}

fn main() -> Result<()> {
    let mut raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() > 1
        && (raw_args[1] == "sbpf-cov" || raw_args[1] == "sbpf_cov")
    {
        raw_args.remove(1);
    }
    let cli = Cli::parse_from(raw_args);

    println!("=======================================================");
    println!("   sbpf-cov: Solana SBPF Zero-Runtime Coverage Suite  ");
    println!("=======================================================");

    // Step 1: Fixup raw SBPF ELF for VM execution if --raw-elf and --fixed-elf are specified
    if let (Some(raw), Some(fixed)) = (&cli.raw_elf, &cli.fixed_elf) {
        println!("\n[1/4] Fixing up raw SBPF ELF {:?} -> {:?}", raw, fixed);
        let meta = fixup_sbpf_elf_for_vm(raw, fixed)
            .map_err(|e| anyhow::anyhow!("{:#}", e))?;
        println!("✅ Generated VM-ready SBPF ELF at {:?}", fixed);
        println!(
            "   Counter offset in .rodata: {} bytes",
            meta.counter_offset_in_rodata
        );
    } else {
        println!("\n[1/4] Fixup phase: Using existing SBPF ELF binary");
    }

    // Step 2: Resolve Coverage Dump path & run VM integration tests if not skipped
    let dump_path = if let Some(d) = &cli.dump {
        d.clone()
    } else {
        PathBuf::from("coverage_dump.json")
    };

    if !cli.skip_test && cli.manifest_path.is_some() {
        println!("\n[2/4] Executing VM test suite under dynamic coverage interposer...");
        run_cargo_test_under_interposer(
            cli.manifest_path.as_deref(),
            &dump_path,
        )?;
    } else {
        println!(
            "\n[2/4] Test execution phase: Using existing coverage dump JSON"
        );
    }

    let resolved_dump = if dump_path.exists() {
        dump_path
    } else if std::path::Path::new("/tmp/c_coverage_dump.json").exists() {
        PathBuf::from("/tmp/c_coverage_dump.json")
    } else if std::path::Path::new("/tmp/rust_coverage_dump.json").exists() {
        PathBuf::from("/tmp/rust_coverage_dump.json")
    } else if std::path::Path::new("/tmp/java_coverage_dump.json").exists() {
        PathBuf::from("/tmp/java_coverage_dump.json")
    } else {
        bail!("Could not locate coverage dump JSON file. Please specify --dump <path>");
    };

    // Step 3: Resolve Target ELF / Object file and convert dump to .profraw
    let elf_path = if let Some(e) = &cli.elf {
        e.clone()
    } else {
        find_target_elf(cli.manifest_path.as_deref())?
    };

    println!(
        "\n[3/4] Converting coverage dump {:?} to LLVM .profraw format...",
        resolved_dump
    );
    convert_dump_to_profraw(&resolved_dump, &elf_path, &cli.profraw)
        .map_err(|e| anyhow::anyhow!("{:#}", e))?;
    println!("✅ Generated {:?}", cli.profraw);

    // Step 4: Merge .profraw -> .profdata and generate reports
    println!("\n[4/4] Merging raw profile data and generating reports...");
    merge_profraw_to_profdata(&cli.profraw, &cli.profdata)
        .map_err(|e| anyhow::anyhow!("{:#}", e))?;
    generate_coverage_report(
        &elf_path,
        &cli.profdata,
        None,
        cli.html.as_deref(),
        cli.lcov.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!("{:#}", e))?;

    println!("\n✅ SBPF coverage report generation complete!");
    Ok(())
}

fn run_cargo_test_under_interposer(
    manifest_path: Option<&std::path::Path>,
    dump_path: &std::path::Path,
) -> Result<()> {
    let interposer_lib = if cfg!(target_os = "macos") {
        "target/debug/libsbpf_cov_interposer.dylib"
    } else {
        "target/debug/libsbpf_cov_interposer.so"
    };

    let mut test_cmd = Command::new("cargo");
    test_cmd.arg("test");
    if let Some(mp) = manifest_path {
        test_cmd.arg("--manifest-path").arg(mp);
    }
    test_cmd.arg("--").arg("--nocapture");

    if cfg!(target_os = "macos") {
        test_cmd.env("DYLD_INSERT_LIBRARIES", interposer_lib);
    } else {
        test_cmd.env("LD_PRELOAD", interposer_lib);
    }
    test_cmd.env("SBPF_COV_DUMP_PATH", dump_path);

    let status = test_cmd
        .status()
        .context("Failed to execute cargo test under interposer")?;
    if !status.success() {
        println!("⚠️ Test suite reported status {:?} (proceeding with report generation)", status);
    }
    Ok(())
}

fn find_target_elf(
    manifest_path: Option<&std::path::Path>,
) -> Result<PathBuf> {
    let base_dir = manifest_path
        .and_then(|p| p.parent())
        .unwrap_or_else(|| std::path::Path::new("."));

    let candidates = [
        base_dir.join("target/c_program.o"),
        base_dir.join("target/rust_program.o"),
        base_dir.join("target/java_program.o"),
        base_dir.join("examples/c-example/target/c_program.o"),
        base_dir.join("examples/rust-example/target/rust_program.o"),
        base_dir.join("examples/java-example/target/java_program.o"),
        base_dir.join("target/deploy/bpf_prog.so"),
        base_dir.join("target/sbpf-solana-solana/release/bpf_prog.so"),
        std::path::PathBuf::from("target/deploy/bpf_prog.so"),
        std::path::PathBuf::from("/tmp/bpf_prog_instrumented.so"),
    ];

    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    if base_dir.join("target").exists() {
        for entry in walkdir::WalkDir::new(base_dir.join("target"))
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let file_name =
                    path.file_name().unwrap_or_default().to_string_lossy();
                if (ext == "so" || ext == "o")
                    && !file_name.contains("interposer")
                    && !file_name.starts_with("lib")
                {
                    return Ok(path.to_path_buf());
                }
            }
        }
    }

    bail!("Could not locate SBPF ELF (.so) or object (.o) file for coverage reporting")
}
