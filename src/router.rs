//! Adaptive task router.
//!
//! Classifies user input into complexity levels and decides which
//! execution mode to use:
//!
//!   - **Simple**:  Basic Loop (single model, fast response)
//!   - **Medium**:  Plan + Execute (two-stage, no Checker)
//!   - **Complex**: Full Pipeline (Planner → Executor → Checker)
//!
//! Classification uses a two-tier strategy:
//!   1. **Rule-based heuristics** — fast, zero-cost, catches obvious cases.
//!   2. **LLM-based classification** — optional, uses a lightweight model call
//!      when heuristics are inconclusive.
//!
//! Enable adaptive routing in `models.toml`:
//! ```toml
//! [pipeline]
//! enabled = true
//! router = "auto"   # "auto" | "always_pipeline" | "always_simple"
//! ```

use std::fmt;

/// The complexity level of a user task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple question / explanation / single-file change.
    /// → Basic Loop (single model, no pipeline overhead).
    Simple,
    /// Multi-file change, refactoring, feature addition.
    /// → Plan + Execute (two-stage, skip Checker).
    Medium,
    /// Large-scale migration, cross-cutting changes, architecture work.
    /// → Full Pipeline (Planner → Executor → Checker).
    Complex,
}

impl fmt::Display for TaskComplexity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskComplexity::Simple => write!(f, "Simple"),
            TaskComplexity::Medium => write!(f, "Medium"),
            TaskComplexity::Complex => write!(f, "Complex"),
        }
    }
}

/// The execution mode chosen by the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Single-model loop — fast, low cost.
    BasicLoop,
    /// Plan then Execute — two stages, no verification.
    PlanAndExecute,
    /// Full three-stage pipeline with Checker feedback loop.
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

impl From<TaskComplexity> for ExecutionMode {
    fn from(complexity: TaskComplexity) -> Self {
        match complexity {
            TaskComplexity::Simple => ExecutionMode::BasicLoop,
            TaskComplexity::Medium => ExecutionMode::PlanAndExecute,
            TaskComplexity::Complex => ExecutionMode::FullPipeline,
        }
    }
}

// ── Rule-based heuristic classifier ──────────────────────────────────

/// Keyword / pattern groups used by the heuristic classifier.
///
/// Each group has a set of patterns and the score delta it contributes.
/// Positive scores push towards Complex, negative towards Simple.
struct HeuristicRule {
    patterns: &'static [&'static str],
    score: i32,
}

/// Rules that indicate a **simple** task (negative score).
const SIMPLE_RULES: &[HeuristicRule] = &[
    // Direct questions / explanations
    HeuristicRule {
        patterns: &[
            "explain", "what is", "what does", "how does", "why does",
            "tell me", "describe", "show me", "what are", "can you explain",
            "help me understand", "what's the difference",
            "解释", "什么是", "是什么", "为什么", "怎么回事",
            "告诉我", "描述", "区别",
        ],
        score: -3,
    },
    // Simple lookups / reads
    HeuristicRule {
        patterns: &[
            "read", "find", "search", "look for", "grep", "list files",
            "show the file", "cat ", "print",
            "读", "查找", "搜索", "看看", "找一下",
        ],
        score: -2,
    },
    // Simple single-file operations
    HeuristicRule {
        patterns: &[
            "fix this bug", "fix the error", "fix the typo",
            "add a comment", "rename", "format",
            "修复", "改个名", "加个注释", "格式化",
        ],
        score: -1,
    },
];

/// Rules that indicate a **complex** task (positive score).
const COMPLEX_RULES: &[HeuristicRule] = &[
    // Multi-file / cross-cutting operations
    HeuristicRule {
        patterns: &[
            "refactor", "restructure", "reorganize", "migrate",
            "all files", "across the project", "throughout",
            "重构", "重组", "迁移", "所有文件", "整个项目",
        ],
        score: 3,
    },
    // Architecture / design work
    HeuristicRule {
        patterns: &[
            "implement", "build", "create a new feature", "design",
            "add support for", "integrate", "set up",
            "实现", "构建", "新功能", "设计", "集成", "搭建",
        ],
        score: 2,
    },
    // Multi-step explicit requests
    HeuristicRule {
        patterns: &[
            "step by step", "first ", "then ", "after that",
            "multiple files", "several changes",
            "分步", "首先", "然后", "接着", "多个文件",
        ],
        score: 2,
    },
    // Testing & verification requests (suggests need for Checker)
    HeuristicRule {
        patterns: &[
            "and test", "and verify", "make sure", "ensure",
            "run the tests", "validate",
            "并测试", "确保", "验证",
        ],
        score: 1,
    },
];

