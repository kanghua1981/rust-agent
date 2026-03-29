//! Main browser tool module

pub mod browser_tool;
pub mod client;
pub mod config;
pub mod error;
pub mod runtime;
pub mod snapshot;
pub mod batch;
pub mod driver;
pub mod protocol;

#[cfg(test)]
mod test;

pub use browser_tool::*;