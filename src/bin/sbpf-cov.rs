use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sbpf_cov::coverage::{convert_dump_to_profraw, fixup_sbpf_elf_for_vm};

#[derive(Parser, Debug)]
#[command(
    name = "sbpf-cov",
    author,
    version,
    about = "Zero-runtime LLVM source coverage toolchain for Solana SBPF programs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Post-processes an instrumented SBPF ELF for solana_rbpf VM execution (embeds counters in .rodata)
    Fixup {
        /// Input instrumented SBPF ELF (.so)
        #[arg(short, long)]
        input: PathBuf,

        /// Output VM-ready SBPF ELF (.so)
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Converts intermediate coverage dump JSON + SBPF ELF into standard LLVM .profraw (Version 10 spec)
    Convert {
        /// Path to coverage dump JSON
        #[arg(short, long)]
        dump: PathBuf,

        /// Path to original SBPF ELF (.so) containing __llvm_prf_data and __llvm_prf_names
        #[arg(short, long)]
        elf: PathBuf,

        /// Output path for .profraw file
        #[arg(short, long, default_value = "default.profraw")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Fixup { input, output } => {
            println!("Fixing up SBPF ELF {:?} for VM execution...", input);
            let meta = fixup_sbpf_elf_for_vm(&input, &output)?;
            println!("✅ Generated VM-ready ELF at {:?}", output);
            println!("   Counter offset in .rodata: {} bytes", meta.counter_offset_in_rodata);
            println!("   Total counter count:       {}", meta.num_counters);
        }
        Commands::Convert { dump, elf, output } => {
            println!("Converting coverage dump {:?} and ELF {:?} to profraw...", dump, elf);
            let size = convert_dump_to_profraw(&dump, &elf, &output)?;
            println!("✅ Successfully generated .profraw file at {:?} ({} bytes)", output, size);
            println!("\nNext steps:");
            println!("  llvm-profdata merge -o output.profdata {:?}", output);
            println!("  llvm-cov report <binary> -instr-profile=output.profdata");
        }
    }

    Ok(())
}
