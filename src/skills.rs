//! Skills loading system.
//!
//! Scans the working directory for skill files that provide
//! project-specific instructions to the agent. Skills are loaded
//! from:
//!   - `AGENT.md` (project root, global instructions)
//!   - `.agent/skills/*.md` (individual skill files)
//!
//! Loaded skills are appended to the system prompt so the LLM
//! is aware of project conventions and workflows.

use std::path::{Path, PathBuf};
use tracing::debug;

/// A loaded skill
#[derive(Debug, Clone)]
pub struct Skill {
    /// Display name derived from the file name
    pub name: String,
    /// Source file path (relative to working directory)
    pub source: String,
    /// The raw Markdown content
    pub content: String,
}

/// Result of scanning for skills
#[derive(Debug, Clone)]
pub struct LoadedSkills {
    pub skills: Vec<Skill>,
}

impl LoadedSkills {
    /// Format all loaded skills into a single string for the system prompt.
    pub fn to_system_prompt_section(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push("\n\n--- Project Skills ---".to_string());

        for skill in &self.skills {
            parts.push(format!(
                "\n## Skill: {} (from {})\n\n{}",
                skill.name, skill.source, skill.content
            ));
        }

        parts.join("\n")
    }

    /// Return true if no skills were loaded.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }
}

/// Scan the given directory (typically cwd) for skill files.
pub fn load_skills(workdir: &Path) -> LoadedSkills {
    let mut skills = Vec::new();

    // 1. Load AGENT.md from the project root
    let agent_md = workdir.join("AGENT.md");
    if let Some(skill) = load_skill_file(&agent_md, "Project Instructions", "AGENT.md") {
        debug!("Loaded project instructions from AGENT.md");
        skills.push(skill);
    }

    // 2. Load .agent/skills/*.md
    let skills_dir = workdir.join(".agent").join("skills");
    if skills_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            let mut entries: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|ext| ext == "md" || ext == "markdown")
                })
                .collect();

            // Sort for deterministic ordering
            entries.sort();

            for path in entries {
                let file_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                let relative = format!(
                    ".agent/skills/{}",
                    path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                );

                // Convert file stem to a human-readable name:
                //   "add-new-tool" -> "Add New Tool"
                let display_name = file_name
                    .split(|c: char| c == '-' || c == '_')
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(first) => {
                                format!("{}{}", first.to_uppercase(), chars.as_str())
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                if let Some(skill) = load_skill_file(&path, &display_name, &relative) {
                    debug!("Loaded skill '{}' from {}", display_name, relative);
                    skills.push(skill);
                }
            }
        }
    }

    LoadedSkills { skills }
}

/// Read a single skill file, returning None if it can't be read or is empty.
fn load_skill_file(path: &Path, name: &str, source: &str) -> Option<Skill> {
    let content = std::fs::read_to_string(path).ok()?;
    let content = content.trim().to_string();

    if content.is_empty() {
        return None;
    }

    Some(Skill {
        name: name.to_string(),
        source: source.to_string(),
        content,
    })
}
