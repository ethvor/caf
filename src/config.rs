use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::art::{BUILTIN_PALETTE_NAMES, PATTERN_NAMES, Rgb};

pub const DISPLAY_MODES: &[&str] = &["auto", "full", "medium", "compact", "oneline", "off"];
pub const ALIGN_MODES: &[&str] = &["left", "center"];
pub const COLOR_MODES: &[&str] = &["auto", "always", "never"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPalette {
    /// Top / primary gradient color (`#rrggbb` or `r,g,b`)
    pub from: String,
    /// Bottom / secondary gradient color (`#rrggbb` or `r,g,b`)
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PresetSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lod: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafConfig {
    /// Display LOD for explicit `caf` and `caf logo` invocations
    #[serde(default = "default_auto")]
    pub display: String,
    /// Display LOD for new-shell reminders (`caf logo --if-active --shell`)
    #[serde(default = "default_auto")]
    pub shell_reminder: String,
    /// Horizontal alignment for static banners: `center` (default) or `left` (extra resize-proof)
    #[serde(default = "default_align")]
    pub align: String,
    /// Play live animations on interactive `caf` and `decaf` commands (`true` | `false`)
    #[serde(default = "default_true")]
    pub animate: bool,
    /// Clear a fresh interactive terminal (e.g. macOS `Last login:`) before printing the shell reminder
    #[serde(default)]
    pub clear_on_shell: bool,
    /// Default lid-closed keep-awake behavior (`true` by default; override per-run with `--no-lid`)
    #[serde(default = "default_true")]
    pub lid_by_default: bool,
    /// Optional default named preset from `[presets.<name>]`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Mug color palette (`random`, built-in name, or custom name from `[palettes.<name>]`)
    #[serde(default = "default_random")]
    pub palette: String,
    /// Mug ceramic glaze pattern (`random` or a named pattern)
    #[serde(default = "default_random")]
    pub pattern: String,
    /// Color output mode (`auto`, `always`, or `never`)
    #[serde(default = "default_auto")]
    pub color: String,
    /// Caption beneath title: `auto` (current OS username), `off`, or custom text
    #[serde(default = "default_auto")]
    pub caption: String,
    /// Palette names excluded from `random` selection and live arrow cycling
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_palettes: Vec<String>,
    /// Pattern names excluded from `random` selection and live arrow cycling
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_patterns: Vec<String>,
    /// User-defined color palettes (`[palettes.<name>]` with `from` and `to` hex/RGB)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub palettes: BTreeMap<String, CustomPalette>,
    /// User-defined presets (`[presets.<name>]` combining `palette`, `pattern`, and optional `lod`)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub presets: BTreeMap<String, PresetSpec>,
}

impl Default for CafConfig {
    fn default() -> Self {
        Self {
            display: default_auto(),
            shell_reminder: default_auto(),
            align: default_align(),
            animate: true,
            clear_on_shell: false,
            lid_by_default: true,
            preset: None,
            palette: default_random(),
            pattern: default_random(),
            color: default_auto(),
            caption: default_auto(),
            disabled_palettes: Vec::new(),
            disabled_patterns: Vec::new(),
            palettes: BTreeMap::new(),
            presets: BTreeMap::new(),
        }
    }
}

fn default_auto() -> String {
    "auto".to_string()
}

fn default_align() -> String {
    "center".to_string()
}

fn default_random() -> String {
    "random".to_string()
}

fn default_true() -> bool {
    true
}

impl CafConfig {
    /// Resolves `caption`: `auto` -> OS username, `off`/empty -> None, otherwise literal.
    pub fn resolved_caption(&self) -> Option<String> {
        let raw = self.caption.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("off") || raw.eq_ignore_ascii_case("none") {
            return None;
        }
        if raw.eq_ignore_ascii_case("auto") {
            return current_username();
        }
        Some(raw.to_string())
    }

    pub fn has_palette_name(&self, name: &str) -> bool {
        let lower = name.trim().to_ascii_lowercase();
        BUILTIN_PALETTE_NAMES.contains(&lower.as_str())
            || self.palettes.keys().any(|k| k.eq_ignore_ascii_case(&lower))
    }
}

pub fn parse_color_spec(raw: &str) -> Result<Rgb> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let r = u8::from_str_radix(&hex[0..2], 16)?;
            let g = u8::from_str_radix(&hex[2..4], 16)?;
            let b = u8::from_str_radix(&hex[4..6], 16)?;
            return Ok(Rgb::new(r, g, b));
        }
        if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let dup = |idx: usize| -> Result<u8> {
                let n = u8::from_str_radix(&hex[idx..idx + 1], 16)?;
                Ok(n * 17)
            };
            return Ok(Rgb::new(dup(0)?, dup(1)?, dup(2)?));
        }
        bail!("invalid hex color `{raw}` (expected `#rrggbb` or `#rgb`)");
    }
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() == 3 {
        let r = parts[0]
            .parse::<u8>()
            .with_context(|| format!("invalid red channel in `{raw}`"))?;
        let g = parts[1]
            .parse::<u8>()
            .with_context(|| format!("invalid green channel in `{raw}`"))?;
        let b = parts[2]
            .parse::<u8>()
            .with_context(|| format!("invalid blue channel in `{raw}`"))?;
        return Ok(Rgb::new(r, g, b));
    }
    bail!("invalid color `{raw}` (use `#rrggbb`, `#rgb`, or `r,g,b`)")
}

