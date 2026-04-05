#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aggregates results from multiple operation steps
#[derive(Debug, Clone)]
pub struct ResultAggregator {
    /// Step results
    results: HashMap<usize, StepResult>,
    
    /// Aggregation strategy
    strategy: AggregationStrategy,
    
    /// Summary statistics
    statistics: AggregationStatistics,
}

/// Result of a step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step ID
    pub step_id: usize,
    
    /// Step name
    pub step_name: String,
    
    /// Whether step succeeded
    pub success: bool,
    
    /// Result message
    pub message: String,
    
    /// Error message (if any)
    pub error: Option<String>,
    
    /// Execution duration (in milliseconds)
    pub duration_ms: u64,
    
    /// Number of retry attempts
    pub retry_attempts: u32,
    
    /// Step output data
    pub output_data: HashMap<String, String>,
    
    /// Timestamp of execution start
    pub start_timestamp: i64,
    
    /// Timestamp of execution end
    pub end_timestamp: i64,
}

/// Aggregation strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggregationStrategy {
    /// All steps must succeed for overall success
    All,
    
    /// At least one step must succeed for overall success
    Any,
    
    /// Majority of steps must succeed for overall success
    Majority,
    
    /// Custom threshold (percentage of steps that must succeed)
    Threshold(f32),
    
    /// Weighted aggregation based on step importance
    Weighted,
}

/// Aggregation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    /// Overall success
    pub overall_success: bool,
    
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
    
    /// Total execution time (in milliseconds)
    pub total_duration_ms: u64,
    
    /// Average execution time per step (in milliseconds)
    pub avg_duration_ms: f64,
    
    /// Number of successful steps
    pub successful_steps: usize,
    
    /// Number of failed steps
    pub failed_steps: usize,
    
    /// Number of retried steps
    pub retried_steps: usize,
    
    /// Total retry attempts
    pub total_retry_attempts: u32,
    
    /// Error summary
    pub error_summary: Vec<String>,
    
    /// Step results summary
    pub step_summary: Vec<StepSummary>,
    
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
}

/// Step summary for aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSummary {
    /// Step ID
    pub step_id: usize,
    
    /// Step name
    pub step_name: String,
    
    /// Success status
    pub success: bool,
    
    /// Duration (in milliseconds)
    pub duration_ms: u64,
    
    /// Retry attempts
    pub retry_attempts: u32,
    
    /// Error message (if any)
    pub error: Option<String>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Minimum execution time (in milliseconds)
    pub min_duration_ms: u64,
    
    /// Maximum execution time (in milliseconds)
    pub max_duration_ms: u64,
    
    /// Median execution time (in milliseconds)
    pub median_duration_ms: u64,
    
    /// 95th percentile execution time (in milliseconds)
    pub p95_duration_ms: u64,
    
    /// Throughput (steps per second)
    pub throughput: f64,
    
    /// Success rate over time
    pub success_rate_over_time: Vec<(i64, f32)>,
}

/// Aggregation statistics
#[derive(Debug, Clone, Default)]
struct AggregationStatistics {
    /// Total steps
    total_steps: usize,
    
    /// Successful steps
    successful_steps: usize,
    
    /// Failed steps
    failed_steps: usize,
    
    /// Retried steps
    retried_steps: usize,
    
    /// Total retry attempts
    total_retry_attempts: u32,
    
    /// Total duration
    total_duration_ms: u64,
    
    /// Step durations
    step_durations: Vec<u64>,
    
    /// Errors
    errors: Vec<String>,
}

impl ResultAggregator {
    /// Create a new result aggregator
    pub fn new(strategy: AggregationStrategy) -> Self {
        Self {
            results: HashMap::new(),
            strategy,
            statistics: AggregationStatistics::default(),
        }
    }
    
