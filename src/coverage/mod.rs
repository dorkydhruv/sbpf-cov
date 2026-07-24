pub mod convert;
pub mod fixup;

pub use convert::{convert_dump_to_profraw, CoverageDump, ProgramCoverageDump};
pub use fixup::{fixup_sbpf_elf_for_vm, CoverageFixupMetadata};