fn current_username() -> Option<String> {
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(val) = env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn config_path() -> Result<PathBuf> {
    if let Ok(val) = env::var("CAF_CONFIG")
        && !val.trim().is_empty()
    {
        return Ok(PathBuf::from(val.trim()));
    }
    let home = dirs::home_dir().context("could not resolve home directory")?;
    Ok(home.join(".config").join("caf").join("config.toml"))
}

pub fn load_config() -> Result<CafConfig> {
    let path = config_path()?;
    if !path.is_file() {
        return Ok(CafConfig::default());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("invalid TOML in {}", path.display()))
}

pub fn save_config(cfg: &CafConfig) -> Result<PathBuf> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# caf configuration — edit directly or via `caf config ...`\n\
         # display / shell_reminder: auto | full | medium | compact | oneline | off\n\
         # align:          center | left (extra resize-proof)\n\
         # animate:        true | false\n\
         # lid_by_default: true | false\n\
         # clear_on_shell: false | true\n\
         # palette:        random | {} (or any custom [palettes.<name>])\n\
         # pattern:        random | {}\n\
         # color:          auto | always | never\n\
         # caption:        auto (OS username) | off | custom string\n\n{}",
        BUILTIN_PALETTE_NAMES.join(" | "),
        PATTERN_NAMES.join(" | "),
        toml::to_string_pretty(cfg)?
    );
    fs::write(&path, body).with_context(|| format!("failed writing {}", path.display()))?;
    Ok(path)
}

pub fn cmd_config_show(as_json: bool) -> Result<()> {
    let cfg = load_config()?;
    let path = config_path()?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path.display().to_string(),
                "exists": path.is_file(),
                "config": cfg,
                "resolved_caption": cfg.resolved_caption(),
            }))?
        );
        return Ok(());
    }
    println!(
        "# caf config ({}{})\n",
        path.display(),
        if path.is_file() { "" } else { " — defaults" }
    );
    println!("  display            = {}", cfg.display);
    println!("  shell_reminder     = {}", cfg.shell_reminder);
    println!("  align              = {}", cfg.align);
    println!("  animate            = {}", cfg.animate);
    println!("  lid_by_default     = {}", cfg.lid_by_default);
    println!("  clear_on_shell     = {}", cfg.clear_on_shell);
    println!(
        "  preset             = {}",
        cfg.preset.as_deref().unwrap_or("(none)")
    );
    println!("  palette            = {}", cfg.palette);
    println!("  pattern            = {}", cfg.pattern);
    println!("  color              = {}", cfg.color);
    let cap_detail = match cfg.resolved_caption() {
        Some(resolved) if cfg.caption.eq_ignore_ascii_case("auto") => {
            format!("auto ({resolved})")
        }
        Some(resolved) => resolved,
        None => "off".to_string(),
    };
    println!("  caption            = {cap_detail}");
    if !cfg.disabled_palettes.is_empty() {
        println!(
            "  disabled_palettes  = {}",
            cfg.disabled_palettes.join(", ")
        );
    }
    if !cfg.disabled_patterns.is_empty() {
        println!(
            "  disabled_patterns  = {}",
            cfg.disabled_patterns.join(", ")
        );
    }
    if !cfg.palettes.is_empty() {
        println!("\nCustom palettes:");
        for (name, p) in &cfg.palettes {
            println!("  {name:<16} {} -> {}", p.from, p.to);
        }
    }
    if !cfg.presets.is_empty() {
        println!("\nPresets:");
        for (name, pr) in &cfg.presets {
            println!(
                "  {name:<16} palette={} pattern={} lod={}",
                pr.palette.as_deref().unwrap_or("default"),
                pr.pattern.as_deref().unwrap_or("default"),
                pr.lod.as_deref().unwrap_or("auto"),
            );
        }
    }
    Ok(())
}

