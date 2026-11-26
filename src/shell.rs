use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::load_config;

const MARKER_START: &str = "# caf:managed:start";
const MARKER_END: &str = "# caf:managed:end";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    Powershell,
}

impl ShellKind {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "zsh" => Ok(Self::Zsh),
            "bash" => Ok(Self::Bash),
            "fish" => Ok(Self::Fish),
            "pwsh" | "powershell" | "ps" => Ok(Self::Powershell),
            other => bail!("unsupported shell `{other}` (supported: zsh, bash, fish, powershell)"),
        }
    }

    pub fn detect() -> Self {
        if cfg!(target_os = "windows") {
            return Self::Powershell;
        }
        if let Ok(shell) = env::var("SHELL") {
            let lower = shell.to_ascii_lowercase();
            if lower.ends_with("zsh") {
                return Self::Zsh;
            }
            if lower.ends_with("fish") {
                return Self::Fish;
            }
            if lower.ends_with("bash") {
                return Self::Bash;
            }
        }
        if cfg!(target_os = "macos") {
            Self::Zsh
        } else {
            Self::Bash
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Powershell => "powershell",
        }
    }

    pub fn rc_path(self) -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not resolve home directory")?;
        Ok(match self {
            Self::Zsh => home.join(".zshrc"),
            Self::Bash => home.join(".bashrc"),
            Self::Fish => home.join(".config").join("fish").join("config.fish"),
            Self::Powershell => {
                if cfg!(target_os = "windows") {
                    home.join("Documents")
                        .join("PowerShell")
                        .join("Microsoft.PowerShell_profile.ps1")
                } else {
                    home.join(".config")
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1")
                }
            }
        })
    }

    /// Minimal, TTY-guarded, once-per-shell hook snippet.
    /// Exits in <1ms silently when caf is off, non-TTY, or `shell_reminder = "off"`.
    pub fn snippet(self) -> &'static str {
        match self {
            Self::Zsh => {
                "# caf:managed:start\n\
                 if [[ -o interactive && -t 1 && -z \"${CAF_SHELL_SHOWN:-}\" ]]; then\n\
                   export CAF_SHELL_SHOWN=1\n\
                   command -v caf >/dev/null 2>&1 && caf logo --if-active --shell\n\
                 fi\n\
                 # caf:managed:end\n"
            }
            Self::Bash => {
                "# caf:managed:start\n\
                 if [[ $- == *i* && -t 1 && -z \"${CAF_SHELL_SHOWN:-}\" ]]; then\n\
                   export CAF_SHELL_SHOWN=1\n\
                   command -v caf >/dev/null 2>&1 && caf logo --if-active --shell\n\
                 fi\n\
                 # caf:managed:end\n"
            }
            Self::Fish => {
                "# caf:managed:start\n\
                 if status is-interactive; and isatty stdout; and not set -q CAF_SHELL_SHOWN\n\
                   set -gx CAF_SHELL_SHOWN 1\n\
                   type -q caf; and caf logo --if-active --shell\n\
                 end\n\
                 # caf:managed:end\n"
            }
            Self::Powershell => {
                "# caf:managed:start\n\
                 if (-not $env:CAF_SHELL_SHOWN -and -not [Console]::IsOutputRedirected) {\n\
                   $env:CAF_SHELL_SHOWN = '1'\n\
                   if (Get-Command caf -ErrorAction SilentlyContinue) { caf logo --if-active --shell }\n\
                 }\n\
                 # caf:managed:end\n"
            }
        }
    }
}

pub fn cmd_shell_init(shell: Option<&str>) -> Result<()> {
    let kind = match shell {
        Some(s) => ShellKind::parse(s)?,
        None => ShellKind::detect(),
    };
    print!("{}", kind.snippet());
    Ok(())
}

