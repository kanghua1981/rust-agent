//! Execution mode selection.
//!
//! The router was retired; the model drives orchestration via plan mode
//! (the `plan_mode` flag) and sub-agents. This module keeps the single
//! `ExecutionMode::BasicLoop` value the CLI and worker's `set_mode` still carry.
//! Runtime mode is always the basic loop; whether to plan first is the separate
//! `plan_mode` flag.

use std::fmt;

/// The execution mode carried by `force_mode` / `set_mode`.
/// `BasicLoop` is the only mode reached at runtime; planning is driven by the
/// agent's `plan_mode` flag, not by this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Single-model loop — always used at runtime.
    BasicLoop,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Basic Loop")
    }
}