pub fn cmd_config_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let k = key.trim().to_ascii_lowercase().replace('-', "_");
    let v = value.trim();
    let vl = v.to_ascii_lowercase();

    match k.as_str() {
        "display" => {
            validate_choice(&k, &vl, DISPLAY_MODES)?;
            cfg.display = vl;
        }
        "shell_reminder" | "shell" | "reminder" => {
            validate_choice("shell_reminder", &vl, DISPLAY_MODES)?;
            cfg.shell_reminder = vl;
        }
        "align" => {
            validate_choice(&k, &vl, ALIGN_MODES)?;
            cfg.align = vl;
        }
        "animate" | "animation" => {
            cfg.animate = parse_bool(&k, v)?;
        }
        "lid_by_default" | "lid" => {
            cfg.lid_by_default = parse_bool("lid_by_default", v)?;
        }
        "clear_on_shell" | "clear" => {
            cfg.clear_on_shell = parse_bool("clear_on_shell", v)?;
        }
        "preset" => {
            if vl.is_empty() || vl == "none" || vl == "off" {
                cfg.preset = None;
            } else if cfg.presets.contains_key(&vl) {
                cfg.preset = Some(vl);
            } else {
                bail!("unknown preset `{v}` (add one first with `caf config add-preset {v} ...`)");
            }
        }
        "palette" | "mug" => {
            if vl != "random" && !cfg.has_palette_name(&vl) {
                bail!(
                    "unknown palette `{v}` (valid: random, {}; or add custom via `caf config add-palette`)",
                    BUILTIN_PALETTE_NAMES.join(", ")
                );
            }
            cfg.palette = vl;
        }
        "pattern" => {
            if vl != "random" && !PATTERN_NAMES.contains(&vl.as_str()) {
                bail!(
                    "unknown pattern `{v}` (valid: random, {})",
                    PATTERN_NAMES.join(", ")
                );
            }
            cfg.pattern = vl;
        }
        "color" => {
            validate_choice(&k, &vl, COLOR_MODES)?;
            cfg.color = vl;
        }
        "caption" => {
            cfg.caption = v.to_string();
        }
        _ => {
            bail!(
                "unknown config key `{key}` (valid: display, shell_reminder, align, animate, lid_by_default, clear_on_shell, preset, palette, pattern, color, caption)"
            );
        }
    }

    let path = save_config(&cfg)?;
    println!("Updated `{k}` = `{v}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_disable_palette(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    if !cfg.has_palette_name(&lower) {
        bail!("unknown palette `{name}`");
    }
    if !cfg
        .disabled_palettes
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&lower))
    {
        cfg.disabled_palettes.push(lower.clone());
        cfg.disabled_palettes.sort();
    }
    let path = save_config(&cfg)?;
    println!("Disabled palette `{lower}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_enable_palette(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    let before = cfg.disabled_palettes.len();
    cfg.disabled_palettes
        .retain(|p| !p.eq_ignore_ascii_case(&lower));
    if cfg.disabled_palettes.len() == before {
        println!("Palette `{lower}` was not disabled.");
        return Ok(());
    }
    let path = save_config(&cfg)?;
    println!("Re-enabled palette `{lower}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_disable_pattern(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    if !PATTERN_NAMES.contains(&lower.as_str()) {
        bail!(
            "unknown pattern `{name}` (valid: {})",
            PATTERN_NAMES.join(", ")
        );
    }
    if !cfg
        .disabled_patterns
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&lower))
    {
        cfg.disabled_patterns.push(lower.clone());
        cfg.disabled_patterns.sort();
    }
    let path = save_config(&cfg)?;
    println!("Disabled pattern `{lower}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_enable_pattern(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    let before = cfg.disabled_patterns.len();
    cfg.disabled_patterns
        .retain(|p| !p.eq_ignore_ascii_case(&lower));
    if cfg.disabled_patterns.len() == before {
        println!("Pattern `{lower}` was not disabled.");
        return Ok(());
    }
    let path = save_config(&cfg)?;
    println!("Re-enabled pattern `{lower}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_add_palette(name: &str, from: &str, to: &str) -> Result<()> {
    let clean = validate_identifier(name, "palette")?;
    let _ = parse_color_spec(from)?;
    let _ = parse_color_spec(to)?;
    let mut cfg = load_config()?;
    cfg.palettes.insert(
        clean.clone(),
        CustomPalette {
            from: from.trim().to_string(),
            to: to.trim().to_string(),
        },
    );
    let path = save_config(&cfg)?;
    println!(
        "Saved custom palette `{clean}` ({from} -> {to}) in {}",
        path.display()
    );
    Ok(())
}

pub fn cmd_config_rm_palette(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    if cfg.palettes.remove(&lower).is_none() {
        bail!("no custom palette `{name}` found in config.toml");
    }
    let path = save_config(&cfg)?;
    println!("Removed custom palette `{lower}` from {}", path.display());
    Ok(())
}

pub fn cmd_config_add_preset(
    name: &str,
    palette: Option<&str>,
    pattern: Option<&str>,
    lod: Option<&str>,
) -> Result<()> {
    let clean = validate_identifier(name, "preset")?;
    let mut cfg = load_config()?;
    if let Some(p) = palette {
        let pl = p.trim().to_ascii_lowercase();
        if pl != "random" && !cfg.has_palette_name(&pl) {
            bail!("unknown palette `{p}`");
        }
    }
    if let Some(pat) = pattern {
        let pl = pat.trim().to_ascii_lowercase();
        if pl != "random" && !PATTERN_NAMES.contains(&pl.as_str()) {
            bail!("unknown pattern `{pat}`");
        }
    }
    if let Some(l) = lod {
        let ll = l.trim().to_ascii_lowercase();
        validate_choice("lod", &ll, DISPLAY_MODES)?;
    }
    cfg.presets.insert(
        clean.clone(),
        PresetSpec {
            palette: palette.map(|s| s.trim().to_ascii_lowercase()),
            pattern: pattern.map(|s| s.trim().to_ascii_lowercase()),
            lod: lod.map(|s| s.trim().to_ascii_lowercase()),
        },
    );
    let path = save_config(&cfg)?;
    println!("Saved preset `{clean}` in {}", path.display());
    Ok(())
}

pub fn cmd_config_rm_preset(name: &str) -> Result<()> {
    let mut cfg = load_config()?;
    let lower = name.trim().to_ascii_lowercase();
    if cfg.presets.remove(&lower).is_none() {
        bail!("no preset `{name}` found in config.toml");
    }
    if cfg.preset.as_deref() == Some(lower.as_str()) {
        cfg.preset = None;
    }
    let path = save_config(&cfg)?;
    println!("Removed preset `{lower}` from {}", path.display());
    Ok(())
}

pub fn cmd_config_reset() -> Result<()> {
    let cfg = CafConfig::default();
    let path = save_config(&cfg)?;
    println!("Reset caf configuration to defaults in {}", path.display());
    Ok(())
}

pub fn cmd_config_path() -> Result<()> {
    println!("{}", config_path()?.display());
    Ok(())
}

fn parse_bool(key: &str, raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => bail!("invalid value `{raw}` for `{key}` (valid: true, false)"),
    }
}

fn validate_choice(key: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if !allowed.contains(&value) {
        bail!(
            "invalid value `{value}` for `{key}` (valid: {})",
            allowed.join(", ")
        );
    }
    Ok(())
}

fn validate_identifier(raw: &str, kind: &str) -> Result<String> {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty()
        || lower == "random"
        || !lower
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("{kind} name `{raw}` must contain only ASCII letters, digits, '-' or '_'");
    }
    Ok(lower)
}
