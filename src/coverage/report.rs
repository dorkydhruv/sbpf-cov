use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the path to an LLVM binary (e.g. llvm-profdata, llvm-cov)
pub fn find_llvm_tool(name: &str) -> Result<PathBuf> {
    // Check PATH first
    if let Ok(path) = which::which(name) {
        return Ok(path);
    }

    // Check xcrun on macOS
    if cfg!(target_os = "macos") {
        if let Ok(output) =
            Command::new("xcrun").arg("-find").arg(name).output()
        {
            if output.status.success() {
                let path_str =
                    String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    return Ok(PathBuf::from(path_str));
                }
            }
        }
    }

    // Check Solana platform-tools cache
    if let Ok(home) = std::env::var("HOME") {
        let platform_tools = PathBuf::from(home)
            .join(".cache/solana/v1.51/platform-tools/llvm/bin")
            .join(name);
        if platform_tools.exists() {
            return Ok(platform_tools);
        }
    }

    bail!("Could not find LLVM tool `{}`. Please install LLVM or ensure it is in PATH.", name);
}

/// Discovers compiled SBPF ELF (.so) or object (.o) files in the target directory
pub fn find_target_elf(manifest_path: Option<&Path>) -> Result<PathBuf> {
    let mut target_so_name = String::from("bpf_prog.so");

    if let Some(mp) = manifest_path {
        if mp.exists() {
            if let Ok(content) = std::fs::read_to_string(mp) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("name =") {
                        let name_val = trimmed
                            .trim_start_matches("name =")
                            .trim()
                            .trim_matches('"');
                        let snake_name = name_val.replace('-', "_");
                        target_so_name = format!("{}.so", snake_name);
                        break;
                    }
                }
            }
        }

        let base_dir = mp.parent().unwrap_or_else(|| Path::new("."));
        let candidates = [
            base_dir.join("target/deploy").join(&target_so_name),
            base_dir
                .join("target/sbpf-solana-solana/release")
                .join(&target_so_name),
            PathBuf::from("target/deploy").join(&target_so_name),
        ];

        for c in &candidates {
            if c.exists() {
                return Ok(c.clone());
            }
        }

        let target_dir = base_dir.join("target");
        if target_dir.exists() {
            for entry in walkdir::WalkDir::new(target_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "so" || ext == "o" {
                        let file_name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        if !file_name.contains("interposer")
                            && !file_name.starts_with("lib")
                        {
                            return Ok(path.to_path_buf());
                        }
                    }
                }
            }
        }

        bail!("Could not locate SBPF ELF (.so) or object (.o) file for manifest {:?}", mp);
    }

    // Workspace fallback search when manifest_path is None
    if Path::new("target").exists() {
        for entry in
            walkdir::WalkDir::new("target").into_iter().filter_map(|e| e.ok())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "so" || ext == "o" {
                    let file_name =
                        path.file_name().unwrap_or_default().to_string_lossy();
                    if !file_name.contains("interposer")
                        && !file_name.starts_with("lib")
                    {
                        return Ok(path.to_path_buf());
                    }
                }
            }
        }
    }

    let fallback_tmp = PathBuf::from("/tmp/bpf_prog_instrumented.so");
    if fallback_tmp.exists() {
        return Ok(fallback_tmp);
    }

    bail!("Could not locate SBPF ELF (.so) or object (.o) file")
}

pub fn merge_profraw_to_profdata(
    profraw_path: &Path,
    profdata_path: &Path,
) -> Result<()> {
    let profdata_bin = find_llvm_tool("llvm-profdata")?;
    let status = Command::new(&profdata_bin)
        .arg("merge")
        .arg("-o")
        .arg(profdata_path)
        .arg(profraw_path)
        .status()
        .with_context(|| format!("Failed to execute {:?}", profdata_bin))?;

    if !status.success() {
        bail!("llvm-profdata failed with status {:?}", status);
    }
    Ok(())
}

pub fn generate_coverage_report(
    elf_path: &Path,
    profdata_path: &Path,
    source_path: Option<&Path>,
    html_dir: Option<&Path>,
    lcov_file: Option<&Path>,
) -> Result<()> {
    let cov_bin = find_llvm_tool("llvm-cov")?;

    if let Some(html_dir) = html_dir {
        let mut cmd = Command::new(&cov_bin);
        cmd.arg("show")
            .arg(elf_path)
            .arg(format!("-instr-profile={}", profdata_path.display()))
            .arg(format!("-output-dir={}", html_dir.display()))
            .arg("-format=html");
        if let Some(src) = source_path {
            cmd.arg(src);
        }
        let status = cmd.status();
        if status.is_err() || !status.as_ref().unwrap().success() {
            // Fall back to DWARF line coverage report generator for HTML
            super::dwarf_cov::render_dwarf_coverage_report(
                elf_path,
                &[1],
                Some(html_dir),
            )?;
        } else {
            println!(
                "✅ Generated interactive HTML coverage report in {:?}",
                html_dir
            );
        }
    } else if let Some(lcov_file) = lcov_file {
        let mut cmd = Command::new(&cov_bin);
        cmd.arg("export")
            .arg(elf_path)
            .arg(format!("-instr-profile={}", profdata_path.display()))
            .arg("-format=lcov");
        if let Some(src) = source_path {
            cmd.arg(src);
        }
        let output = cmd.output()?;
        if !output.status.success() {
            bail!("llvm-cov lcov export failed");
        }
        std::fs::write(lcov_file, output.stdout)?;
        println!("✅ Exported LCOV coverage file to {:?}", lcov_file);
    } else {
        // Terminal summary report
        let mut cmd = Command::new(&cov_bin);
        cmd.arg("report")
            .arg(elf_path)
            .arg(format!("-instr-profile={}", profdata_path.display()));
        if let Some(src) = source_path {
            cmd.arg(src);
        }
        let status = cmd.status();
        if status.is_err() || !status.as_ref().unwrap().success() {
            // Fall back to DWARF line coverage report + llvm-profdata show for Rust programs
            super::dwarf_cov::render_dwarf_coverage_report(
                elf_path,
                &[1],
                None,
            )?;

            let profdata_bin = find_llvm_tool("llvm-profdata")?;
            let output = Command::new(&profdata_bin)
                .arg("show")
                .arg("--all-functions")
                .arg(profdata_path)
                .output()?;
            if output.status.success() {
                println!("\n=======================================================");
                println!("  Rust SBPF Program LLVM Coverage Profile Summary ");
                println!(
                    "======================================================="
                );
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
        }
    }

    Ok(())
}
