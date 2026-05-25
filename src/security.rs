//! File security guard — protects sensitive files and blocks dangerous commands.
//!
//! Two layers of protection:
//! 1. **Write safety**: prevents the agent from writing to sensitive paths
//!    (SSH keys, shell configs, password files, etc.)
//! 2. **Command safety**: detects dangerous shell command patterns
//!    (`rm -rf /`, `sudo rm`, `curl | bash`, etc.)

use std::path::Path;

// ── Sensitive file blacklist ────────────────────────────────────────────

/// Exact file paths that must NEVER be written to.
/// Matched by checking if the path *ends with* any of these suffixes
/// (relative to the user's home directory).
const DENIED_PATH_SUFFIXES: &[&str] = &[
    // SSH
    ".ssh/authorized_keys",
    ".ssh/id_rsa",
    ".ssh/id_ed25519",
    ".ssh/id_ecdsa",
    ".ssh/id_dsa",
    ".ssh/config",
    ".ssh/known_hosts",
    // Shell config
    ".bashrc",
    ".zshrc",
    ".profile",
    ".bash_profile",
    ".zprofile",
    ".bash_logout",
    ".zshenv",
    // Secrets / credentials
    ".netrc",
    ".pgpass",
    ".npmrc",
    ".pypirc",
    ".aws/credentials",
    ".aws/config",
    ".git-credentials",
    ".gitconfig",
    // System files (absolute)
    "/etc/sudoers",
    "/etc/passwd",
    "/etc/shadow",
    "/etc/group",
    "/etc/hosts",
    "/etc/hostname",
    "/etc/resolv.conf",
    "/etc/fstab",
    "/etc/crontab",
    // Agent/config files
    ".agent/config.toml",
    ".agent/memory.md",
    ".agent/user.md",
    ".env",
    ".env.local",
    ".env.production",
];

/// Directory prefixes that should NOT be written to.
const DENIED_PREFIXES: &[&str] = &[
    ".agent/",
    ".hermes/",
    ".openclaw/",
    ".ssh/",
    ".gnupg/",
    ".git/",
    "/etc/",
    "/boot/",
    "/sys/",
    "/proc/",
    "/dev/",
];

// ── Dangerous command patterns ──────────────────────────────────────────

/// Regex-like patterns that indicate a dangerous command.
/// We use simple substring matching for efficiency — regex would be overkill here.
const DANGEROUS_COMMAND_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "rm -rf $HOME",
    "rm -rf /*",
    "sudo rm",
    "sudo mv",
    "sudo dd",
    "sudo mkfs",
    "> /dev/sda",
    "mkfs.",
    "dd if=",
    ":(){ :|:& };:",   // fork bomb
    "chmod 777 /",
    "chmod -R 777 /",
    "chown -R ",
    "curl | bash",
    "curl | sh",
    "wget -O - | bash",
    "wget -O - | sh",
    "bash <(curl",
    "bash <(wget",
    "/etc/passwd",
    "/etc/shadow",
    "sudo shutdown",
    "sudo reboot",
    "sudo halt",
    "sudo poweroff",
    "git push --force",
    "git push -f",
];

/// Markers that indicate a command is *high risk* (should require extra confirmation).
const HIGH_RISK_MARKERS: &[&str] = &[
    "sudo ",
    "rm -rf",
    "rm -r",
    "| bash",
    "| sh",
    "/dev/sd",
    "mkfs.",
    "dd if=",
    "chmod 777",
    "chown ",
    "> /etc/",
    ">> /etc/",
];

// ── Public API ──────────────────────────────────────────────────────────

/// Check whether writing to `path` is safe.
///
/// `home` should be the user's home directory (or the project directory
/// for sandboxed runs).  Returns `Err(reason)` if the path is denied.
pub fn check_write_safety(path: &Path, home: &Path) -> Result<(), String> {
    // Resolve to a canonical form for reliable matching
    let canonical = canonicalize_best_effort(path);

    // 1. Check exact denied suffixes (relative to home)
    if let Ok(stripped) = canonical.strip_prefix(home) {
        let stripped_str = stripped.to_string_lossy();
        for denied in DENIED_PATH_SUFFIXES {
            if stripped_str == *denied || stripped_str.ends_with(&format!("/{}", denied)) {
                return Err(format!(
                    "Blocked: '{}' is a protected system/security file. Writing to it could compromise system security.",
                    path.display()
                ));
            }
        }
    }

    // 2. Check absolute /etc paths
    let path_str = canonical.to_string_lossy();
    for denied in DENIED_PATH_SUFFIXES {
        if denied.starts_with('/') && path_str == *denied {
            return Err(format!(
                "Blocked: '{}' is a protected system file. Writing to it could compromise system security.",
                path.display()
            ));
        }
    }

    // 3. Check denied prefixes
    let stripped_str = if let Ok(s) = canonical.strip_prefix(home) {
        s.to_string_lossy().to_string()
    } else {
        path_str.to_string()
    };
    for prefix in DENIED_PREFIXES {
        if prefix.starts_with('/') {
            // Absolute prefix — match against the full path
            if path_str.starts_with(*prefix) {
                return Err(format!(
                    "Blocked: '{}' is inside a protected directory ({}).",
                    path.display(), prefix
                ));
            }
        } else {
            // Relative prefix — match against the home-relative path
            if stripped_str.starts_with(prefix) || stripped_str == prefix.trim_end_matches('/') {
                return Err(format!(
                    "Blocked: '{}' is in a protected location ({}).",
                    path.display(), prefix
                ));
            }
        }
    }

    Ok(())
}

