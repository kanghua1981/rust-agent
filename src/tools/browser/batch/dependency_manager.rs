use std::collections::{HashMap, HashSet};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;

/// Manages dependencies between operations
#[derive(Debug, Clone)]
pub struct DependencyManager {
    /// Dependency graph
    graph: DiGraph<usize, ()>,
    
    /// Node index to step ID mapping
    node_to_step: HashMap<NodeIndex, usize>,
    
    /// Step ID to node index mapping
    step_to_node: HashMap<usize, NodeIndex>,
}

/// Dependency between operations
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Dependent step ID
    pub dependent: usize,
    
    /// Dependency step ID
    pub dependency: usize,
    
    /// Dependency type
    pub dependency_type: DependencyType,
}

/// Type of dependency
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    /// Step must complete successfully before dependent can start
    Success,
    
    /// Step must complete (regardless of success) before dependent can start
    Completion,
    
    /// Step output is used as input for dependent
    Data,
    
    /// Steps must run on same page/tab
    SamePage,
    
    /// Steps must run in sequence (no parallelism)
    Sequential,
}

/// Dependency graph for visualization
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Nodes (step IDs)
    pub nodes: Vec<usize>,
    
    /// Edges (dependencies)
    pub edges: Vec<(usize, usize, DependencyType)>,
    
    /// Graph cycles (if any)
    pub cycles: Vec<Vec<usize>>,
}

