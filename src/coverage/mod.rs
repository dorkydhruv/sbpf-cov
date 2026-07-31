pub mod convert;
pub mod dwarf_cov;
pub mod fixup;
pub mod harness;
pub mod report;

pub use convert::{
    convert_dump_to_profraw, CoverageDump, ProgramCoverageDump,
};
pub use dwarf_cov::*;
pub use fixup::{fixup_sbpf_elf_for_vm, CoverageFixupMetadata};
pub use harness::CoverageTracker;
pub use report::{
    find_llvm_tool, find_target_elf, generate_coverage_report,
    merge_profraw_to_profdata,
};