/// Classify a user message using rule-based heuristics.
///
/// Returns `Some(complexity)` if confident, `None` if inconclusive
/// (score is within the ambiguity zone).
pub fn classify_heuristic(input: &str) -> Option<TaskComplexity> {
    let lower = input.to_lowercase();
    let mut score: i32 = 0;

    // Apply simple rules
    for rule in SIMPLE_RULES {
        for pattern in rule.patterns {
            if lower.contains(pattern) {
                score += rule.score;
            }
        }
    }

    // Apply complex rules
    for rule in COMPLEX_RULES {
        for pattern in rule.patterns {
            if lower.contains(pattern) {
                score += rule.score;
            }
        }
    }

    // Length heuristic: very short messages are likely simple questions
    let char_count = input.chars().count();
    if char_count < 30 {
        score -= 2;
    } else if char_count > 200 {
        score += 1;
    } else if char_count > 500 {
        score += 2;
    }

    // Line count heuristic: multi-line prompts suggest complexity
    let line_count = input.lines().count();
    if line_count > 5 {
        score += 1;
    }
    if line_count > 15 {
        score += 2;
    }

    // Decision thresholds
    if score <= -2 {
        Some(TaskComplexity::Simple)
    } else if score >= 4 {
        Some(TaskComplexity::Complex)
    } else if score >= 1 {
        Some(TaskComplexity::Medium)
    } else {
        // Ambiguous — fall back to LLM or default
        None
    }
}

/// Build a minimal LLM prompt for task classification.
///
/// This is designed to be cheap: short system prompt, single-token-ish
/// response. Called only when heuristics are inconclusive.
pub fn build_classification_prompt(user_input: &str) -> String {
    format!(
        r#"Classify the following user request into exactly one category.
Reply with ONLY one word: SIMPLE, MEDIUM, or COMPLEX.

Criteria:
- SIMPLE: questions, explanations, single-file reads, trivial fixes, discussions
- MEDIUM: multi-file edits, feature additions, refactoring within a module
- COMPLEX: cross-cutting changes, migrations, architecture redesign, multi-step tasks requiring verification

User request:
{}"#,
        user_input
    )
}

/// Parse the LLM's classification response.
pub fn parse_classification(response: &str) -> TaskComplexity {
    let upper = response.trim().to_uppercase();
    if upper.contains("SIMPLE") {
        TaskComplexity::Simple
    } else if upper.contains("COMPLEX") {
        TaskComplexity::Complex
    } else {
        // Default to Medium for anything ambiguous
        TaskComplexity::Medium
    }
}

/// The router mode configured in models.toml.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterMode {
    /// Adaptive routing: heuristics + optional LLM classification.
    Auto,
    /// Always use the full pipeline (backward-compatible with `enabled = true`).
    AlwaysPipeline,
    /// Always use basic loop (effectively `enabled = false`).
    AlwaysSimple,
}

impl Default for RouterMode {
    fn default() -> Self {
        RouterMode::AlwaysSimple
    }
}

impl std::str::FromStr for RouterMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" | "adaptive" | "router" => Ok(RouterMode::Auto),
            "always_pipeline" | "pipeline" | "full" => Ok(RouterMode::AlwaysPipeline),
            "always_simple" | "simple" | "basic" | "off" => Ok(RouterMode::AlwaysSimple),
            other => Err(format!(
                "unknown router mode '{}', expected: auto, always_pipeline, always_simple",
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_questions() {
        assert_eq!(
            classify_heuristic("What is this function doing?"),
            Some(TaskComplexity::Simple)
        );
        assert_eq!(
            classify_heuristic("explain the code"),
            Some(TaskComplexity::Simple)
        );
        assert_eq!(
            classify_heuristic("这个函数是什么意思？"),
            Some(TaskComplexity::Simple)
        );
    }

    #[test]
    fn test_complex_tasks() {
        assert_eq!(
            classify_heuristic(
                "Refactor the entire project to migrate from REST API to gRPC, \
                 update all files and ensure tests pass"
            ),
            Some(TaskComplexity::Complex)
        );
    }

    #[test]
    fn test_medium_tasks() {
        let result = classify_heuristic("Implement a new caching layer for the database module");
        assert!(
            result == Some(TaskComplexity::Medium) || result == Some(TaskComplexity::Complex),
            "Expected Medium or Complex, got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_classification() {
        assert_eq!(parse_classification("SIMPLE"), TaskComplexity::Simple);
        assert_eq!(parse_classification("COMPLEX"), TaskComplexity::Complex);
        assert_eq!(parse_classification("MEDIUM"), TaskComplexity::Medium);
        assert_eq!(parse_classification("  simple  "), TaskComplexity::Simple);
    }
}
