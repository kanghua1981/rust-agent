//! Diff preview for file modifications.
//!
//! Shows a unified diff before file changes are applied,
//! so the user can review what will change.

use colored::*;

/// Generate and print a unified diff between old and new content
pub fn print_diff(path: &str, old_content: &str, new_content: &str) {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    println!(
        "\n{}",
        format!("━━━ Diff: {} ━━━", path).bright_cyan().bold()
    );

    // Simple line-by-line diff using longest common subsequence approach
    let diff_hunks = compute_diff(&old_lines, &new_lines);

    if diff_hunks.is_empty() {
        println!("   {}", "(no changes)".dimmed());
    } else {
        for hunk in &diff_hunks {
            // Print hunk header
            println!(
                "{}",
                format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start + 1,
                    hunk.old_count,
                    hunk.new_start + 1,
                    hunk.new_count,
                )
                .bright_cyan()
            );

            for line in &hunk.lines {
                match line {
                    DiffLine::Context(text) => {
                        println!(" {}", text);
                    }
                    DiffLine::Added(text) => {
                        println!("{}", format!("+ {}", text).green());
                    }
                    DiffLine::Removed(text) => {
                        println!("{}", format!("- {}", text).red());
                    }
                }
            }
        }
    }

    println!(
        "{}",
        "━".repeat(40).bright_cyan()
    );
}

/// Generate a diff string (for tool result output, not colored)
#[allow(dead_code)]
pub fn diff_string(path: &str, old_content: &str, new_content: &str) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let diff_hunks = compute_diff(&old_lines, &new_lines);
    let mut result = format!("--- a/{}\n+++ b/{}\n", path, path);

    for hunk in &diff_hunks {
        result.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start + 1,
            hunk.old_count,
            hunk.new_start + 1,
            hunk.new_count,
        ));

        for line in &hunk.lines {
            match line {
                DiffLine::Context(text) => {
                    result.push_str(&format!(" {}\n", text));
                }
                DiffLine::Added(text) => {
                    result.push_str(&format!("+{}\n", text));
                }
                DiffLine::Removed(text) => {
                    result.push_str(&format!("-{}\n", text));
                }
            }
        }
    }

    result
}

#[derive(Debug)]
enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

#[derive(Debug)]
struct DiffHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<DiffLine>,
}

/// Compute diff hunks between old and new lines using a simple Myers-like algorithm
fn compute_diff(old: &[&str], new: &[&str]) -> Vec<DiffHunk> {
    // Compute the edit script using LCS
    let lcs = longest_common_subsequence(old, new);

    let mut edits: Vec<Edit> = Vec::new();
    let mut old_idx = 0;
    let mut new_idx = 0;

    for &(o, n) in &lcs {
        // Lines removed from old (before this LCS match)
        while old_idx < o {
            edits.push(Edit::Remove(old_idx, old[old_idx].to_string()));
            old_idx += 1;
        }
        // Lines added in new (before this LCS match)
        while new_idx < n {
            edits.push(Edit::Add(new_idx, new[new_idx].to_string()));
            new_idx += 1;
        }
        // Context line
        edits.push(Edit::Context(old_idx, new_idx, old[old_idx].to_string()));
        old_idx += 1;
        new_idx += 1;
    }

    // Remaining lines
    while old_idx < old.len() {
        edits.push(Edit::Remove(old_idx, old[old_idx].to_string()));
        old_idx += 1;
    }
    while new_idx < new.len() {
        edits.push(Edit::Add(new_idx, new[new_idx].to_string()));
        new_idx += 1;
    }

    // Group edits into hunks with context
    group_into_hunks(&edits, old.len(), new.len())
}

#[derive(Debug)]
enum Edit {
    Context(usize, usize, String), // (old_line, new_line, text)
    Add(usize, String),            // (new_line, text)
    Remove(usize, String),         // (old_line, text)
}

/// Group edits into hunks, including 3 lines of context around changes
fn group_into_hunks(edits: &[Edit], _old_len: usize, _new_len: usize) -> Vec<DiffHunk> {
    let context_lines = 3;
    let mut hunks: Vec<DiffHunk> = Vec::new();

    // Find ranges of changes
    let mut i = 0;
    while i < edits.len() {
        // Skip context lines until we find a change
        match &edits[i] {
            Edit::Context(_, _, _) => {
                i += 1;
                continue;
            }
            _ => {}
        }

        // Found a change - build a hunk
        let start = if i > context_lines {
            i - context_lines
        } else {
            0
        };

        // Find the end of this change group
        let mut end = i;
        let mut last_change = i;
        while end < edits.len() {
            match &edits[end] {
                Edit::Context(_, _, _) => {
                    if end - last_change > context_lines * 2 {
                        break;
                    }
                }
                _ => {
                    last_change = end;
                }
            }
            end += 1;
        }

        let hunk_end = (last_change + context_lines + 1).min(edits.len());

        // Build the hunk
        let mut lines = Vec::new();
        let mut old_start = usize::MAX;
        let mut new_start = usize::MAX;
        let mut old_count = 0;
        let mut new_count = 0;

        for edit in &edits[start..hunk_end] {
            match edit {
                Edit::Context(ol, nl, text) => {
                    if old_start == usize::MAX {
                        old_start = *ol;
                        new_start = *nl;
                    }
                    old_count += 1;
                    new_count += 1;
                    lines.push(DiffLine::Context(text.clone()));
                }
                Edit::Remove(ol, text) => {
                    if old_start == usize::MAX {
                        old_start = *ol;
                        new_start = *ol; // approximation
                    }
                    old_count += 1;
                    lines.push(DiffLine::Removed(text.clone()));
                }
                Edit::Add(nl, text) => {
                    if old_start == usize::MAX {
                        old_start = *nl;
                        new_start = *nl;
                    }
                    new_count += 1;
                    lines.push(DiffLine::Added(text.clone()));
                }
            }
        }

        if old_start == usize::MAX {
            old_start = 0;
        }
        if new_start == usize::MAX {
            new_start = 0;
        }

        hunks.push(DiffHunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines,
        });

        i = hunk_end;
    }

    hunks
}

/// Compute Longest Common Subsequence - returns indices pairs (old_idx, new_idx)
fn longest_common_subsequence(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let m = old.len();
    let n = new.len();

    // For very large files, use a simplified approach
    if m * n > 10_000_000 {
        return simple_lcs(old, new);
    }

    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find the LCS
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if old[i - 1] == new[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    result.reverse();
    result
}

/// Simplified LCS for very large files - match equal lines greedily
fn simple_lcs(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut j = 0;

    for i in 0..old.len() {
        while j < new.len() {
            if old[i] == new[j] {
                result.push((i, j));
                j += 1;
                break;
            }
            j += 1;
        }
        if j >= new.len() {
            break;
        }
    }

    result
}
