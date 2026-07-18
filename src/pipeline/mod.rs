//! Multi-role pipeline engine — configurable DAG-based execution.
//!
//! Pipelines are defined in `.agent/pipelines/*.toml` (per-project),
//! with roles referencing reusable definitions in `models.toml` (global).

pub mod dag;
pub mod deprecated;
pub mod loader;
pub mod runner;

// Re-export the old PipelineRunner for backward compatibility during migration.
pub use deprecated::PipelineRunner;
