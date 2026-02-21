//! Skills loading system.
//!
//! Scans the working directory for skill files that provide
//! project-specific instructions to the agent. Skills are loaded
//! from:
//!   - `AGENT.md` (project root, global instructions) — **always fully loaded**
//!   - `.agent/skills/*.md` (individual skill files) — **index only** in system prompt;
//!     full content loaded on demand via the `load_skill` tool.
//!
//! This two-tier approach keeps the system prompt lean while still
//! letting the LLM discover and use any number of skills.

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

/// A lightweight index entry for a skill (name + one-line description).
#[derive(Debug, Clone)]
pub struct SkillIndex {
    /// Display name derived from the file name
    pub name: String,
    /// Source file path (relative to working directory)
    pub source: String,
    /// First non-empty, non-heading line of the skill file (one-liner).
    pub description: String,
}

/// Result of scanning for skills
#[derive(Debug, Clone)]
pub struct LoadedSkills {
    /// Skills whose full content is embedded (currently only AGENT.md).
    pub skills: Vec<Skill>,
    /// Skills available on demand (`.agent/skills/*.md`).
    pub index: Vec<SkillIndex>,
}

impl LoadedSkills {
    /// Format into a system prompt section.
    ///
    /// Full skills are embedded verbatim; indexed skills are listed as a
    /// compact catalogue so the LLM knows they exist and can call the
    /// `load_skill` tool when it needs the details.
    pub fn to_system_prompt_section(&self) -> String {
        if self.skills.is_empty() && self.index.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push("\n\n--- Project Skills ---".to_string());

        // Full-content skills (AGENT.md)
        for skill in &self.skills {
            parts.push(format!(
                "\n## Skill: {} (from {})\n\n{}",
                skill.name, skill.source, skill.content
            ));
        }

        // On-demand skill index
        if !self.index.is_empty() {
            parts.push("\n## Available Skills (use `load_skill` tool to read full content)".to_string());
            for entry in &self.index {
                parts.push(format!("- **{}** ({}) — {}", entry.name, entry.source, entry.description));
            }
        }

        parts.join("\n")
    }

    /// Return true if nothing was loaded at all.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.index.is_empty()
    }

    /// Total number of loaded + indexed skills.
    pub fn len(&self) -> usize {
        self.skills.len() + self.index.len()
    }
}

/// Scan the given directory (typically cwd) for skill files.
pub fn load_skills(workdir: &Path) -> LoadedSkills {
    let mut skills = Vec::new();
    let mut index = Vec::new();

    // 1. Load AGENT.md from the project root — always full content
    let agent_md = workdir.join("AGENT.md");
    if let Some(skill) = load_skill_file(&agent_md, "Project Instructions", "AGENT.md") {
        debug!("Loaded project instructions from AGENT.md");
        skills.push(skill);
    }

    // 2. Index .agent/skills/*.md — only name + description
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
                let relative = format!(
                    ".agent/skills/{}",
                    path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                );

                if let Some(entry) = build_skill_index(&path, &relative) {
                    debug!("Indexed skill '{}' from {}", entry.name, relative);
                    index.push(entry);
                }
            }
        }
    }

    LoadedSkills { skills, index }
}

/// Load a specific skill by name (for the `load_skill` tool).
///
/// Searches `.agent/skills/` for a file whose frontmatter `name`,
/// humanised file stem, or raw file stem matches `skill_name`
/// (case-insensitive). Returns `None` if not found.
pub fn load_skill_by_name(workdir: &Path, skill_name: &str) -> Option<Skill> {
    let skills_dir = workdir.join(".agent").join("skills");
    if !skills_dir.is_dir() {
        return None;
    }

    let entries = std::fs::read_dir(&skills_dir).ok()?;
    let needle = skill_name.to_lowercase();

    for entry in entries.flatten() {
        let path = entry.path();
        let ext_ok = path
            .extension()
            .is_some_and(|ext| ext == "md" || ext == "markdown");
        if !ext_ok {
            continue;
        }

        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let raw_content = std::fs::read_to_string(&path).ok().unwrap_or_default();
        let fm = parse_frontmatter(&raw_content);
        let display_name = fm.name.clone().unwrap_or_else(|| humanize_name(&file_stem));

        // Match by frontmatter name, display name, or raw file stem (case-insensitive)
        if display_name.to_lowercase() == needle
            || file_stem.to_lowercase() == needle
            || file_stem.to_lowercase().replace('-', " ") == needle
            || file_stem.to_lowercase().replace('_', " ") == needle
        {
            let relative = format!(
                ".agent/skills/{}",
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            );
            return load_skill_file(&path, &display_name, &relative);
        }
    }

    None
}