impl DependencyManager {
    /// Create a new dependency manager
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_to_step: HashMap::new(),
            step_to_node: HashMap::new(),
        }
    }
    
    /// Add a step to the dependency graph
    pub fn add_step(&mut self, step_id: usize) {
        if !self.step_to_node.contains_key(&step_id) {
            let node = self.graph.add_node(step_id);
            self.node_to_step.insert(node, step_id);
            self.step_to_node.insert(step_id, node);
        }
    }
    
    /// Add a dependency between steps
    pub fn add_dependency(&mut self, dependent: usize, dependency: usize, dependency_type: DependencyType) -> Result<(), String> {
        // Ensure both steps exist
        if !self.step_to_node.contains_key(&dependent) {
            return Err(format!("Dependent step {} not found", dependent));
        }
        
        if !self.step_to_node.contains_key(&dependency) {
            return Err(format!("Dependency step {} not found", dependency));
        }
        
        // Get node indices
        let dependent_node = self.step_to_node[&dependent];
        let dependency_node = self.step_to_node[&dependency];
        
        // Add edge (dependency -> dependent)
        self.graph.add_edge(dependency_node, dependent_node, ());
        
        Ok(())
    }
    
    /// Remove a dependency
    pub fn remove_dependency(&mut self, dependent: usize, dependency: usize) -> Result<(), String> {
        if let (Some(dependent_node), Some(dependency_node)) = (
            self.step_to_node.get(&dependent),
            self.step_to_node.get(&dependency),
        ) {
            // Find and remove the edge
            if let Some(edge) = self.graph.find_edge(*dependency_node, *dependent_node) {
                self.graph.remove_edge(edge);
            }
        }
        
        Ok(())
    }
    
    /// Check if there are circular dependencies
    pub fn has_circular_dependencies(&self) -> bool {
        match toposort(&self.graph, None) {
            Ok(_) => false,
            Err(_) => true,
        }
    }
    
    /// Get all circular dependencies
    pub fn get_circular_dependencies(&self) -> Vec<Vec<usize>> {
        use petgraph::algo::kosaraju_scc;
        
        let sccs = kosaraju_scc(&self.graph);
        
        sccs.into_iter()
            .filter(|scc| scc.len() > 1) // Only keep cycles (SCCs with more than one node)
            .map(|scc| {
                scc.into_iter()
                    .map(|node| self.node_to_step[&node])
                    .collect()
            })
            .collect()
    }
    
    /// Get topological order of steps
    pub fn get_topological_order(&self) -> Result<Vec<usize>, String> {
        match toposort(&self.graph, None) {
            Ok(order) => {
                let step_order: Vec<usize> = order
                    .into_iter()
                    .map(|node| self.node_to_step[&node])
                    .collect();
                Ok(step_order)
            }
            Err(cycle) => {
                let cycle_node = cycle.node_id();
                let cycle_step = self.node_to_step[&cycle_node];
                Err(format!("Circular dependency detected involving step {}", cycle_step))
            }
        }
    }
    
    /// Get steps that depend on a given step
    pub fn get_dependents(&self, step_id: usize) -> Vec<usize> {
        if let Some(node) = self.step_to_node.get(&step_id) {
            self.graph.neighbors_directed(*node, petgraph::Direction::Outgoing)
                .map(|neighbor| self.node_to_step[&neighbor])
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Get steps that a given step depends on
    pub fn get_dependencies(&self, step_id: usize) -> Vec<usize> {
        if let Some(node) = self.step_to_node.get(&step_id) {
            self.graph.neighbors_directed(*node, petgraph::Direction::Incoming)
                .map(|neighbor| self.node_to_step[&neighbor])
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Check if a step can be executed (all dependencies satisfied)
    pub fn can_execute(&self, step_id: usize, completed_steps: &HashSet<usize>) -> bool {
        let dependencies = self.get_dependencies(step_id);
        dependencies.iter().all(|dep| completed_steps.contains(dep))
    }
    
    /// Get steps that are ready to execute
    pub fn get_ready_steps(&self, completed_steps: &HashSet<usize>) -> Vec<usize> {
        let mut ready_steps = Vec::new();
        
        for step_id in self.step_to_node.keys() {
            if !completed_steps.contains(step_id) && self.can_execute(*step_id, completed_steps) {
                ready_steps.push(*step_id);
            }
        }
        
        ready_steps
    }
    
    /// Get the dependency graph for visualization
    pub fn get_graph(&self) -> DependencyGraph {
        let nodes: Vec<usize> = self.node_to_step.values().copied().collect();
        
        let mut edges = Vec::new();
        for edge in self.graph.edge_indices() {
            let (source, target) = self.graph.edge_endpoints(edge).unwrap();
            let source_step = self.node_to_step[&source];
            let target_step = self.node_to_step[&target];
            
            // For now, we don't track dependency types in the graph
            // This could be extended if needed
            edges.push((source_step, target_step, DependencyType::Success));
        }
        
        let cycles = self.get_circular_dependencies();
        
        DependencyGraph {
            nodes,
            edges,
            cycles,
        }
    }
    
    /// Get steps that have no dependencies
    pub fn get_root_steps(&self) -> Vec<usize> {
        self.step_to_node.keys()
            .filter(|&&step_id| self.get_dependencies(step_id).is_empty())
            .copied()
            .collect()
    }
    
    /// Get steps that have no dependents (leaf steps)
    pub fn get_leaf_steps(&self) -> Vec<usize> {
        self.step_to_node.keys()
            .filter(|&&step_id| self.get_dependents(step_id).is_empty())
            .copied()
            .collect()
    }
    
    /// Get the critical path (longest path) through the dependency graph
    pub fn get_critical_path(&self) -> Vec<usize> {
        // Simple implementation - returns topological order
        // In a real implementation, this would consider step durations
        self.get_topological_order().unwrap_or_default()
    }
}

impl Default for DependencyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    /// Convert to DOT format for visualization
    pub fn to_dot(&self) -> String {
        let mut dot = String::new();
        dot.push_str("digraph G {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box];\n\n");
        
        // Add nodes
        for node in &self.nodes {
            dot.push_str(&format!("  step{} [label=\"Step {}\"];\n", node, node));
        }
        
        dot.push_str("\n");
        
        // Add edges
        for (source, target, dep_type) in &self.edges {
            let style = match dep_type {
                DependencyType::Success => "solid",
                DependencyType::Completion => "dashed",
                DependencyType::Data => "dotted",
                DependencyType::SamePage => "bold",
                DependencyType::Sequential => "solid",
            };
            
            dot.push_str(&format!("  step{} -> step{} [style={}];\n", source, target, style));
        }
        
        // Highlight cycles
        if !self.cycles.is_empty() {
            dot.push_str("\n  // Cycles\n");
            dot.push_str("  edge [color=red];\n");
            
            for cycle in &self.cycles {
                for i in 0..cycle.len() {
                    let source = cycle[i];
                    let target = cycle[(i + 1) % cycle.len()];
                    dot.push_str(&format!("  step{} -> step{};\n", source, target));
                }
            }
        }
        
        dot.push_str("}\n");
        dot
    }
}