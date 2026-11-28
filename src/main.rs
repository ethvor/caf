#![forbid(unsafe_code)]

mod art;
mod config;
mod power;
mod shell;

use std::env;
use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::power::DisplayOverrides;

#[derive(Parser)]
#[command(
    name = "caf",
    version,
    about = "Cross-platform lid-closed keep-awake, dark-screen, and timer CLI with adaptive terminal art (symlink as `decaf` to turn off)"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,

    /// Apply a named preset from `~/.config/caf/config.toml` (`[presets.<name>]`)
    #[arg(long, global = true)]
    preset: Option<String>,

    /// Override display LOD (`auto`, `full`, `medium`, `compact`, `oneline`, `off`)
    #[arg(long, global = true)]
    lod: Option<String>,

    /// Override mug color palette (`random`, built-in name, or custom palette)
    #[arg(long, alias = "mug", global = true)]
    palette: Option<String>,

    /// Override mug glaze pattern (`random`, `gradient`, `diagonal`, `wave`, `chevron`, `checker`, `stripes`, `dots`, `plasma`)
    #[arg(long, global = true)]
    pattern: Option<String>,

    /// Simulate a specific terminal column width (for testing resize / LOD)
    #[arg(long, global = true)]
    width: Option<usize>,

    /// Override horizontal alignment (`center` or `left`)
    #[arg(long, global = true)]
    align: Option<String>,

    /// Skip live animation and print static banner immediately
    #[arg(long = "no-animate", alias = "static", global = true)]
    no_animate: bool,

    /// Optional duration when running `caf <duration>` directly (e.g. 45m, 2h)
    duration: Option<String>,

    /// Explicitly enable lid-closed keep-awake (already on by default)
    #[arg(long)]
    lid: bool,

    /// Disable lid-closed stay-awake (standard display+system keep-awake only; no pmset/sudo)
    #[arg(long)]
    no_lid: bool,

    /// Put display to sleep immediately while keeping system/agents awake
    #[arg(long)]
    dark: bool,

    /// Keep display awake for <duration>, then put display to sleep while system stays awake (e.g. `--dark-in 10m`)
    #[arg(long = "dark-in", alias = "dark-after")]
    dark_in: Option<String>,

    /// Short human reason for keeping the machine awake
    #[arg(short, long)]
    reason: Option<String>,

    /// Force live animated watch view even if `animate = false` in config
    #[arg(long)]
    watch: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Turn keep-awake ON (lid-close safe by default; same as bare `caf`)
    On {
        duration: Option<String>,
        #[arg(long)]
        lid: bool,
        #[arg(long)]
        no_lid: bool,
        #[arg(long)]
        dark: bool,
        #[arg(long = "dark-in", alias = "dark-after")]
        dark_in: Option<String>,
        #[arg(short, long)]
        reason: Option<String>,
        #[arg(long)]
        watch: bool,
    },
    /// Turn keep-awake OFF and restore normal sleep behavior (alias: `decaf`)
    Off,
    /// Show concise plain-text keep-awake status (no art by default; use `caf logo` for art)
    Status,
    /// Render, animate, or browse CAF/DECAF terminal art, LODs, palettes, patterns, and presets
    #[command(alias = "art")]
    Logo {
        /// Exit silently without output when caf is inactive
        #[arg(long)]
        if_active: bool,
        /// Use `shell_reminder` config policy and skip non-TTY shells
        #[arg(long)]
        shell: bool,
        /// Animate live (←/→ palette, ↑/↓ pattern, / or ? random, s save default, r reset random, x hide palette, q exit)
        #[arg(long)]
        animate: bool,
        /// Browse gallery: `all`, `lods`, `patterns`, or `palettes`
        #[arg(long, num_args = 0..=1, default_missing_value = "all")]
        gallery: Option<String>,
        /// List available palettes (including custom & disabled), patterns, and presets
        #[arg(long)]
        list: bool,
    },
    /// Manage ~/.config/caf/config.toml (modes, disabled colors, custom palettes, presets)
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Install, inspect, or remove the new-terminal reminder hook (zsh, bash, fish, PowerShell)
    Shell {
        #[command(subcommand)]
        action: ShellAction,
    },
    /// Install a narrowly scoped macOS sudoers rule for passwordless lid-closed mode and timed restore
    AuthorizeLid {
        /// Preview the sudoers rule without writing `/etc/sudoers.d/`
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print active configuration, custom palettes, and presets
    Show,
    /// Set a configuration key (`display`, `shell_reminder`, `align`, `animate`, `lid_by_default`, `clear_on_shell`, `preset`, `palette`, `pattern`, `color`, `caption`)
    Set { key: String, value: String },
    /// Exclude a color palette from random picks and arrow cycling
    DisablePalette { name: String },
    /// Re-enable a previously disabled color palette
    EnablePalette { name: String },
    /// Exclude a glaze pattern from random picks and arrow cycling
    DisablePattern { name: String },
    /// Re-enable a previously disabled glaze pattern
    EnablePattern { name: String },
    /// Add or update a custom color palette (`#rrggbb` or `r,g,b`)
    AddPalette {
        name: String,
        from: String,
        to: String,
    },
    /// Remove a custom color palette
    RmPalette { name: String },
    /// Add or update a named preset combining palette, pattern, and optional LOD
    AddPreset {
        name: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long)]
        lod: Option<String>,
    },
    /// Remove a named preset
    RmPreset { name: String },
    /// Print the path to config.toml
    Path,
    /// Reset config.toml to defaults
    Reset,
}

