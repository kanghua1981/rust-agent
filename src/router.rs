//! Execution mode selection.
//!
//! The pipeline router was retired; the model drives orchestration via plan mode
//! and sub-agents. This module keeps the ExecutionMode value the CLI and the
//! worker's set_mode still carry for compatibility. Only BasicLoop is reached at
//! runtime; PlanAndExecute / FullPipeline are retained as inert values.

use std::fmt;

/// The execution mode value carried by force_mode / set_mode.
/// The retired pipeline modes are kept as inert values for compatibility; the
/// agent loop treats every variation as the basic loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Single-model loop — always used at runtime.
    BasicLoop,
    /// Retired: plan+execute pipeline value (no-op).
    PlanAndExecute,
    /// Retired: full pipeline value (no-op).
    FullPipeline,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionMode::BasicLoop => write!(f, "Basic Loop"),
            ExecutionMode::PlanAndExecute => write!(f, "Plan + Execute"),
            ExecutionMode::FullPipeline => write!(f, "Full Pipeline"),
        }
    }
}