pub fn cmd_shell_install(shell: Option<&str>, dry_run: bool) -> Result<()> {
    let kind = match shell {
        Some(s) => ShellKind::parse(s)?,
        None => ShellKind::detect(),
    };
    let rc = kind.rc_path()?;
    let existing = if rc.is_file() {
        fs::read_to_string(&rc).with_context(|| format!("failed reading {}", rc.display()))?
    } else {
        String::new()
    };

    let updated = upsert_managed_block(&existing, kind.snippet());
    if updated == existing {
        println!(
            "caf shell hook ({}) is already up to date in {}",
            kind.name(),
            rc.display()
        );
        return Ok(());
    }

    if dry_run {
        println!(
            "Will install caf shell hook ({}) into {}",
            kind.name(),
            rc.display()
        );
        return Ok(());
    }

    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&rc, updated).with_context(|| format!("failed writing {}", rc.display()))?;
    println!(
        "Installed caf shell hook ({}) in {}",
        kind.name(),
        rc.display()
    );
    println!(
        "  Configure reminder style anytime with `caf config set shell_reminder <auto|full|medium|compact|oneline|off>`."
    );
    Ok(())
}

pub fn cmd_shell_uninstall(shell: Option<&str>, dry_run: bool) -> Result<()> {
    let kind = match shell {
        Some(s) => ShellKind::parse(s)?,
        None => ShellKind::detect(),
    };
    let rc = kind.rc_path()?;
    if !rc.is_file() {
        println!("No {} rc file found at {}.", kind.name(), rc.display());
        return Ok(());
    }
    let existing =
        fs::read_to_string(&rc).with_context(|| format!("failed reading {}", rc.display()))?;
    let Some(cleaned) = remove_managed_block(&existing) else {
        println!("No managed caf shell hook found in {}.", rc.display());
        return Ok(());
    };

    if dry_run {
        println!("Will remove managed caf shell hook from {}", rc.display());
        return Ok(());
    }

    fs::write(&rc, cleaned).with_context(|| format!("failed writing {}", rc.display()))?;
    println!("Removed managed caf shell hook from {}", rc.display());
    Ok(())
}

pub fn cmd_shell_status() -> Result<()> {
    let cfg = load_config()?;
    println!("shell_reminder = {}", cfg.shell_reminder);
    for kind in [
        ShellKind::Zsh,
        ShellKind::Bash,
        ShellKind::Fish,
        ShellKind::Powershell,
    ] {
        let Ok(rc) = kind.rc_path() else {
            continue;
        };
        if !rc.is_file() {
            continue;
        }
        let installed = has_managed_block(&rc);
        println!(
            "  {:<11} {:<11} {}",
            kind.name(),
            if installed { "installed" } else { "not hooked" },
            rc.display()
        );
    }
    Ok(())
}

fn has_managed_block(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|s| s.contains(MARKER_START) && s.contains(MARKER_END))
        .unwrap_or(false)
}

fn upsert_managed_block(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end_rel)) = (existing.find(MARKER_START), existing.find(MARKER_END))
        && end_rel >= start
    {
        let after_end = end_rel + MARKER_END.len();
        let after_nl = if existing[after_end..].starts_with("\r\n") {
            after_end + 2
        } else if existing[after_end..].starts_with('\n') {
            after_end + 1
        } else {
            after_end
        };
        let mut out = String::with_capacity(existing.len() + block.len());
        out.push_str(&existing[..start]);
        out.push_str(block);
        out.push_str(&existing[after_nl..]);
        return out;
    }

    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(block);
    out
}

fn remove_managed_block(existing: &str) -> Option<String> {
    let start = existing.find(MARKER_START)?;
    let end = existing.find(MARKER_END)?;
    if end < start {
        return None;
    }
    let after_end = end + MARKER_END.len();
    let after_nl = if existing[after_end..].starts_with("\r\n") {
        after_end + 2
    } else if existing[after_end..].starts_with('\n') {
        after_end + 1
    } else {
        after_end
    };
    let prefix = existing[..start].trim_end_matches('\n');
    let suffix = existing[after_nl..].trim_start_matches('\n');
    let mut out = String::new();
    if !prefix.is_empty() {
        out.push_str(prefix);
        out.push('\n');
    }
    if !suffix.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(suffix);
    }
    Some(out)
}