#[derive(Subcommand)]
enum ShellAction {
    /// Print the managed shell hook snippet for manual sourcing
    Init {
        /// Shell type (`zsh`, `bash`, `fish`, `powershell`; auto-detected if omitted)
        shell: Option<String>,
    },
    /// Idempotently install the managed reminder hook in your shell rc file
    Install {
        /// Shell type (`zsh`, `bash`, `fish`, `powershell`; auto-detected if omitted)
        #[arg(long)]
        shell: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the managed caf reminder hook from your shell rc file
    Uninstall {
        #[arg(long)]
        shell: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show installed shell hooks and active `shell_reminder` mode
    Status,
}

fn main() -> Result<()> {
    let invoked_as_decaf = env::args()
        .next()
        .and_then(|a| {
            Path::new(&a)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s == "decaf" || s == "decaf.exe")
        })
        .unwrap_or(false);

    let cli = Cli::parse();
    let overrides = DisplayOverrides {
        preset: cli.preset.as_deref(),
        lod: cli.lod.as_deref(),
        palette: cli.palette.as_deref(),
        pattern: cli.pattern.as_deref(),
        width: cli.width,
        align: cli.align.as_deref(),
    };

    if invoked_as_decaf {
        return power::cmd_off(cli.no_animate, overrides, cli.json);
    }

    match cli.command {
        Some(Commands::Off) => power::cmd_off(cli.no_animate, overrides, cli.json),
        Some(Commands::Status) => power::cmd_status(cli.json),
        Some(Commands::Logo {
            if_active,
            shell,
            animate,
            gallery,
            list,
        }) => power::cmd_logo(
            if_active,
            shell,
            animate,
            gallery.as_deref(),
            list,
            overrides,
        ),
        Some(Commands::AuthorizeLid { dry_run }) => power::cmd_authorize_lid(dry_run),
        Some(Commands::Config { action }) => match action {
            None | Some(ConfigAction::Show) => config::cmd_config_show(cli.json),
            Some(ConfigAction::Set { key, value }) => config::cmd_config_set(&key, &value),
            Some(ConfigAction::DisablePalette { name }) => {
                config::cmd_config_disable_palette(&name)
            }
            Some(ConfigAction::EnablePalette { name }) => config::cmd_config_enable_palette(&name),
            Some(ConfigAction::DisablePattern { name }) => {
                config::cmd_config_disable_pattern(&name)
            }
            Some(ConfigAction::EnablePattern { name }) => config::cmd_config_enable_pattern(&name),
            Some(ConfigAction::AddPalette { name, from, to }) => {
                config::cmd_config_add_palette(&name, &from, &to)
            }
            Some(ConfigAction::RmPalette { name }) => config::cmd_config_rm_palette(&name),
            Some(ConfigAction::AddPreset {
                name,
                palette,
                pattern,
                lod,
            }) => config::cmd_config_add_preset(
                &name,
                palette.as_deref(),
                pattern.as_deref(),
                lod.as_deref(),
            ),
            Some(ConfigAction::RmPreset { name }) => config::cmd_config_rm_preset(&name),
            Some(ConfigAction::Path) => config::cmd_config_path(),
            Some(ConfigAction::Reset) => config::cmd_config_reset(),
        },
        Some(Commands::Shell { action }) => match action {
            ShellAction::Init { shell } => shell::cmd_shell_init(shell.as_deref()),
            ShellAction::Install { shell, dry_run } => {
                shell::cmd_shell_install(shell.as_deref(), dry_run)
            }
            ShellAction::Uninstall { shell, dry_run } => {
                shell::cmd_shell_uninstall(shell.as_deref(), dry_run)
            }
            ShellAction::Status => shell::cmd_shell_status(),
        },
        Some(Commands::On {
            duration,
            lid,
            no_lid,
            dark,
            dark_in,
            reason,
            watch,
        }) => power::cmd_on(
            duration.as_deref(),
            lid || cli.lid,
            no_lid || cli.no_lid,
            dark || cli.dark,
            dark_in.as_deref().or(cli.dark_in.as_deref()),
            reason.as_deref().or(cli.reason.as_deref()),
            watch || cli.watch,
            cli.no_animate,
            overrides,
            cli.json,
        ),
        None => power::cmd_on(
            cli.duration.as_deref(),
            cli.lid,
            cli.no_lid,
            cli.dark,
            cli.dark_in.as_deref(),
            cli.reason.as_deref(),
            cli.watch,
            cli.no_animate,
            overrides,
            cli.json,
        ),
    }
}