/// List all available on-demand skill names (for error messages / hints).
pub fn list_skill_names(workdir: &Path) -> Vec<String> {
    let loaded = load_skills(workdir);
    loaded
        .index
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

/// Convert a file stem like "add-new-tool" into "Add New Tool".
fn humanize_name(stem: &str) -> String {
    stem.split(|c: char| c == '-' || c == '_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parsed YAML frontmatter from a skill file.
#[derive(Debug, Default)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// Parse YAML frontmatter delimited by `---` from the beginning of a
/// Markdown file.  Returns the extracted fields and does **not** require
/// a YAML parsing crate — we only care about simple `key: value` lines.
fn parse_frontmatter(raw: &str) -> Frontmatter {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Frontmatter::default();
    }

    // Find the closing `---`
    let after_open = &trimmed[3..]; // skip the opening ---
    let close_pos = match after_open.find("\n---") {
        Some(p) => p,
        None => return Frontmatter::default(),
    };

    let yaml_block = &after_open[..close_pos];
    let mut fm = Frontmatter::default();

    for line in yaml_block.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name:") {
            fm.name = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(rest) = line.strip_prefix("description:") {
            fm.description = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }

    fm
}

/// Strip YAML frontmatter from the raw file content, returning only the
/// body (everything after the closing `---`).
fn strip_frontmatter(raw: &str) -> &str {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return raw;
    }
    let after_open = &trimmed[3..];
    match after_open.find("\n---") {
        Some(p) => {
            let after_close = &after_open[p + 4..]; // skip \n---
            // Skip the newline right after closing ---
            after_close.strip_prefix('\n').unwrap_or(after_close)
        }
        None => raw,
    }
}

/// Build a lightweight index entry from a skill file.
///
/// If the file contains a YAML frontmatter with `name` and `description`,
/// those are used directly.  Otherwise we fall back to the old heuristic
/// (name from filename, description from first non-heading line).
fn build_skill_index(path: &Path, source: &str) -> Option<SkillIndex> {
    let raw = std::fs::read_to_string(path).ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let fm = parse_frontmatter(raw);

    // Name: frontmatter > humanized filename
    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = fm.name.unwrap_or_else(|| humanize_name(&file_stem));

    // Description: frontmatter > first non-empty, non-heading line
    let description = fm.description.unwrap_or_else(|| {
        let body = strip_frontmatter(raw);
        body.lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .unwrap_or("(no description)")
            .to_string()
    });

    // Truncate very long descriptions
    let description = if description.len() > 120 {
        format!("{}…", &description[..117])
    } else {
        description
    };

    Some(SkillIndex {
        name,
        source: source.to_string(),
        description,
    })
}

/// Read a single skill file, returning None if it can't be read or is empty.
///
/// If the file has YAML frontmatter, the `name` field overrides the
/// fallback `name` argument, and the frontmatter block is stripped from
/// the returned `content`.
fn load_skill_file(path: &Path, name: &str, source: &str) -> Option<Skill> {
    let raw = std::fs::read_to_string(path).ok()?;
    let raw_trimmed = raw.trim();
    if raw_trimmed.is_empty() {
        return None;
    }

    let fm = parse_frontmatter(raw_trimmed);
    let skill_name = fm.name.unwrap_or_else(|| name.to_string());
    let body = strip_frontmatter(raw_trimmed).trim().to_string();

    Some(Skill {
        name: skill_name,
        source: source.to_string(),
        content: body,
    })
}