/// Check whether a shell command looks dangerous.
/// Returns `Err(reason)` if the command should be blocked outright.
pub fn check_command_safety(command: &str) -> Result<(), String> {
    let cmd_lower = command.to_lowercase();

    // Check exact/prefix patterns
    for pattern in DANGEROUS_COMMAND_PATTERNS {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return Err(format!(
                "Blocked: dangerous command pattern detected: '{}' in '{}'",
                pattern, command
            ));
        }
    }

    // Check combined patterns: curl/wget piped to bash/sh
    if (cmd_lower.contains("curl") || cmd_lower.contains("wget"))
        && cmd_lower.contains('|')
        && (cmd_lower.contains("bash") || cmd_lower.contains("sh"))
    {
        return Err(format!(
            "Blocked: pipe-to-shell pattern detected in '{}'. This is a common attack vector.",
            command
        ));
    }

    Ok(())
}

/// Check whether a command is *high risk* (needs extra confirmation beyond normal).
pub fn is_high_risk_command(command: &str) -> bool {
    let cmd_lower = command.to_lowercase();
    HIGH_RISK_MARKERS
        .iter()
        .any(|marker| cmd_lower.contains(&marker.to_lowercase()))
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Try to canonicalize a path; fall back to the original if it doesn't exist yet.
fn canonicalize_best_effort(path: &Path) -> std::path::PathBuf {
    // If the path exists, canonicalize it
    if let Ok(canon) = std::fs::canonicalize(path) {
        return canon;
    }
    // If the parent exists, canonicalize the parent + append the filename
    if let Some(parent) = path.parent() {
        if let Ok(canon_parent) = std::fs::canonicalize(parent) {
            if let Some(file_name) = path.file_name() {
                return canon_parent.join(file_name);
            }
        }
    }
    // Fallback: return as-is
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_write_safety_ssh_key_denied() {
        let home = Path::new("/home/user");
        assert!(check_write_safety(&home.join(".ssh/id_rsa"), home).is_err());
        assert!(check_write_safety(&home.join(".ssh/authorized_keys"), home).is_err());
        assert!(check_write_safety(&home.join(".ssh/config"), home).is_err());
    }

    #[test]
    fn test_check_write_safety_normal_file_allowed() {
        let home = Path::new("/home/user");
        assert!(check_write_safety(&home.join("src/main.rs"), home).is_ok());
        assert!(check_write_safety(&home.join("README.md"), home).is_ok());
    }

    #[test]
    fn test_check_write_safety_etc_denied() {
        assert!(check_write_safety(Path::new("/etc/passwd"), Path::new("/home/user")).is_err());
        assert!(check_write_safety(Path::new("/etc/sudoers"), Path::new("/home/user")).is_err());
    }

    #[test]
    fn test_check_command_safety_dangerous_blocked() {
        assert!(check_command_safety("rm -rf /").is_err());
        assert!(check_command_safety("sudo rm -rf /etc").is_err());
        assert!(check_command_safety("curl example.com | bash").is_err());
        assert!(check_command_safety("chmod 777 /").is_err());
    }

    #[test]
    fn test_check_command_safety_normal_allowed() {
        assert!(check_command_safety("cargo build").is_ok());
        assert!(check_command_safety("ls -la").is_ok());
        assert!(check_command_safety("git status").is_ok());
    }

    #[test]
    fn test_is_high_risk_command() {
        assert!(is_high_risk_command("sudo systemctl restart nginx"));
        assert!(is_high_risk_command("rm -rf target/"));
        assert!(is_high_risk_command("curl example.com | bash"));
        assert!(!is_high_risk_command("cargo build"));
        assert!(!is_high_risk_command("echo hello"));
    }
}
