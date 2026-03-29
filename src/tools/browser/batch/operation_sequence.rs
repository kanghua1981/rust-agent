use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A sequence of browser operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationSequence {
    /// Sequence name
    pub name: String,
    
    /// Sequence description
    pub description: Option<String>,
    
    /// Operation steps
    pub steps: Vec<OperationStep>,
    
    /// Maximum retry attempts per step
    pub max_retries: u32,
    
    /// Whether to stop on first failure
    pub stop_on_failure: bool,
    
    /// Timeout for the entire sequence (in seconds)
    pub timeout_seconds: Option<u64>,
    
    /// Variables for parameterization
    pub variables: HashMap<String, String>,
    
    /// Step dependencies
    pub dependencies: HashMap<usize, Vec<usize>>,
}

/// A single operation step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStep {
    /// Step ID (unique within sequence)
    pub id: usize,
    
    /// Step name
    pub name: String,
    
    /// Step description
    pub description: Option<String>,
    
    /// Action type (navigate, click, type, etc.)
    pub action_type: String,
    
    /// Action parameters
    pub parameters: HashMap<String, String>,
    
    /// Expected result pattern (for validation)
    pub expected_result: Option<String>,
    
    /// Timeout for this step (in seconds)
    pub timeout_seconds: Option<u64>,
    
    /// Whether step is optional
    pub optional: bool,
    
    /// Retry configuration
    pub retry_config: RetryConfig,
    
    /// Conditions for execution
    pub conditions: Vec<ExecutionCondition>,
}

/// Retry configuration for a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: u32,
    
    /// Delay between retries (in seconds)
    pub retry_delay_seconds: u64,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    
    /// Retry on specific error patterns
    pub retry_on_errors: Vec<String>,
}

/// Condition for step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCondition {
    /// Condition type (previous_step_success, variable_equals, element_exists, etc.)
    pub condition_type: String,
    
    /// Condition parameters
    pub parameters: HashMap<String, String>,
    
    /// Whether to negate the condition
    pub negate: bool,
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

impl OperationSequence {
    /// Create a new operation sequence
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            steps: Vec::new(),
            max_retries: 3,
            stop_on_failure: true,
            timeout_seconds: None,
            variables: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }
    
    /// Add a step to the sequence
    pub fn add_step(&mut self, step: OperationStep) {
        self.steps.push(step);
    }
    
    /// Set a variable value
    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }
    
    /// Get a variable value
    pub fn get_variable(&self, name: &str) -> Option<&String> {
        self.variables.get(name)
    }
    
    /// Add a dependency between steps
    pub fn add_dependency(&mut self, step_id: usize, depends_on: Vec<usize>) {
        self.dependencies.insert(step_id, depends_on);
    }
    
    /// Validate the sequence
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check sequence name
        if self.name.trim().is_empty() {
            errors.push("Sequence name cannot be empty".to_string());
        }
        
        // Check steps
        if self.steps.is_empty() {
            errors.push("Sequence must have at least one step".to_string());
        }
        
        // Check step IDs are unique
        let mut step_ids = std::collections::HashSet::new();
        for step in &self.steps {
            if !step_ids.insert(step.id) {
                errors.push(format!("Duplicate step ID: {}", step.id));
            }
        }
        
        // Check dependencies refer to valid step IDs
        for (step_id, dependencies) in &self.dependencies {
            if !step_ids.contains(step_id) {
                errors.push(format!("Dependency references non-existent step ID: {}", step_id));
            }
            
            for dep_id in dependencies {
                if !step_ids.contains(dep_id) {
                    errors.push(format!("Dependency references non-existent step ID: {}", dep_id));
                }
            }
        }
        
        // Check for circular dependencies
        if self.has_circular_dependencies() {
            errors.push("Circular dependencies detected in sequence".to_string());
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Check for circular dependencies
    fn has_circular_dependencies(&self) -> bool {
        use std::collections::{HashSet, VecDeque};
        
        let mut graph = HashMap::new();
        for (step_id, deps) in &self.dependencies {
            graph.entry(*step_id).or_insert_with(Vec::new).extend(deps);
        }
        
        // Add empty adjacency lists for steps without dependencies
        for step in &self.steps {
            graph.entry(step.id).or_insert_with(Vec::new);
        }
        
        // Kahn's algorithm for topological sorting
        let mut in_degree = HashMap::new();
        for (&node, neighbors) in &graph {
            in_degree.entry(node).or_insert(0);
            for &neighbor in neighbors {
                *in_degree.entry(neighbor).or_insert(0) += 1;
            }
        }
        
        let mut queue = VecDeque::new();
        for (&node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node);
            }
        }
        
        let mut visited = 0;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            
            if let Some(neighbors) = graph.get(&node) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        
        visited != graph.len()
    }
    
    /// Get step by ID
    pub fn get_step(&self, step_id: usize) -> Option<&OperationStep> {
        self.steps.iter().find(|step| step.id == step_id)
    }
    
    /// Get step execution order considering dependencies
    pub fn get_execution_order(&self) -> Vec<usize> {
        use std::collections::{HashSet, VecDeque};
        
        let mut graph = HashMap::new();
        for (step_id, deps) in &self.dependencies {
            graph.entry(*step_id).or_insert_with(Vec::new).extend(deps);
        }
        
        // Add empty adjacency lists for steps without dependencies
        for step in &self.steps {
            graph.entry(step.id).or_insert_with(Vec::new);
        }
        
        // Kahn's algorithm
        let mut in_degree = HashMap::new();
        for (&node, neighbors) in &graph {
            in_degree.entry(node).or_insert(0);
            for &neighbor in neighbors {
                *in_degree.entry(neighbor).or_insert(0) += 1;
            }
        }
        
        let mut queue = VecDeque::new();
        for (&node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node);
            }
        }
        
        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node);
            
            if let Some(neighbors) = graph.get(&node) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        
        order
    }
    
    /// Execute sequence sequentially
    pub async fn execute_sequential(&self) -> Vec<StepResult> {
        let mut results = Vec::new();
        
        for step in &self.steps {
            let start_time = chrono::Utc::now().timestamp();
            
            // TODO: Actually execute the step using browser actions
            // For now, create a mock result
            let result = StepResult {
                step_id: step.id,
                step_name: step.name.clone(),
                success: true, // Mock success
                message: format!("Executed step '{}'", step.name),
                error: None,
                duration_ms: 100, // Mock duration
                retry_attempts: 0,
                output_data: std::collections::HashMap::new(),
                start_timestamp: start_time,
                end_timestamp: start_time + 1,
            };
            
            results.push(result);
        }
        
        results
    }
    
    /// Execute sequence in parallel
    pub async fn execute_parallel(&self) -> Vec<StepResult> {
        let mut results = Vec::new();
        
        // TODO: Actually execute steps in parallel
        // For now, execute sequentially but mark as parallel
        for step in &self.steps {
            let start_time = chrono::Utc::now().timestamp();
            
            // TODO: Actually execute the step using browser actions
            // For now, create a mock result
            let result = StepResult {
                step_id: step.id,
                step_name: step.name.clone(),
                success: true, // Mock success
                message: format!("Executed step '{}' in parallel", step.name),
                error: None,
                duration_ms: 50, // Mock shorter duration for parallel
                retry_attempts: 0,
                output_data: std::collections::HashMap::new(),
                start_timestamp: start_time,
                end_timestamp: start_time + 1,
            };
            
            results.push(result);
        }
        
        results
    }
}