    /// Add a step result
    pub fn add_result(&mut self, result: StepResult) {
        let step_id = result.step_id;
        
        // Update statistics
        self.statistics.total_steps += 1;
        
        if result.success {
            self.statistics.successful_steps += 1;
        } else {
            self.statistics.failed_steps += 1;
        }
        
        if result.retry_attempts > 0 {
            self.statistics.retried_steps += 1;
            self.statistics.total_retry_attempts += result.retry_attempts;
        }
        
        self.statistics.total_duration_ms += result.duration_ms;
        self.statistics.step_durations.push(result.duration_ms);
        
        if let Some(error) = &result.error {
            self.statistics.errors.push(error.clone());
        }
        
        // Store result
        self.results.insert(step_id, result);
    }
    
    /// Get aggregated result
    pub fn get_aggregated_result(&self) -> AggregationResult {
        let success_rate = if self.statistics.total_steps > 0 {
            self.statistics.successful_steps as f32 / self.statistics.total_steps as f32
        } else {
            0.0
        };
        
        let overall_success = match self.strategy {
            AggregationStrategy::All => self.statistics.failed_steps == 0,
            AggregationStrategy::Any => self.statistics.successful_steps > 0,
            AggregationStrategy::Majority => self.statistics.successful_steps > self.statistics.failed_steps,
            AggregationStrategy::Threshold(threshold) => success_rate >= threshold,
            AggregationStrategy::Weighted => {
                // Simple weighted strategy: give more weight to steps with longer duration
                let weighted_score: f64 = self.results.values()
                    .map(|result| {
                        let weight = result.duration_ms as f64 / self.statistics.total_duration_ms as f64;
                        if result.success { weight } else { 0.0 }
                    })
                    .sum();
                weighted_score > 0.5
            }
        };
        
        let avg_duration_ms = if self.statistics.total_steps > 0 {
            self.statistics.total_duration_ms as f64 / self.statistics.total_steps as f64
        } else {
            0.0
        };
        
        // Calculate performance metrics
        let performance_metrics = self.calculate_performance_metrics();
        
        // Create step summaries
        let mut step_summary: Vec<StepSummary> = self.results.values()
            .map(|result| StepSummary {
                step_id: result.step_id,
                step_name: result.step_name.clone(),
                success: result.success,
                duration_ms: result.duration_ms,
                retry_attempts: result.retry_attempts,
                error: result.error.clone(),
            })
            .collect();
        
        // Sort by step ID
        step_summary.sort_by_key(|s| s.step_id);
        
        AggregationResult {
            overall_success,
            success_rate,
            total_duration_ms: self.statistics.total_duration_ms,
            avg_duration_ms,
            successful_steps: self.statistics.successful_steps,
            failed_steps: self.statistics.failed_steps,
            retried_steps: self.statistics.retried_steps,
            total_retry_attempts: self.statistics.total_retry_attempts,
            error_summary: self.statistics.errors.clone(),
            step_summary,
            performance_metrics,
        }
    }
    
    /// Calculate performance metrics
    fn calculate_performance_metrics(&self) -> PerformanceMetrics {
        let mut durations = self.statistics.step_durations.clone();
        durations.sort_unstable();
        
        let min_duration_ms = durations.first().copied().unwrap_or(0);
        let max_duration_ms = durations.last().copied().unwrap_or(0);
        
        let median_duration_ms = if !durations.is_empty() {
            let mid = durations.len() / 2;
            if durations.len() % 2 == 0 {
                (durations[mid - 1] + durations[mid]) / 2
            } else {
                durations[mid]
            }
        } else {
            0
        };
        
        let p95_duration_ms = if !durations.is_empty() {
            let index = (durations.len() as f64 * 0.95).floor() as usize;
            durations[index.min(durations.len() - 1)]
        } else {
            0
        };
        
        let throughput = if self.statistics.total_duration_ms > 0 {
            self.statistics.total_steps as f64 / (self.statistics.total_duration_ms as f64 / 1000.0)
        } else {
            0.0
        };
        
        // Calculate success rate over time (simplified)
        let mut success_rate_over_time = Vec::new();
        let mut successful_so_far = 0;
        
        let mut sorted_results: Vec<&StepResult> = self.results.values().collect();
        sorted_results.sort_by_key(|r| r.end_timestamp);
        
        for (i, result) in sorted_results.iter().enumerate() {
            if result.success {
                successful_so_far += 1;
            }
            
            let rate = successful_so_far as f32 / (i + 1) as f32;
            success_rate_over_time.push((result.end_timestamp, rate));
        }
        
        PerformanceMetrics {
            min_duration_ms,
            max_duration_ms,
            median_duration_ms,
            p95_duration_ms,
            throughput,
            success_rate_over_time,
        }
    }
    
