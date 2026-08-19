use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Represents an execution trace captured during VM test execution (Mollusk / LiteSVM / Validator).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Set of executed Program Counter (PC) virtual addresses (e.g. 0x100000000, 0x100000008, ...)
    #[serde(default)]
    pub executed_pcs: HashSet<u64>,
    /// Program name or identifier
    #[serde(default)]
    pub program_name: Option<String>,
}

impl ExecutionTrace {
    /// Loads an execution trace from a JSON file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read trace file {:?}", path))?;

        // Attempt parsing as structured ExecutionTrace
        if let Ok(trace) = serde_json::from_str::<ExecutionTrace>(&content) {
            return Ok(trace);
        }

        // Fallback: parse direct JSON array of PC integers [0x100000000, ...]
        if let Ok(pcs) = serde_json::from_str::<Vec<u64>>(&content) {
            return Ok(ExecutionTrace {
                executed_pcs: pcs.into_iter().collect(),
                program_name: None,
            });
        }

        anyhow::bail!("Invalid execution trace format in {:?}", path);
    }

    /// Checks if a given PC address was executed.
    pub fn is_pc_executed(&self, pc: u64) -> bool {
        self.executed_pcs.contains(&pc)
    }

    /// Total number of unique instructions executed.
    pub fn unique_executed_count(&self) -> usize {
        self.executed_pcs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_parsing() {
        let json = r#"{"executed_pcs": [268435456, 268435464], "program_name": "test"}"#;
        let trace: ExecutionTrace = serde_json::from_str(json).unwrap();
        assert_eq!(trace.unique_executed_count(), 2);
        assert!(trace.is_pc_executed(268435456));
    }
}