impl OperationStep {
    /// Create a new operation step
    pub fn new(id: usize, name: &str, action_type: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            description: None,
            action_type: action_type.to_string(),
            parameters: HashMap::new(),
            expected_result: None,
            timeout_seconds: None,
            optional: false,
            retry_config: RetryConfig::default(),
            conditions: Vec::new(),
        }
    }
    
    /// Add a parameter to the step
    pub fn add_parameter(&mut self, key: &str, value: &str) {
        self.parameters.insert(key.to_string(), value.to_string());
    }
    
    /// Add an execution condition
    pub fn add_condition(&mut self, condition: ExecutionCondition) {
        self.conditions.push(condition);
    }
    
    /// Check if step should execute based on conditions
    pub fn should_execute(&self, context: &HashMap<String, String>) -> bool {
        for condition in &self.conditions {
            let condition_result = self.evaluate_condition(condition, context);
            if condition.negate {
                if condition_result {
                    return false;
                }
            } else {
                if !condition_result {
                    return false;
                }
            }
        }
        true
    }
    
    /// Evaluate a single condition
    fn evaluate_condition(&self, condition: &ExecutionCondition, context: &HashMap<String, String>) -> bool {
        match condition.condition_type.as_str() {
            "variable_equals" => {
                if let Some(var_name) = condition.parameters.get("variable") {
                    if let Some(expected_value) = condition.parameters.get("value") {
                        if let Some(actual_value) = context.get(var_name) {
                            return actual_value == expected_value;
                        }
                    }
                }
                false
            }
            "variable_exists" => {
                if let Some(var_name) = condition.parameters.get("variable") {
                    return context.contains_key(var_name);
                }
                false
            }
            "previous_step_success" => {
                if let Some(step_id_str) = condition.parameters.get("step_id") {
                    if let Ok(step_id) = step_id_str.parse::<usize>() {
                        let result_key = format!("step_{}_success", step_id);
                        if let Some(result_value) = context.get(&result_key) {
                            return result_value == "true";
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
    
    /// Create a step from JSON value
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        // Try to parse as a full OperationStep struct
        if let Ok(step) = serde_json::from_value(value.clone()) {
            return Ok(step);
        }
        
        // If that fails, try to parse as a simplified format
        if let Some(obj) = value.as_object() {
            let id = obj.get("id")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "Missing or invalid 'id' field".to_string())? as usize;
            
            let name = obj.get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing or invalid 'name' field".to_string())?
                .to_string();
            
            let action_type = obj.get("action_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing or invalid 'action_type' field".to_string())?
                .to_string();
            
            let mut step = OperationStep::new(id, &name, &action_type);
            
            // Parse parameters
            if let Some(params) = obj.get("parameters") {
                if let Some(param_obj) = params.as_object() {
                    for (key, value) in param_obj {
                        if let Some(str_value) = value.as_str() {
                            step.add_parameter(key, str_value);
                        }
                    }
                }
            }
            
            // Parse optional fields
            if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                step.description = Some(desc.to_string());
            }
            
            if let Some(expected) = obj.get("expected_result").and_then(|v| v.as_str()) {
                step.expected_result = Some(expected.to_string());
            }
            
            if let Some(timeout) = obj.get("timeout_seconds").and_then(|v| v.as_u64()) {
                step.timeout_seconds = Some(timeout);
            }
            
            if let Some(optional) = obj.get("optional").and_then(|v| v.as_bool()) {
                step.optional = optional;
            }
            
            Ok(step)
        } else {
            Err("Expected JSON object for operation step".to_string())
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            retry_delay_seconds: 1,
            backoff_multiplier: 1.5,
            retry_on_errors: Vec::new(),
        }
    }
}