    /// Get result for a specific step
    pub fn get_step_result(&self, step_id: usize) -> Option<&StepResult> {
        self.results.get(&step_id)
    }
    
    /// Get all results
    pub fn get_all_results(&self) -> Vec<&StepResult> {
        self.results.values().collect()
    }
    
    /// Get success rate
    pub fn get_success_rate(&self) -> f32 {
        if self.statistics.total_steps > 0 {
            self.statistics.successful_steps as f32 / self.statistics.total_steps as f32
        } else {
            0.0
        }
    }
    
    /// Get error rate
    pub fn get_error_rate(&self) -> f32 {
        if self.statistics.total_steps > 0 {
            self.statistics.failed_steps as f32 / self.statistics.total_steps as f32
        } else {
            0.0
        }
    }
    
    /// Get retry rate
    pub fn get_retry_rate(&self) -> f32 {
        if self.statistics.total_steps > 0 {
            self.statistics.retried_steps as f32 / self.statistics.total_steps as f32
        } else {
            0.0
        }
    }
    
    /// Generate report in JSON format
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let result = self.get_aggregated_result();
        serde_json::to_string_pretty(&result)
    }
    
    /// Generate report in Markdown format
    pub fn to_markdown(&self) -> String {
        let result = self.get_aggregated_result();
        
        let mut md = String::new();
        md.push_str("# Batch Operation Results\n\n");
        
        md.push_str(&format!("## Summary\n\n"));
        md.push_str(&format!("- **Overall Success**: {}\n", result.overall_success));
        md.push_str(&format!("- **Success Rate**: {:.1}%\n", result.success_rate * 100.0));
        md.push_str(&format!("- **Total Duration**: {} ms\n", result.total_duration_ms));
        md.push_str(&format!("- **Successful Steps**: {}\n", result.successful_steps));
        md.push_str(&format!("- **Failed Steps**: {}\n", result.failed_steps));
        md.push_str(&format!("- **Retried Steps**: {}\n", result.retried_steps));
        
        md.push_str("\n## Performance Metrics\n\n");
        md.push_str(&format!("- **Min Duration**: {} ms\n", result.performance_metrics.min_duration_ms));
        md.push_str(&format!("- **Max Duration**: {} ms\n", result.performance_metrics.max_duration_ms));
        md.push_str(&format!("- **Median Duration**: {} ms\n", result.performance_metrics.median_duration_ms));
        md.push_str(&format!("- **95th Percentile**: {} ms\n", result.performance_metrics.p95_duration_ms));
        md.push_str(&format!("- **Throughput**: {:.2} steps/second\n", result.performance_metrics.throughput));
        
        if !result.error_summary.is_empty() {
            md.push_str("\n## Error Summary\n\n");
            for error in &result.error_summary {
                md.push_str(&format!("- {}\n", error));
            }
        }
        
        md.push_str("\n## Step Details\n\n");
        md.push_str("| Step ID | Step Name | Success | Duration (ms) | Retries | Error |\n");
        md.push_str("|---------|-----------|---------|---------------|---------|-------|\n");
        
        for step in &result.step_summary {
            let success_str = if step.success { "✅" } else { "❌" };
            let error_str = step.error.as_deref().unwrap_or("");
            md.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", 
                step.step_id, step.step_name, success_str, step.duration_ms, step.retry_attempts, error_str));
        }
        
        md
    }
}

impl Default for ResultAggregator {
    fn default() -> Self {
        Self::new(AggregationStrategy::All)
    }
}