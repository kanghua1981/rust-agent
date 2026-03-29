//! Snapshot module for capturing structured page information
//! 
//! This module provides functionality for creating AI-friendly snapshots
//! of web pages, including ARIA information and semantic element roles.

mod ai_snapshot;
mod aria_snapshot;
mod element_roles;

pub use ai_snapshot::AiSnapshot;
pub use aria_snapshot::AriaSnapshot;
pub use element_roles::ElementRoles;