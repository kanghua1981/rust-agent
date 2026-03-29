//! Batch operations module for executing sequences of browser actions
//! 
//! This module provides functionality for defining, managing, and executing
//! sequences of browser operations with dependency management and result aggregation.

mod operation_sequence;
mod dependency_manager;
pub mod result_aggregator;

pub use operation_sequence::{OperationSequence, OperationStep, StepResult};
pub use dependency_manager::{DependencyManager, DependencyGraph, Dependency};
pub use result_aggregator::{ResultAggregator, AggregationResult, AggregationStrategy};