use std::env;
use std::io::{IsTerminal, Write, stdout};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};

use crate::config::{CafConfig, parse_color_spec};

pub const BUILTIN_PALETTE_NAMES: &[&str] = &[
    "cyan-pink",
    "pink-black",
    "purple-black",
    "orange-red",
    "navy-teal",
    "amber-roast",
    "matcha",
    "sakura",
    "aurora",
    "ember",
    "mono",
    "thailand",
    "america",
];

pub const PATTERN_NAMES: &[&str] = &[
    "gradient", "diagonal", "wave", "chevron", "checker", "stripes", "dots", "plasma",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    Full,
    Medium,
    Compact,
    Oneline,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Gradient,
    Diagonal,
    Wave,
    Chevron,
    Checker,
    Stripes,
    Dots,
    Plasma,
}

impl Pattern {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "gradient" => Ok(Self::Gradient),
            "diagonal" => Ok(Self::Diagonal),
            "wave" => Ok(Self::Wave),
            "chevron" => Ok(Self::Chevron),
            "checker" => Ok(Self::Checker),
            "stripes" => Ok(Self::Stripes),
            "dots" => Ok(Self::Dots),
            "plasma" => Ok(Self::Plasma),
            other => bail!(
                "unknown pattern `{other}` (valid: random, {})",
                PATTERN_NAMES.join(", ")
            ),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Gradient => "gradient",
            Self::Diagonal => "diagonal",
            Self::Wave => "wave",
            Self::Chevron => "chevron",
            Self::Checker => "checker",
            Self::Stripes => "stripes",
            Self::Dots => "dots",
            Self::Plasma => "plasma",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Gradient,
            Self::Diagonal,
            Self::Wave,
            Self::Chevron,
            Self::Checker,
            Self::Stripes,
            Self::Dots,
            Self::Plasma,
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
        }
    }

    pub fn scale(self, factor: f32) -> Self {
        let s = |v: u8| -> u8 { (f32::from(v) * factor).round().clamp(0.0, 255.0) as u8 };
        Self {
            r: s(self.r),
            g: s(self.g),
            b: s(self.b),
        }
    }

    pub fn paint(self, text: &str, use_color: bool) -> String {
        if !use_color || text.is_empty() {
            return text.to_string();
        }
        format!("\x1b[38;2;{};{};{}m{text}\x1b[0m", self.r, self.g, self.b)
    }

    pub fn paint_on(self, bg: Self, text: &str, use_color: bool) -> String {
        if !use_color || text.is_empty() {
            return text.to_string();
        }
        format!(
            "\x1b[38;2;{};{};{};48;2;{};{};{}m{text}\x1b[0m",
            self.r, self.g, self.b, bg.r, bg.g, bg.b
        )
    }
}

const MAX_STOPS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlagStyle {
    None,
    Thailand,
    America,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorRamp {
    stops: [Rgb; MAX_STOPS],
    len: usize,
    stepped: bool,
    flag: FlagStyle,
}

impl ColorRamp {
    const fn two(a: Rgb, b: Rgb) -> Self {
        Self {
            stops: [a, b, b, b, b, b],
            len: 2,
            stepped: false,
            flag: FlagStyle::None,
        }
    }

    const fn three_flag(a: Rgb, b: Rgb, c: Rgb, flag: FlagStyle) -> Self {
        Self {
            stops: [a, b, c, c, c, c],
            len: 3,
            stepped: true,
            flag,
        }
    }

    const fn six_flag(a: Rgb, b: Rgb, c: Rgb, d: Rgb, e: Rgb, f: Rgb, flag: FlagStyle) -> Self {
        Self {
            stops: [a, b, c, d, e, f],
            len: 6,
            stepped: true,
            flag,
        }
    }

    fn sample(self, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        if self.len <= 1 {
            return self.stops[0];
        }
        if self.stepped {
            let idx = ((t * self.len as f32).floor() as usize).min(self.len - 1);
            return self.stops[idx];
        }
        let pos = t * (self.len - 1) as f32;
        let lo = (pos.floor() as usize).min(self.len - 2);
        let frac = pos - lo as f32;
        self.stops[lo].lerp(self.stops[lo + 1], frac)
    }

    fn cool_to(self, slate: Rgb, amount: f32) -> Self {
        let mut stops = self.stops;
        for stop in stops.iter_mut().take(self.len) {
            *stop = stop.lerp(slate, amount);
        }
        Self { stops, ..self }
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        let mut stops = self.stops;
        for (dst, src) in stops.iter_mut().zip(other.stops).take(self.len) {
            *dst = dst.lerp(src, t);
        }
        Self { stops, ..self }
    }

    fn describe(self) -> String {
        if self.len == 2 {
            let (a, b) = (self.stops[0], self.stops[1]);
            return format!(
                "#{:02x}{:02x}{:02x} -> #{:02x}{:02x}{:02x}",
                a.r, a.g, a.b, b.r, b.g, b.b
            );
        }
        let mut parts: Vec<String> = Vec::new();
        for i in 0..self.len {
            let c = self.stops[i];
            if i > 0 {
                let prev = self.stops[i - 1];
                if prev.r == c.r && prev.g == c.g && prev.b == c.b {
                    continue;
                }
            }
            parts.push(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone)]
pub struct MugProfile {
    pub name: String,
    pub ramp: ColorRamp,
    pub custom: bool,
}

const TH_RED: Rgb = Rgb::new(239, 51, 64);
const TH_WHITE: Rgb = Rgb::new(244, 246, 250);
const TH_BLUE: Rgb = Rgb::new(36, 48, 138);

const US_RED: Rgb = Rgb::new(220, 38, 56);
const US_WHITE: Rgb = Rgb::new(245, 247, 252);
const US_BLUE: Rgb = Rgb::new(30, 68, 162);

const BUILTIN_PROFILES: &[(&str, ColorRamp)] = &[
    (
        "cyan-pink",
        ColorRamp::two(Rgb::new(0, 220, 220), Rgb::new(220, 100, 180)),
    ),
    (
        "pink-black",
        ColorRamp::two(Rgb::new(255, 20, 147), Rgb::new(40, 16, 30)),
    ),
    (
        "purple-black",
        ColorRamp::two(Rgb::new(155, 70, 225), Rgb::new(38, 20, 58)),
    ),
    (
        "orange-red",
        ColorRamp::two(Rgb::new(245, 155, 35), Rgb::new(175, 35, 25)),
    ),
    (
        "navy-teal",
        ColorRamp::two(Rgb::new(35, 65, 165), Rgb::new(0, 185, 170)),
    ),
    (
        "amber-roast",
        ColorRamp::two(Rgb::new(242, 185, 80), Rgb::new(115, 48, 18)),
    ),
    (
        "matcha",
        ColorRamp::two(Rgb::new(134, 224, 127), Rgb::new(28, 96, 68)),
    ),
    (
        "sakura",
        ColorRamp::two(Rgb::new(255, 172, 199), Rgb::new(120, 50, 104)),
    ),
    (
        "aurora",
        ColorRamp::two(Rgb::new(56, 242, 200), Rgb::new(90, 66, 232)),
    ),
    (
        "ember",
        ColorRamp::two(Rgb::new(255, 154, 82), Rgb::new(122, 25, 41)),
    ),
    (
        "mono",
        ColorRamp::two(Rgb::new(228, 224, 216), Rgb::new(88, 94, 104)),
    ),
    (
        "thailand",
        ColorRamp::six_flag(
            TH_RED,
            TH_WHITE,
            TH_BLUE,
            TH_BLUE,
            TH_WHITE,
            TH_RED,
            FlagStyle::Thailand,
        ),
    ),
    (
        "america",
        ColorRamp::three_flag(US_RED, US_WHITE, US_BLUE, FlagStyle::America),
    ),
];

/// Returns all defined profiles (built-ins + valid user custom palettes from config.toml).
pub fn all_profiles(cfg: &CafConfig) -> Vec<MugProfile> {
    let mut list: Vec<MugProfile> = BUILTIN_PROFILES
        .iter()
        .map(|&(name, ramp)| MugProfile {
            name: name.to_string(),
            ramp,
            custom: false,
        })
        .collect();
    for (name, spec) in &cfg.palettes {
        if let (Ok(a), Ok(b)) = (parse_color_spec(&spec.from), parse_color_spec(&spec.to)) {
            let lower = name.to_ascii_lowercase();
            let ramp = ColorRamp::two(a, b);
            if let Some(existing) = list.iter_mut().find(|p| p.name == lower) {
                existing.ramp = ramp;
                existing.custom = true;
            } else {
                list.push(MugProfile {
                    name: lower,
                    ramp,
                    custom: true,
                });
            }
        }
    }
    list
}

/// Returns profiles eligible for `random` picks and live arrow cycling (respecting `disabled_palettes`).
pub fn enabled_profiles(cfg: &CafConfig) -> Vec<MugProfile> {
    let all = all_profiles(cfg);
    let filtered: Vec<MugProfile> = all
        .iter()
        .filter(|p| {
            !cfg.disabled_palettes
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&p.name))
        })
        .cloned()
        .collect();
    if filtered.is_empty() { all } else { filtered }
}

/// Returns glaze patterns eligible for `random` picks and live arrow cycling (respecting `disabled_patterns`).
pub fn enabled_patterns(cfg: &CafConfig) -> Vec<Pattern> {
    let filtered: Vec<Pattern> = Pattern::all()
        .iter()
        .copied()
        .filter(|p| {
            !cfg.disabled_patterns
                .iter()
                .any(|d| d.eq_ignore_ascii_case(p.name()))
        })
        .collect();
    if filtered.is_empty() {
        Pattern::all().to_vec()
    } else {
        filtered
    }
}

pub fn pick_random_combo(cfg: &CafConfig) -> (String, Pattern) {
    let seed = entropy_seed();
    let profs = enabled_profiles(cfg);
    let pats = enabled_patterns(cfg);
    let p = profs[(seed as usize) % profs.len()].name.clone();
    let g = pats[((seed ^ 0x9e37_79b9) as usize) % pats.len()];
    (p, g)
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    steam_dim: Rgb,
    steam_mid: Rgb,
    steam_bright: Rgb,
    rim_inner: Rgb,
    coffee_surf: Rgb,
    coffee_deep: Rgb,
    cup: Rgb,
    cup_dark: Rgb,
    saucer: Rgb,
    saucer_edge: Rgb,
    meta: Rgb,
    ramp: ColorRamp,
}

impl Palette {
    fn warm(profile: &MugProfile) -> Self {
        Self {
            steam_dim: Rgb::new(140, 100, 55),
            steam_mid: Rgb::new(185, 145, 85),
            steam_bright: Rgb::new(230, 195, 140),
            rim_inner: Rgb::new(230, 190, 110),
            coffee_surf: Rgb::new(120, 72, 24),
            coffee_deep: Rgb::new(72, 38, 10),
            cup: Rgb::new(212, 202, 188),
            cup_dark: Rgb::new(170, 160, 145),
            saucer: Rgb::new(200, 190, 175),
            saucer_edge: Rgb::new(160, 150, 135),
            meta: Rgb::new(150, 146, 140),
            ramp: profile.ramp,
        }
    }

    fn cool(profile: &MugProfile) -> Self {
        let slate = Rgb::new(115, 122, 135);
        Self {
            steam_dim: Rgb::new(70, 75, 85),
            steam_mid: Rgb::new(90, 95, 105),
            steam_bright: Rgb::new(110, 115, 125),
            rim_inner: Rgb::new(148, 158, 172),
            coffee_surf: Rgb::new(58, 63, 74),
            coffee_deep: Rgb::new(40, 45, 54),
            cup: Rgb::new(155, 160, 170),
            cup_dark: Rgb::new(118, 124, 134),
            saucer: Rgb::new(142, 148, 158),
            saucer_edge: Rgb::new(108, 114, 124),
            meta: Rgb::new(120, 126, 136),
            ramp: profile.ramp.cool_to(slate, 0.45),
        }
    }

    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            steam_dim: self.steam_dim.lerp(other.steam_dim, t),
            steam_mid: self.steam_mid.lerp(other.steam_mid, t),
            steam_bright: self.steam_bright.lerp(other.steam_bright, t),
            rim_inner: self.rim_inner.lerp(other.rim_inner, t),
            coffee_surf: self.coffee_surf.lerp(other.coffee_surf, t),
            coffee_deep: self.coffee_deep.lerp(other.coffee_deep, t),
            cup: self.cup.lerp(other.cup, t),
            cup_dark: self.cup_dark.lerp(other.cup_dark, t),
            saucer: self.saucer.lerp(other.saucer, t),
            saucer_edge: self.saucer_edge.lerp(other.saucer_edge, t),
            meta: self.meta.lerp(other.meta, t),
            ramp: self.ramp.lerp(other.ramp, t),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderRequest<'a> {
    pub active: bool,
    pub lid: bool,
    pub dark: bool,
    pub dark_in: Option<&'a str>,
    pub timer: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub hint: Option<&'a str>,
    pub is_shell: bool,
    pub preset_override: Option<&'a str>,
    pub lod_override: Option<&'a str>,
    pub palette_override: Option<&'a str>,
    pub pattern_override: Option<&'a str>,
    pub width_override: Option<usize>,
    pub height_override: Option<usize>,
    pub align_override: Option<&'a str>,
    pub frame: usize,
    /// Optional 0.0 (warm) .. 1.0 (cool) override for decaf cool-down transitions
    pub cool_blend: Option<f32>,
    /// 0.0 = full steam, 1.0 = fully dissipated steam
    pub steam_fade: f32,
}

#[derive(Debug, Clone)]
pub struct RenderedArt {
    pub lines: Vec<String>,
    pub palette_name: String,
    pub pattern: Pattern,
}

pub fn render_banner(cfg: &CafConfig, req: &RenderRequest<'_>) -> Result<Option<RenderedArt>> {
    let rendered = build_art(cfg, req)?;
    if let Some(ref art) = rendered {
        let mut out = stdout().lock();
        if req.is_shell && cfg.clear_on_shell && stdout().is_terminal() {
            write!(out, "\x1b[2J\x1b[H")?;
        }
        let multi_line = art.lines.len() > 1;
        if multi_line {
            writeln!(out)?;
        }
        for line in &art.lines {
            writeln!(out, "{line}")?;
        }
        if multi_line {
            writeln!(out)?;
        }
        out.flush()?;
    }
    Ok(rendered)
}

pub fn build_art(cfg: &CafConfig, req: &RenderRequest<'_>) -> Result<Option<RenderedArt>> {
    let (detected_w, detected_h) = detect_terminal_size();
    let term_width = req.width_override.unwrap_or(detected_w);
    let term_height = req.height_override.unwrap_or(detected_h);

    // Resolve optional preset (CLI `--preset` overrides config `preset`)
    let preset_name = req.preset_override.or(cfg.preset.as_deref());
    let preset = match preset_name {
        Some(name) => Some(cfg.presets.get(&name.to_ascii_lowercase()).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown preset `{name}` (add one with `caf config add-preset {name} ...`)"
            )
        })?),
        None => None,
    };

    let mode_raw = req
        .lod_override
        .or_else(|| preset.and_then(|pr| pr.lod.as_deref()))
        .unwrap_or(if req.is_shell {
            cfg.shell_reminder.as_str()
        } else {
            cfg.display.as_str()
        });
    let lod = resolve_lod(mode_raw, term_width, term_height)?;
    if lod == Lod::Off {
        return Ok(None);
    }

    let seed = entropy_seed();
    let palette_raw = req
        .palette_override
        .or_else(|| preset.and_then(|pr| pr.palette.as_deref()))
        .unwrap_or(&cfg.palette);
    let pattern_raw = req
        .pattern_override
        .or_else(|| preset.and_then(|pr| pr.pattern.as_deref()))
        .unwrap_or(&cfg.pattern);

    let profile = resolve_profile(cfg, palette_raw, seed)?;
    let pattern = resolve_pattern(cfg, pattern_raw, seed ^ 0x9e37_79b9)?;
    let align = resolve_align(req.align_override.unwrap_or(&cfg.align))?;
    let use_color = should_use_color(&cfg.color);

    let warm = Palette::warm(&profile);
    let cool = Palette::cool(&profile);
    let blend_t = req
        .cool_blend
        .unwrap_or(if req.active { 0.0 } else { 1.0 })
        .clamp(0.0, 1.0);
    let palette = warm.blend(cool, blend_t);

    let show_steam = req.steam_fade < 1.0 && (req.active || req.cool_blend.is_some());
    let title = status_title(req.active);
    let meta = build_meta_line(
        req.active,
        req.lid,
        req.dark,
        req.dark_in,
        req.timer,
        req.reason,
    );
    let caption = cfg.resolved_caption().unwrap_or_default();

    let explicit_oneline = matches!(
        mode_raw.trim().to_ascii_lowercase().as_str(),
        "oneline" | "one-line" | "line"
    );

    let lines = match lod {
        Lod::Off => Vec::new(),
        Lod::Oneline => render_micro_or_oneline(
            palette,
            req.active,
            &title,
            &caption,
            &meta,
            req.hint,
            term_width,
            term_height,
            align,
            explicit_oneline,
            use_color,
        ),
        Lod::Compact => render_compact(
            palette,
            pattern,
            show_steam,
            req.steam_fade,
            &title,
            &caption,
            &meta,
            req.hint,
            term_width,
            term_height,
            align,
            use_color,
            req.frame,
        ),
        Lod::Medium => render_medium(
            palette,
            pattern,
            show_steam,
            req.steam_fade,
            &title,
            &caption,
            &meta,
            req.hint,
            term_width,
            align,
            use_color,
            req.frame,
        ),
        Lod::Full => render_full(
            palette,
            pattern,
            show_steam,
            req.steam_fade,
            &title,
            &caption,
            &meta,
            req.hint,
            term_width,
            term_height,
            align,
            use_color,
            req.frame,
        ),
    };

    Ok(Some(RenderedArt {
        lines,
        palette_name: profile.name,
        pattern,
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_logo_gallery(
    cfg: &CafConfig,
    gallery: Option<&str>,
    list_only: bool,
    palette_override: Option<&str>,
    pattern_override: Option<&str>,
    preset_override: Option<&str>,
) -> Result<()> {
    if list_only {
        println!("Palettes:");
        for prof in all_profiles(cfg) {
            let disabled = cfg
                .disabled_palettes
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&prof.name));
            let tag = match (prof.custom, disabled) {
                (true, true) => " [custom, disabled]",
                (true, false) => " [custom]",
                (false, true) => " [disabled]",
                (false, false) => "",
            };
            println!("  {:<16} {}{tag}", prof.name, prof.ramp.describe());
        }
        println!("\nPatterns:");
        for pat in Pattern::all() {
            let disabled = cfg
                .disabled_patterns
                .iter()
                .any(|d| d.eq_ignore_ascii_case(pat.name()));
            let tag = if disabled { " [disabled]" } else { "" };
            println!("  {}{tag}", pat.name());
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
        return Ok(());
    }

    let mode = gallery.unwrap_or("all").trim().to_ascii_lowercase();
    let (show_lods, show_patterns, show_palettes) = match mode.as_str() {
        "all" => (true, true, true),
        "lods" | "lod" => (true, false, false),
        "patterns" | "pattern" => (false, true, false),
        "palettes" | "palette" | "colors" => (false, false, true),
        other => bail!("unknown gallery `{other}` (valid: all, lods, patterns, palettes)"),
    };

    let (term_w, term_h) = detect_terminal_size();

    if show_lods {
        println!("# caf LOD gallery\n");
        for (label, sim_w, sim_h) in [
            ("full (50+ cols x 25+ rows)", 60usize, 30usize),
            ("medium (33..49 cols or 17..24 rows)", 40usize, 22usize),
            ("compact (19..32 cols or 8..16 rows)", 26usize, 18usize),
            ("micro (< 19 cols or < 8 rows)", 18usize, 6usize),
        ] {
            let eff_w = sim_w.min(term_w.max(18));
            println!("--- {label} ---");
            let _ = render_banner(
                cfg,
                &RenderRequest {
                    active: true,
                    lid: true,
                    dark: false,
                    dark_in: Some("10m"),
                    timer: Some("1h 20m left"),
                    reason: Some("build running"),
                    hint: None,
                    is_shell: false,
                    preset_override,
                    lod_override: Some("auto"),
                    palette_override: palette_override.or(Some("cyan-pink")),
                    pattern_override: pattern_override.or(Some("checker")),
                    width_override: Some(eff_w),
                    height_override: Some(sim_h),
                    align_override: Some("left"),
                    frame: 0,
                    cool_blend: None,
                    steam_fade: 0.0,
                },
            )?;
        }
    }

    if show_patterns {
        println!("# caf glaze patterns ({})\n", PATTERN_NAMES.join(", "));
        let lod = if term_w >= 33 { "medium" } else { "compact" };
        for pat in Pattern::all() {
            println!("--- pattern: {} ---", pat.name());
            let _ = render_banner(
                cfg,
                &RenderRequest {
                    active: true,
                    lid: true,
                    dark: false,
                    dark_in: None,
                    timer: None,
                    reason: Some(pat.name()),
                    hint: None,
                    is_shell: false,
                    preset_override,
                    lod_override: Some(lod),
                    palette_override: palette_override.or(Some("aurora")),
                    pattern_override: Some(pat.name()),
                    width_override: Some(term_w),
                    height_override: Some(term_h),
                    align_override: Some("left"),
                    frame: 0,
                    cool_blend: None,
                    steam_fade: 0.0,
                },
            )?;
        }
    }

    if show_palettes {
        println!("# caf palettes\n");
        for prof in all_profiles(cfg) {
            let disabled = cfg
                .disabled_palettes
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&prof.name));
            let label = if disabled {
                format!("{} (disabled)", prof.name)
            } else if prof.custom {
                format!("{} (custom)", prof.name)
            } else {
                prof.name.clone()
            };
            let _ = render_banner(
                cfg,
                &RenderRequest {
                    active: true,
                    lid: true,
                    dark: false,
                    dark_in: None,
                    timer: None,
                    reason: Some(&label),
                    hint: None,
                    is_shell: false,
                    preset_override: None,
                    lod_override: Some("compact"),
                    palette_override: Some(&prof.name),
                    pattern_override: pattern_override.or(Some("checker")),
                    width_override: Some(term_w),
                    height_override: Some(12), // side-by-side compact rows
                    align_override: Some("left"),
                    frame: 0,
                    cool_blend: None,
                    steam_fade: 0.0,
                },
            )?;
        }
    }

    Ok(())
}

pub fn detect_terminal_size() -> (usize, usize) {
    if let Ok((cols, rows)) = crossterm::terminal::size()
        && cols > 0
        && rows > 0
    {
        return (usize::from(cols), usize::from(rows));
    }

    let cols = env::var("COLUMNS")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(80);
    let rows = env::var("LINES")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(24);
    (cols, rows)
}

fn resolve_lod(raw: &str, term_width: usize, term_height: usize) -> Result<Lod> {
    let auto_lod = if term_width >= 50 && term_height >= 25 {
        Lod::Full
    } else if term_width >= 33 && term_height >= 17 {
        Lod::Medium
    } else if term_width >= 19 && term_height >= 8 {
        Lod::Compact
    } else {
        Lod::Oneline
    };

    let requested = match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "none" | "false" => return Ok(Lod::Off),
        "oneline" | "one-line" | "line" => Lod::Oneline,
        "compact" | "mini" | "small" => Lod::Compact,
        "medium" | "med" => Lod::Medium,
        "full" | "large" => Lod::Full,
        "auto" => auto_lod,
        other => bail!(
            "unknown display mode `{other}` (valid: auto, full, medium, compact, oneline, off)"
        ),
    };

    let safe = match requested {
        Lod::Full if term_width < 50 => {
            if term_width >= 33 {
                Lod::Medium
            } else if term_width >= 19 {
                Lod::Compact
            } else {
                Lod::Oneline
            }
        }
        Lod::Medium if term_width < 33 => {
            if term_width >= 19 {
                Lod::Compact
            } else {
                Lod::Oneline
            }
        }
        Lod::Compact if term_width < 19 => Lod::Oneline,
        other => other,
    };
    Ok(safe)
}

fn resolve_profile(cfg: &CafConfig, raw: &str, seed: u64) -> Result<MugProfile> {
    let clean = raw.trim().to_ascii_lowercase();
    if clean.is_empty() || clean == "random" {
        let pool = enabled_profiles(cfg);
        return Ok(pool[(seed as usize) % pool.len()].clone());
    }
    let all = all_profiles(cfg);
    all.into_iter().find(|p| p.name == clean).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown palette `{raw}` (run `caf logo --list` to see built-in & custom palettes)"
        )
    })
}

fn resolve_pattern(cfg: &CafConfig, raw: &str, seed: u64) -> Result<Pattern> {
    let clean = raw.trim().to_ascii_lowercase();
    if clean.is_empty() || clean == "random" {
        let pool = enabled_patterns(cfg);
        return Ok(pool[(seed as usize) % pool.len()]);
    }
    Pattern::parse(&clean)
}

fn resolve_align(raw: &str) -> Result<Align> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "center" | "centre" => Ok(Align::Center),
        "left" => Ok(Align::Left),
        other => bail!("unknown align `{other}` (valid: center, left)"),
    }
}

fn should_use_color(mode: &str) -> bool {
    match mode.trim().to_ascii_lowercase().as_str() {
        "always" => true,
        "never" => false,
        _ => env::var_os("NO_COLOR").is_none() && stdout().is_terminal(),
    }
}

fn status_title(active: bool) -> String {
    let base = if active {
        "Caffeinated"
    } else {
        "Decaffeinated"
    };
    if cfg!(target_os = "macos") {
        format!("Mac {base}")
    } else {
        base.to_string()
    }
}

fn build_meta_line(
    active: bool,
    lid: bool,
    dark: bool,
    dark_in: Option<&str>,
    timer: Option<&str>,
    reason: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if active {
        if !lid {
            parts.push("lid-open only".to_string());
        }
        if dark {
            parts.push("dark".to_string());
        } else if let Some(d) = dark_in
            && !d.trim().is_empty()
        {
            parts.push(format!("dark in {}", d.trim()));
        }
    }
    if let Some(t) = timer
        && !t.trim().is_empty()
    {
        parts.push(t.trim().to_string());
    }
    if let Some(r) = reason
        && !r.trim().is_empty()
    {
        parts.push(r.trim().to_string());
    }
    if active && parts.is_empty() {
        parts.push("lid-close safe".to_string());
    }
    parts.join(" · ")
}

fn paint_gradient_text(text: &str, ramp: ColorRamp, use_color: bool) -> String {
    if !use_color {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let denom = (chars.len().saturating_sub(1)).max(1) as f32;
    let mut out = String::new();
    for (i, ch) in chars.into_iter().enumerate() {
        let t = (i as f32) / denom;
        let c = ramp.sample(t);
        let mut s = [0u8; 4];
        out.push_str(&c.paint(ch.encode_utf8(&mut s), true));
    }
    out
}

fn left_pad_for(align: Align, max_content_width: usize, term_width: usize) -> usize {
    match align {
        Align::Left => {
            if max_content_width + 2 <= term_width {
                2
            } else {
                0
            }
        }
        Align::Center => term_width.saturating_sub(max_content_width) / 2,
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = text.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

/// Tailored per-pattern ceramic glaze for the `america` palette across all 8 patterns.
#[allow(clippy::too_many_arguments)]
fn glaze_america(
    p: Palette,
    pattern: Pattern,
    col_x: usize,
    row_y: usize,
    height: usize,
    is_handle: bool,
    fx: f32,
    fy: f32,
    cyl: f32,
) -> (char, Rgb, Option<Rgb>) {
    let red = p.ramp.stops[0];
    let white = p.ramp.stops[1];
    let blue = p.ramp.stops[2];
    let star_ch = if height <= 5 { '·' } else { '*' };
    let rwbw = [red, white, blue, white];

    match pattern {
        Pattern::Gradient => {
            if !is_handle && fx < 0.44 && fy < 0.52 {
                let is_star = if row_y.is_multiple_of(2) {
                    col_x % 4 == 1
                } else {
                    col_x % 4 == 3
                };
                let bg = blue.scale(cyl);
                if is_star {
                    return (star_ch, white, Some(bg));
                }
                return ('█', bg, None);
            }
            let stripe = if row_y.is_multiple_of(2) { red } else { white };
            ('█', stripe.scale(cyl), None)
        }
        Pattern::Wave => {
            let ripple = (fx * std::f32::consts::TAU * 1.4).sin();
            let fy_w = (fy + ripple * 0.085).clamp(0.0, 0.999);
            let row_w = (fy_w * (height.max(1) as f32)).floor() as usize;
            let crest = ripple > 0.65;
            if !is_handle && fx < 0.44 && fy_w < 0.52 {
                let is_star = if row_w.is_multiple_of(2) {
                    col_x % 4 == 1
                } else {
                    col_x % 4 == 3
                };
                let bg = blue.scale(cyl);
                if is_star {
                    return (star_ch, white, Some(bg));
                }
                return (if crest { '▓' } else { '█' }, bg, None);
            }
            let stripe = if row_w.is_multiple_of(2) { red } else { white };
            (if crest { '▓' } else { '█' }, stripe.scale(cyl), None)
        }
        Pattern::Diagonal => {
            let band = ((col_x / 3) + row_y) % 4;
            ('█', rwbw[band].scale(cyl), None)
        }
        Pattern::Chevron => {
            let step = (col_x / 2) % 6;
            let zig = if step <= 3 { step } else { 6 - step };
            let band = (row_y + zig) % 4;
            ('█', rwbw[band].scale(cyl), None)
        }
        Pattern::Checker => {
            let alt = ((col_x / 2) + row_y).is_multiple_of(2);
            if !is_handle && fx < 0.44 && fy < 0.52 {
                let bg = blue.scale(cyl);
                if alt {
                    return ('█', bg, None);
                }
                if col_x.is_multiple_of(2) {
                    return (star_ch, white, Some(bg));
                }
                return ('▓', bg.lerp(white, 0.25), None);
            }
            let (ch, col) = if alt { ('█', red) } else { ('▓', white) };
            (ch, col.scale(cyl), None)
        }
        Pattern::Stripes => match row_y % 4 {
            0 => ('█', red.scale(cyl), None),
            2 => {
                let bg = blue.scale(cyl);
                if col_x % 4 == 1 {
                    (star_ch, white, Some(bg))
                } else {
                    ('█', bg, None)
                }
            }
            _ => ('█', white.scale(cyl), None),
        },
        Pattern::Dots => {
            let bg = blue.scale(cyl);
            let slot = (col_x + (row_y % 2) * 2) % 6;
            if slot == 1 {
                (star_ch, white, Some(bg))
            } else if slot == 4 {
                ('▓', red.scale(cyl), None)
            } else {
                ('█', bg, None)
            }
        }
        Pattern::Plasma => {
            let v1 = (fx * 6.0).sin();
            let v2 = (fy * 5.0).sin();
            let v3 = ((fx + fy) * 4.5).sin();
            let raw = (v1 + v2 + v3) / 6.0 + 0.5;
            let t = ((raw - 0.5) * 1.85 + 0.5).clamp(0.0, 0.999);
            let idx = ((t * 4.0).floor() as usize).min(3);
            ('█', rwbw[idx].scale(cyl), None)
        }
    }
}

/// Maps normalized vertical center `fy_c` (0..1) into the symmetrical 1:1:2:1:1 Thai Trairanga band.
fn thailand_band_color(
    fy_c: f32,
    row_y: usize,
    height: usize,
    red: Rgb,
    white: Rgb,
    blue: Rgb,
) -> Rgb {
    if height == 4 {
        return match row_y {
            0 | 3 => red,
            1 => white,
            _ => blue,
        };
    }
    let dist = (fy_c - 0.5).abs();
    if dist < (1.0 / 6.0) {
        blue
    } else if dist < (2.0 / 6.0) {
        white
    } else {
        red
    }
}

/// Tailored per-pattern ceramic glaze for the `thailand` palette across all 8 patterns.
#[allow(clippy::too_many_arguments)]
fn glaze_thailand(
    p: Palette,
    pattern: Pattern,
    col_x: usize,
    row_y: usize,
    height: usize,
    fx: f32,
    fy: f32,
    cyl: f32,
) -> (char, Rgb, Option<Rgb>) {
    let red = p.ramp.stops[0];
    let white = p.ramp.stops[1];
    let blue = p.ramp.stops[2];
    let trairanga = [red, white, blue, blue, white, red];
    let fy_c = (row_y as f32 + 0.5) / (height.max(1) as f32);

    match pattern {
        Pattern::Gradient => {
            let col = thailand_band_color(fy_c, row_y, height, red, white, blue);
            ('█', col.scale(cyl), None)
        }
        Pattern::Wave => {
            let ripple = (fx * std::f32::consts::TAU * 1.4).sin();
            let fy_w = (fy_c + ripple * 0.085).clamp(0.0, 0.999);
            let row_w = (fy_w * (height.max(1) as f32)).floor() as usize;
            let col = thailand_band_color(fy_w, row_w, height, red, white, blue);
            let ch = if ripple > 0.65 { '▓' } else { '█' };
            (ch, col.scale(cyl), None)
        }
        Pattern::Diagonal => {
            let band = ((col_x / 2) + row_y) % 6;
            ('█', trairanga[band].scale(cyl), None)
        }
        Pattern::Chevron => {
            let step = (col_x / 2) % 6;
            let zig = if step <= 3 { step } else { 6 - step };
            let band = (row_y + zig) % 6;
            ('█', trairanga[band].scale(cyl), None)
        }
        Pattern::Checker => {
            let alt = ((col_x / 2) + row_y).is_multiple_of(2);
            let base = thailand_band_color(fy_c, row_y, height, red, white, blue);
            let is_white = base.r == white.r && base.g == white.g && base.b == white.b;
            let (ch, col) = if alt {
                ('█', base)
            } else if is_white {
                ('▓', blue)
            } else {
                ('▓', white)
            };
            (ch, col.scale(cyl), None)
        }
        Pattern::Stripes => {
            let band = row_y % 6;
            let ch = if band == 3 { '▓' } else { '█' };
            (ch, trairanga[band].scale(cyl), None)
        }
        Pattern::Dots => {
            let base = thailand_band_color(fy_c, row_y, height, red, white, blue);
            let is_white = base.r == white.r && base.g == white.g && base.b == white.b;
            let dot = (col_x + (row_y % 2) * 2).is_multiple_of(4);
            if dot {
                let accent = if is_white { blue } else { white };
                ('▓', accent.scale(cyl), None)
            } else {
                ('█', base.scale(cyl), None)
            }
        }
        Pattern::Plasma => {
            let v1 = (fx * 6.0).sin();
            let v2 = (fy * 5.0).sin();
            let v3 = ((fx + fy) * 4.5).sin();
            let raw = (v1 + v2 + v3) / 6.0 + 0.5;
            let t = ((raw - 0.5) * 1.9 + 0.5).clamp(0.0, 0.999);
            let idx = ((t * 6.0).floor() as usize).min(5);
            ('█', trairanga[idx].scale(cyl), None)
        }
    }
}

/// Evaluates the per-cell ceramic glaze glyph and truecolor shade using exact physical
/// screen-space coordinates `(col_x, row_y)` so patterns align strictly corner-to-corner
/// across rim, tapered body walls (`│` vs `╰╮`), and handle.
fn glaze_cell(
    p: Palette,
    pattern: Pattern,
    col_x: usize,
    row_y: usize,
    width: usize,
    height: usize,
    is_handle: bool,
) -> (char, Rgb, Option<Rgb>) {
    let fx = (col_x as f32) / (width.saturating_sub(1).max(1) as f32);
    let fy = (row_y as f32) / (height.saturating_sub(1).max(1) as f32);

    let cyl = if is_handle {
        0.95
    } else {
        let dist_hi = (fx - 0.26).abs();
        let highlight = (1.0 - (dist_hi * 2.4).min(1.0)) * 0.14;
        let edge_shade = ((fx - 0.5).abs() * 2.0).powi(2) * 0.16;
        (1.0 + highlight - edge_shade).clamp(0.76, 1.15)
    };

    if p.ramp.flag == FlagStyle::America {
        return glaze_america(p, pattern, col_x, row_y, height, is_handle, fx, fy, cyl);
    }
    if p.ramp.flag == FlagStyle::Thailand {
        return glaze_thailand(p, pattern, col_x, row_y, height, fx, fy, cyl);
    }

    let (t, glyph) = match pattern {
        Pattern::Gradient => (fy, '█'),
        Pattern::Diagonal => {
            let phase = ((fx * 2.6 + fy * 1.9) * std::f32::consts::TAU).sin();
            let t = (fy * 0.65 + (phase * 0.5 + 0.5) * 0.35).clamp(0.0, 1.0);
            (t, '█')
        }
        Pattern::Wave => {
            let ripple = (fx * std::f32::consts::TAU * 2.0).sin() * 0.18;
            let band = ((fy + ripple) * 5.0).fract().abs();
            let t = (fy * 0.55 + band * 0.45).clamp(0.0, 1.0);
            let ch = if band > 0.78 { '▓' } else { '█' };
            (t, ch)
        }
        Pattern::Chevron => {
            let zig = ((fx * 4.0).fract() - 0.5).abs() * 2.0;
            let band = (fy * 3.0 + zig * 0.65).fract();
            let t = (fy * 0.5 + if band > 0.5 { 0.45 } else { 0.05 }).clamp(0.0, 1.0);
            (t, '█')
        }
        Pattern::Checker => {
            let alt = ((col_x / 2) + row_y).is_multiple_of(2);
            let t = if alt {
                (fy * 0.52).clamp(0.0, 1.0)
            } else {
                (0.48 + fy * 0.52).clamp(0.0, 1.0)
            };
            let ch = if alt { '█' } else { '▓' };
            (t, ch)
        }
        Pattern::Stripes => {
            let stripe = row_y % 3;
            let t = match stripe {
                0 => (fy * 0.4).clamp(0.0, 1.0),
                1 => (0.55 + fy * 0.45).clamp(0.0, 1.0),
                _ => (0.25 + fy * 0.5).clamp(0.0, 1.0),
            };
            let ch = if stripe == 2 { '▓' } else { '█' };
            (t, ch)
        }
        Pattern::Dots => {
            let dot = (col_x + (row_y % 2) * 2).is_multiple_of(4);
            let t = if dot {
                (1.0 - fy * 0.6).clamp(0.0, 1.0)
            } else {
                fy
            };
            let ch = if dot { '▓' } else { '█' };
            (t, ch)
        }
        Pattern::Plasma => {
            let v1 = (fx * 6.0).sin();
            let v2 = (fy * 5.0).sin();
            let v3 = ((fx + fy) * 4.5).sin();
            let t = ((v1 + v2 + v3) / 6.0 + 0.5).clamp(0.0, 1.0);
            (t, '█')
        }
    };

    let color = p.ramp.sample(t).scale(cyl);
    (glyph, color, None)
}

#[allow(clippy::too_many_arguments)]
fn render_glaze_span(
    p: Palette,
    pattern: Pattern,
    start_col: usize,
    row_y: usize,
    span_w: usize,
    total_w: usize,
    total_h: usize,
    is_handle: bool,
    use_color: bool,
) -> String {
    let mut out = String::new();
    for dx in 0..span_w {
        let (ch, col, bg) = glaze_cell(
            p,
            pattern,
            start_col + dx,
            row_y,
            total_w,
            total_h,
            is_handle,
        );
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        match bg {
            Some(bg_col) => out.push_str(&col.paint_on(bg_col, s, use_color)),
            None => out.push_str(&col.paint(s, use_color)),
        }
    }
    out
}

fn render_coffee_wave(
    p: Palette,
    dip_row: isize,
    dip_steps: usize,
    gap_w: usize,
    inset: usize,
    frame: usize,
    use_color: bool,
) -> String {
    if gap_w == 0 {
        return String::new();
    }
    let pad = inset.min(gap_w / 2);
    let fill_w = gap_w.saturating_sub(pad * 2);
    let eff_row = dip_row.max(0) as f32;
    let depth = eff_row / ((dip_steps.saturating_sub(1)).max(1) as f32);
    let shift = (frame as f32) * 0.14;

    let mut out = String::new();
    out.push_str(&" ".repeat(pad));
    for x in 0..fill_w {
        let fx = (x as f32) / (fill_w.max(1) as f32);
        let wave = (fx * std::f32::consts::TAU + shift).sin() * 0.45
            + (fx * std::f32::consts::PI * 3.7 + shift * 1.4).sin() * 0.30
            + (fx * std::f32::consts::PI * 7.1 + shift * 2.1).sin() * 0.15;
        let t = (fx + wave * 0.5).clamp(0.0, 1.0);
        let blend = (t * (1.0 - depth * 0.3) + depth * 0.5).clamp(0.0, 1.0);
        let col = p.coffee_surf.lerp(p.coffee_deep, blend);
        out.push_str(&col.paint("█", use_color));
    }
    out.push_str(&" ".repeat(pad));
    out
}

fn build_steam_rows(
    p: Palette,
    rows: usize,
    line_width: usize,
    frame: usize,
    steam_fade: f32,
    use_color: bool,
) -> Vec<String> {
    if steam_fade >= 1.0 {
        return vec![String::new(); rows];
    }
    let chars = ['∘', '°', '·', '.', '∘', '°'];
    let mut out = vec![String::new(); rows];
    let half = (line_width / 2) as isize;

    for row in 0..rows {
        let mut buf = vec![' '; line_width];
        let count = (rows + 1).saturating_sub(row).max(2);
        let spread = ((line_width / 4) + row).max(2) as isize;

        for j in 0..count {
            let h = hash_u64((frame as u64).wrapping_mul(31) ^ ((row as u64) << 8) ^ (j as u64));
            let keep_prob = ((h >> 16) % 1000) as f32 / 1000.0;
            if steam_fade > 0.0 && keep_prob < steam_fade {
                continue;
            }
            let base = ((h % (spread as u64 * 2 + 1)) as isize) - spread;
            let wave = ((((frame + row * 3 + j * 5) as f32) / 3.5).sin() * 1.8).round() as isize;
            let pos = half + base + wave;
            if pos >= 0 && (pos as usize) < line_width {
                let ci = (frame + row + j) % chars.len();
                buf[pos as usize] = chars[ci];
            }
        }

        let trimmed: String = buf.into_iter().collect::<String>().trim_end().to_string();
        if trimmed.is_empty() {
            out[rows - 1 - row] = String::new();
        } else {
            let t = (row as f32) / ((rows.saturating_sub(1)).max(1) as f32);
            let col = if t < 0.5 {
                p.steam_bright.lerp(p.steam_mid, t * 2.0)
            } else {
                p.steam_mid.lerp(p.steam_dim, (t - 0.5) * 2.0)
            };
            out[rows - 1 - row] = col.paint(&trimmed, use_color);
        }
    }
    out
}

// =========================================================================
// LOD 0 — Micro / Oneline
// =========================================================================
#[allow(clippy::too_many_arguments)]
fn render_micro_or_oneline(
    p: Palette,
    active: bool,
    title: &str,
    caption: &str,
    meta: &str,
    hint: Option<&str>,
    term_width: usize,
    term_height: usize,
    align: Align,
    explicit_oneline: bool,
    use_color: bool,
) -> Vec<String> {
    let icon = if active { "☕" } else { "○" };
    let icon_cols = if active { 2usize } else { 1usize };
    let icon_colored = if active {
        p.rim_inner.paint(icon, use_color)
    } else {
        p.meta.paint(icon, use_color)
    };

    if !explicit_oneline && term_height >= 3 {
        let mut lines = Vec::new();
        let center_line = |text_cols: usize, painted: String| -> String {
            let pad = match align {
                Align::Center => " ".repeat(term_width.saturating_sub(text_cols) / 2),
                Align::Left => String::new(),
            };
            format!("{pad}{painted}")
        };

        lines.push(center_line(icon_cols, icon_colored));

        let t_fit = truncate_chars(title, term_width);
        let t_cols = t_fit.chars().count();
        lines.push(center_line(
            t_cols,
            paint_gradient_text(&t_fit, p.ramp, use_color),
        ));

        if term_height >= 5 && !caption.is_empty() {
            let c_fit = truncate_chars(caption, term_width);
            let c_cols = c_fit.chars().count();
            lines.push(center_line(c_cols, p.meta.paint(&c_fit, use_color)));
        }
        if term_height >= 4 && !meta.is_empty() {
            let m_fit = truncate_chars(meta, term_width);
            let m_cols = m_fit.chars().count();
            lines.push(center_line(m_cols, p.meta.paint(&m_fit, use_color)));
        }
        if term_height >= 6
            && let Some(h) = hint
        {
            let h_fit = truncate_chars(h, term_width);
            let h_cols = h_fit.chars().count();
            lines.push(center_line(h_cols, p.meta.paint(&h_fit, use_color)));
        }
        return lines;
    }

    let budget = term_width.saturating_sub(icon_cols + 1);
    let (vis_len, body) = if budget <= 12 || meta.is_empty() {
        let short_title = truncate_chars(title, budget);
        let len = icon_cols + 1 + short_title.chars().count();
        let painted = paint_gradient_text(&short_title, p.ramp, use_color);
        (len, format!("{icon_colored} {painted}"))
    } else {
        let title_len = title.chars().count();
        if title_len + 3 >= budget {
            let short_title = truncate_chars(title, budget);
            let len = icon_cols + 1 + short_title.chars().count();
            let painted = paint_gradient_text(&short_title, p.ramp, use_color);
            (len, format!("{icon_colored} {painted}"))
        } else {
            let meta_budget = budget.saturating_sub(title_len + 3);
            let short_meta = truncate_chars(meta, meta_budget);
            let len = icon_cols + 1 + title_len + 3 + short_meta.chars().count();
            let painted_title = paint_gradient_text(title, p.ramp, use_color);
            let painted_meta = p.meta.paint(&format!(" · {short_meta}"), use_color);
            (len, format!("{icon_colored} {painted_title}{painted_meta}"))
        }
    };
    let pad = match align {
        Align::Center => " ".repeat(term_width.saturating_sub(vis_len) / 2),
        Align::Left => String::new(),
    };
    vec![format!("{pad}{body}")]
}

// =========================================================================
// LOD 1 — Compact (18-col miniature 3D tapered mug w/ coffee rim, steam & handle)
// =========================================================================
#[allow(clippy::too_many_arguments)]
fn render_compact(
    p: Palette,
    pattern: Pattern,
    show_steam: bool,
    steam_fade: f32,
    title: &str,
    caption: &str,
    meta: &str,
    hint: Option<&str>,
    term_width: usize,
    term_height: usize,
    align: Align,
    use_color: bool,
    frame: usize,
) -> Vec<String> {
    const CUP_W: usize = 12;
    const SAUCER_W: usize = 14;
    const BLOCK_W: usize = SAUCER_W + 2;
    const CUP_OFF: usize = 1;
    const MAX_W: usize = CUP_OFF + CUP_W + 2 + 3; // 18
    const TOTAL_H: usize = 5;

    let side_by_side = term_width >= 36 && term_height < 15;

    if side_by_side {
        let steam = if show_steam {
            build_steam_rows(p, 1, CUP_W, frame, steam_fade, use_color)
                .into_iter()
                .next()
                .unwrap_or_default()
        } else {
            String::new()
        };
        let coffee = render_coffee_wave(p, 0, 2, 8, 0, frame, use_color);
        let cup_rows: Vec<(String, usize)> = vec![
            (
                if steam.is_empty() {
                    String::new()
                } else {
                    format!("  {steam}")
                },
                if steam.is_empty() { 0 } else { CUP_W + 2 },
            ),
            (
                format!(
                    " {}{}{}{}{}",
                    p.cup.paint("╭", use_color),
                    p.rim_inner.paint("━╯", use_color),
                    coffee,
                    p.rim_inner.paint("╰━", use_color),
                    p.cup.paint("╮", use_color),
                ),
                15,
            ),
            (
                format!(
                    " {}{}{}{}{}{}",
                    p.cup.paint("│", use_color),
                    render_glaze_span(p, pattern, 1, 0, 2, CUP_W + 2, TOTAL_H, false, use_color),
                    p.rim_inner.paint("╰━━━━━━╯", use_color),
                    render_glaze_span(p, pattern, 11, 0, 2, CUP_W + 2, TOTAL_H, false, use_color),
                    p.cup.paint("│", use_color),
                    p.cup.paint("──╮", use_color),
                ),
                18,
            ),
            (
                format!(
                    " {}{}{}{}{}{}",
                    p.cup.paint("│", use_color),
                    render_glaze_span(
                        p,
                        pattern,
                        1,
                        1,
                        CUP_W,
                        CUP_W + 2,
                        TOTAL_H,
                        false,
                        use_color
                    ),
                    p.cup.paint("│", use_color),
                    " ",
                    render_glaze_span(p, pattern, 15, 1, 1, CUP_W + 5, TOTAL_H, true, use_color),
                    p.cup.paint("│", use_color),
                ),
                18,
            ),
            (
                format!(
                    " {}{}{}{}",
                    p.cup_dark.paint("╰╮", use_color),
                    render_glaze_span(
                        p,
                        pattern,
                        2,
                        2,
                        CUP_W - 2,
                        CUP_W + 2,
                        TOTAL_H,
                        false,
                        use_color
                    ),
                    p.cup_dark.paint("╭╯", use_color),
                    p.cup_dark.paint("──╯", use_color),
                ),
                18,
            ),
            (
                format!(
                    "  {}{}{}",
                    p.cup_dark.paint("╰", use_color),
                    p.cup_dark.paint(&"─".repeat(CUP_W - 2), use_color),
                    p.cup_dark.paint("╯", use_color),
                ),
                14,
            ),
            (
                format!(
                    "{}{}{}",
                    p.saucer_edge.paint("╘", use_color),
                    p.saucer.paint(&"═".repeat(SAUCER_W), use_color),
                    p.saucer_edge.paint("╛", use_color),
                ),
                16,
            ),
        ];

        let right_text_w = title
            .chars()
            .count()
            .max(caption.chars().count())
            .max(meta.chars().count());
        let total_w = (MAX_W + 3 + right_text_w).min(term_width);
        let pad = " ".repeat(left_pad_for(align, total_w, term_width));
        let text_budget = term_width.saturating_sub(pad.len() + MAX_W + 3);
        let t_fit = truncate_chars(title, text_budget);
        let c_fit = truncate_chars(caption, text_budget);
        let m_fit = truncate_chars(meta, text_budget);

        let mut lines = Vec::new();
        for (idx, (cup_line, vis_w)) in cup_rows.into_iter().enumerate() {
            if idx == 0 && cup_line.is_empty() {
                continue;
            }
            let suffix = match idx {
                2 => paint_gradient_text(&t_fit, p.ramp, use_color),
                3 if !c_fit.is_empty() => p.meta.paint(&c_fit, use_color),
                3 if !m_fit.is_empty() => p.meta.paint(&m_fit, use_color),
                4 if !c_fit.is_empty() && !m_fit.is_empty() => p.meta.paint(&m_fit, use_color),
                5 if hint.is_some() => {
                    let h_fit = truncate_chars(hint.unwrap_or_default(), text_budget);
                    p.meta.paint(&h_fit, use_color)
                }
                _ => String::new(),
            };
            if suffix.is_empty() {
                lines.push(format!("{pad}{cup_line}"));
            } else {
                let gap = " ".repeat(MAX_W.saturating_sub(vis_w) + 3);
                lines.push(format!("{pad}{cup_line}{gap}{suffix}"));
            }
        }
        return lines;
    }

    let pad = " ".repeat(left_pad_for(align, MAX_W, term_width));
    let mut lines = Vec::new();

    let steam_rows = if term_height >= 16 { 2 } else { 1 };
    if show_steam {
        for s in build_steam_rows(p, steam_rows, CUP_W, frame, steam_fade, use_color) {
            if s.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!("{pad}{}{s}", " ".repeat(CUP_OFF + 1)));
            }
        }
    }

    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(CUP_OFF + 3),
        p.cup.paint("╭", use_color),
        p.rim_inner.paint("━━━━━━", use_color),
        p.cup.paint("╮", use_color),
    ));
    let coffee_top = render_coffee_wave(p, -1, 2, 8, 1, frame, use_color);
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("╭", use_color),
        p.rim_inner.paint("━╯", use_color),
        coffee_top,
        p.rim_inner.paint("╰━", use_color),
        p.cup.paint("╮", use_color),
    ));

    let coffee_mid = render_coffee_wave(p, 0, 2, 8, 0, frame, use_color);
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("┝", use_color),
        p.rim_inner.paint("━╮", use_color),
        coffee_mid,
        p.rim_inner.paint("╭━", use_color),
        p.cup.paint("┥", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("│", use_color),
        render_glaze_span(p, pattern, 1, 0, 2, CUP_W + 2, TOTAL_H, false, use_color),
        p.rim_inner.paint("╰━━━━━━╯", use_color),
        render_glaze_span(p, pattern, 11, 0, 2, CUP_W + 2, TOTAL_H, false, use_color),
        p.cup.paint("│", use_color),
    ));

    lines.push(format!(
        "{pad}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("│", use_color),
        render_glaze_span(
            p,
            pattern,
            1,
            1,
            CUP_W,
            CUP_W + 2,
            TOTAL_H,
            false,
            use_color
        ),
        p.cup.paint("│", use_color),
        p.cup.paint("──╮", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("│", use_color),
        render_glaze_span(
            p,
            pattern,
            1,
            2,
            CUP_W,
            CUP_W + 2,
            TOTAL_H,
            false,
            use_color
        ),
        p.cup.paint("│", use_color),
        " ",
        render_glaze_span(p, pattern, 15, 2, 1, CUP_W + 5, TOTAL_H, true, use_color),
        p.cup.paint("│", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup_dark.paint("╰╮", use_color),
        render_glaze_span(
            p,
            pattern,
            2,
            3,
            CUP_W - 2,
            CUP_W + 2,
            TOTAL_H,
            false,
            use_color
        ),
        p.cup_dark.paint("╭╯", use_color),
        p.cup_dark.paint("──╯", use_color),
    ));

    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(CUP_OFF + 1),
        p.cup_dark.paint("╰", use_color),
        p.cup_dark.paint(&"─".repeat(CUP_W - 2), use_color),
        p.cup_dark.paint("╯", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}",
        p.saucer_edge.paint("╘", use_color),
        p.saucer.paint(&"═".repeat(SAUCER_W), use_color),
        p.saucer_edge.paint("╛", use_color),
    ));

    append_footer_lines(
        &mut lines, p, title, caption, meta, hint, BLOCK_W, &pad, term_width, align, use_color,
    );
    lines
}

// =========================================================================
// LOD 2 — Medium (31-col max cup+handle, 33 cols w/ 2-space margin)
// =========================================================================
#[allow(clippy::too_many_arguments)]
fn render_medium(
    p: Palette,
    pattern: Pattern,
    show_steam: bool,
    steam_fade: f32,
    title: &str,
    caption: &str,
    meta: &str,
    hint: Option<&str>,
    term_width: usize,
    align: Align,
    use_color: bool,
    frame: usize,
) -> Vec<String> {
    const CUP_W: usize = 22;
    const DIP_STEPS: usize = 3;
    const STAIR_W: usize = 3;
    const BODY_ROWS: usize = 6;
    const TAPER: usize = 3;
    const SAUCER_W: usize = 28;
    const BLOCK_W: usize = SAUCER_W + 2;
    const CUP_OFF: usize = (BLOCK_W - (CUP_W + 2)) / 2;
    const MAX_W: usize = CUP_OFF + CUP_W + 2 + 4;

    let total_glaze_h = DIP_STEPS + BODY_ROWS;
    let pad = " ".repeat(left_pad_for(align, MAX_W, term_width));
    let mut lines = Vec::new();

    if show_steam {
        for s in build_steam_rows(p, 3, CUP_W, frame, steam_fade, use_color) {
            if s.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!("{pad}{}{s}", " ".repeat(CUP_OFF + 1)));
            }
        }
    }

    let top_fill = (DIP_STEPS - 2) * STAIR_W;
    let top_w = CUP_W - top_fill * 2 - 2;
    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(CUP_OFF + top_fill + 1),
        p.cup.paint("╭", use_color),
        p.rim_inner.paint(&"━".repeat(top_w), use_color),
        p.cup.paint("╮", use_color),
    ));

    let gap_up = CUP_W - STAIR_W * 2 - 2;
    let coffee_up = render_coffee_wave(p, -1, DIP_STEPS, gap_up, 3, frame, use_color);
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("╭", use_color),
        p.rim_inner
            .paint(&format!("{}╯", "━".repeat(STAIR_W)), use_color),
        coffee_up,
        p.rim_inner
            .paint(&format!("╰{}", "━".repeat(STAIR_W)), use_color),
        p.cup.paint("╮", use_color),
    ));

    for i in 0..DIP_STEPS {
        let l_fill = i * STAIR_W;
        let row = if i == 0 {
            let gap_w = CUP_W - STAIR_W * 2 - 2;
            let gap = render_coffee_wave(p, 0, DIP_STEPS, gap_w, 1, frame, use_color);
            format!(
                "{}{}{}{}{}",
                p.cup.paint("┝", use_color),
                p.rim_inner
                    .paint(&format!("{}╮", "━".repeat(STAIR_W)), use_color),
                gap,
                p.rim_inner
                    .paint(&format!("╭{}", "━".repeat(STAIR_W)), use_color),
                p.cup.paint("┥", use_color),
            )
        } else if i < DIP_STEPS - 1 {
            let gap_w = CUP_W - l_fill * 2 - (STAIR_W + 1) * 2;
            let gap = render_coffee_wave(p, i as isize, DIP_STEPS, gap_w, 0, frame, use_color);
            let seg = "━".repeat(STAIR_W - 1);
            format!(
                "{}{}{}{}{}{}{}",
                p.cup.paint("│", use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.rim_inner.paint(&format!("╰{seg}╮"), use_color),
                gap,
                p.rim_inner.paint(&format!("╭{seg}╯"), use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1 + CUP_W - l_fill,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.cup.paint("│", use_color),
            )
        } else {
            let flat_w = CUP_W - l_fill * 2 - 2;
            format!(
                "{}{}{}{}{}",
                p.cup.paint("│", use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.rim_inner
                    .paint(&format!("╰{}╯", "━".repeat(flat_w)), use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1 + CUP_W - l_fill,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.cup.paint("│", use_color),
            )
        };
        lines.push(format!("{pad}{}{row}", " ".repeat(CUP_OFF)));
    }

    let width_at = |r: usize| -> usize {
        let shrink = (r * TAPER) / (BODY_ROWS - 1);
        CUP_W - shrink * 2
    };
    for row in 0..BODY_ROWS {
        let gy = DIP_STEPS + row;
        let w = width_at(row);
        let outer_w = if row % 2 == 1 { width_at(row - 1) } else { w };
        let pad_in = (CUP_W - outer_w) / 2;

        let handle_bridge = row == 1 || row == 4;
        let is_step = row > 0 && row % 2 == 1;
        let wall_l_w = if is_step { 2 } else { 1 };
        let (wall_l, wall_r, wall_extra) = if row == 0 {
            (
                p.cup.paint("│", use_color),
                p.cup.paint("│", use_color),
                0usize,
            )
        } else if is_step {
            (
                p.cup_dark.paint("╰╮", use_color),
                if handle_bridge {
                    String::new()
                } else {
                    p.cup_dark.paint("╭╯", use_color)
                },
                if handle_bridge { 0 } else { 2 },
            )
        } else {
            (
                p.cup_dark.paint("│", use_color),
                if handle_bridge {
                    String::new()
                } else {
                    p.cup_dark.paint("│", use_color)
                },
                0usize,
            )
        };

        let fill_w = if handle_bridge {
            if is_step { outer_w } else { outer_w + 1 }
        } else {
            outer_w.saturating_sub(wall_extra)
        };

        let inner_col = pad_in + wall_l_w;
        let inner = render_glaze_span(
            p,
            pattern,
            inner_col,
            gy,
            fill_w,
            CUP_W + 2,
            total_glaze_h,
            false,
            use_color,
        );

        let right_pad_col = inner_col + fill_w;
        let right_pad = if handle_bridge {
            render_glaze_span(
                p,
                pattern,
                right_pad_col,
                gy,
                pad_in,
                CUP_W + 5,
                total_glaze_h,
                true,
                use_color,
            )
        } else if row == 2 || row == 5 {
            p.cup_dark.paint(&"─".repeat(pad_in), use_color)
        } else {
            " ".repeat(pad_in)
        };

        let h_start = CUP_W + 2;
        let h_bar = render_glaze_span(
            p,
            pattern,
            h_start,
            gy,
            2,
            CUP_W + 5,
            total_glaze_h,
            true,
            use_color,
        );
        let v_bar = render_glaze_span(
            p,
            pattern,
            h_start + 2,
            gy,
            1,
            CUP_W + 5,
            total_glaze_h,
            true,
            use_color,
        );
        let handle_str = match row {
            0 => p.cup.paint("───╮", use_color),
            1 | 4 => format!("{h_bar}{v_bar}{}", p.cup.paint("│", use_color)),
            2 => format!(
                "{}{v_bar}{}",
                p.cup.paint("─╮", use_color),
                p.cup.paint("│", use_color)
            ),
            3 => format!(
                " {}{v_bar}{}",
                p.cup_dark.paint("│", use_color),
                p.cup_dark.paint("│", use_color)
            ),
            5 => p.cup_dark.paint("───╯", use_color),
            _ => String::new(),
        };

        lines.push(format!(
            "{pad}{}{wall_l}{inner}{wall_r}{right_pad}{handle_str}",
            " ".repeat(CUP_OFF + pad_in)
        ));
    }

    let bot_w = width_at(BODY_ROWS - 1);
    let bot_pad = CUP_OFF + (CUP_W - bot_w) / 2;
    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(bot_pad),
        p.cup_dark.paint("╰", use_color),
        p.cup_dark.paint(&"─".repeat(bot_w), use_color),
        p.cup_dark.paint("╯", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}",
        p.saucer_edge.paint("╘", use_color),
        p.saucer.paint(&"═".repeat(SAUCER_W), use_color),
        p.saucer_edge.paint("╛", use_color),
    ));

    append_footer_lines(
        &mut lines, p, title, caption, meta, hint, BLOCK_W, &pad, term_width, align, use_color,
    );
    lines
}

// =========================================================================
// LOD 3 — Full (48-col cup+handle, 50 cols w/ 2-space margin)
// =========================================================================
#[allow(clippy::too_many_arguments)]
fn render_full(
    p: Palette,
    pattern: Pattern,
    show_steam: bool,
    steam_fade: f32,
    title: &str,
    caption: &str,
    meta: &str,
    hint: Option<&str>,
    term_width: usize,
    term_height: usize,
    align: Align,
    use_color: bool,
    frame: usize,
) -> Vec<String> {
    const CUP_W: usize = 36;
    const DIP_STEPS: usize = 4;
    const STAIR_W: usize = 4;
    const BODY_ROWS: usize = 9;
    const TAPER: usize = 4;
    const SAUCER_W: usize = 44;
    const BLOCK_W: usize = SAUCER_W + 2;
    const CUP_OFF: usize = (BLOCK_W - (CUP_W + 2)) / 2;
    const MAX_W: usize = CUP_OFF + CUP_W + 2 + 6;

    let total_glaze_h = DIP_STEPS + BODY_ROWS;
    let pad = " ".repeat(left_pad_for(align, MAX_W, term_width));
    let mut lines = Vec::new();

    let steam_rows = if term_height >= 28 { 4 } else { 2 };
    if show_steam {
        for s in build_steam_rows(p, steam_rows, CUP_W, frame, steam_fade, use_color) {
            if s.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!("{pad}{}{s}", " ".repeat(CUP_OFF + 1)));
            }
        }
    }

    let top_fill = (DIP_STEPS - 2) * STAIR_W;
    let top_w = CUP_W - top_fill * 2 - 2;
    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(CUP_OFF + top_fill + 1),
        p.cup.paint("╭", use_color),
        p.rim_inner.paint(&"━".repeat(top_w), use_color),
        p.cup.paint("╮", use_color),
    ));

    let mid_fill = STAIR_W;
    let mid_gap = CUP_W - mid_fill * 2 - (STAIR_W + 1) * 2;
    let mid_seg = "━".repeat(STAIR_W - 1);
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF + mid_fill + 1),
        p.cup.paint("╭", use_color),
        p.rim_inner.paint(&format!("{mid_seg}╯"), use_color),
        " ".repeat(mid_gap),
        p.rim_inner.paint(&format!("╰{mid_seg}"), use_color),
        p.cup.paint("╮", use_color),
    ));

    let gap_up = CUP_W - STAIR_W * 2 - 2;
    let coffee_up = render_coffee_wave(p, -1, DIP_STEPS, gap_up, 6, frame, use_color);
    lines.push(format!(
        "{pad}{}{}{}{}{}{}",
        " ".repeat(CUP_OFF),
        p.cup.paint("╭", use_color),
        p.rim_inner
            .paint(&format!("{}╯", "━".repeat(STAIR_W)), use_color),
        coffee_up,
        p.rim_inner
            .paint(&format!("╰{}", "━".repeat(STAIR_W)), use_color),
        p.cup.paint("╮", use_color),
    ));

    for i in 0..DIP_STEPS {
        let l_fill = i * STAIR_W;
        let row = if i == 0 {
            let gap_w = CUP_W - STAIR_W * 2 - 2;
            let gap = render_coffee_wave(p, 0, DIP_STEPS, gap_w, 3, frame, use_color);
            format!(
                "{}{}{}{}{}",
                p.cup.paint("┝", use_color),
                p.rim_inner
                    .paint(&format!("{}╮", "━".repeat(STAIR_W)), use_color),
                gap,
                p.rim_inner
                    .paint(&format!("╭{}", "━".repeat(STAIR_W)), use_color),
                p.cup.paint("┥", use_color),
            )
        } else if i < DIP_STEPS - 1 {
            let gap_w = CUP_W - l_fill * 2 - (STAIR_W + 1) * 2;
            let gap = render_coffee_wave(p, i as isize, DIP_STEPS, gap_w, 0, frame, use_color);
            let seg = "━".repeat(STAIR_W - 1);
            format!(
                "{}{}{}{}{}{}{}",
                p.cup.paint("│", use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.rim_inner.paint(&format!("╰{seg}╮"), use_color),
                gap,
                p.rim_inner.paint(&format!("╭{seg}╯"), use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1 + CUP_W - l_fill,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.cup.paint("│", use_color),
            )
        } else {
            let flat_w = CUP_W - l_fill * 2 - 2;
            format!(
                "{}{}{}{}{}",
                p.cup.paint("│", use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.rim_inner
                    .paint(&format!("╰{}╯", "━".repeat(flat_w)), use_color),
                render_glaze_span(
                    p,
                    pattern,
                    1 + CUP_W - l_fill,
                    i,
                    l_fill,
                    CUP_W + 2,
                    total_glaze_h,
                    false,
                    use_color
                ),
                p.cup.paint("│", use_color),
            )
        };
        lines.push(format!("{pad}{}{row}", " ".repeat(CUP_OFF)));
    }

    let width_at = |r: usize| -> usize {
        let shrink = (r * TAPER) / (BODY_ROWS - 1);
        CUP_W - shrink * 2
    };

    for row in 0..BODY_ROWS {
        let gy = DIP_STEPS + row;
        let w = width_at(row);
        let outer_w = if row % 2 == 1 { width_at(row - 1) } else { w };
        let pad_in = (CUP_W - outer_w) / 2;

        let handle_bridge = row == 1 || row == 6;
        let is_step = row > 0 && row % 2 == 1;
        let wall_l_w = if is_step { 2 } else { 1 };
        let (wall_l, wall_r, wall_extra) = if row == 0 {
            (
                p.cup.paint("│", use_color),
                p.cup.paint("│", use_color),
                0usize,
            )
        } else if is_step {
            (
                p.cup_dark.paint("╰╮", use_color),
                if handle_bridge {
                    String::new()
                } else {
                    p.cup_dark.paint("╭╯", use_color)
                },
                if handle_bridge { 0 } else { 2 },
            )
        } else {
            (
                p.cup_dark.paint("│", use_color),
                if handle_bridge {
                    String::new()
                } else {
                    p.cup_dark.paint("│", use_color)
                },
                0usize,
            )
        };

        let fill_w = if handle_bridge {
            if is_step { outer_w } else { outer_w + 1 }
        } else {
            outer_w.saturating_sub(wall_extra)
        };

        let inner_col = pad_in + wall_l_w;
        let inner = render_glaze_span(
            p,
            pattern,
            inner_col,
            gy,
            fill_w,
            CUP_W + 2,
            total_glaze_h,
            false,
            use_color,
        );

        let right_pad_col = inner_col + fill_w;
        let right_pad = if row > 7 {
            String::new()
        } else if handle_bridge {
            render_glaze_span(
                p,
                pattern,
                right_pad_col,
                gy,
                pad_in,
                CUP_W + 7,
                total_glaze_h,
                true,
                use_color,
            )
        } else if row == 2 || row == 5 || row == 7 {
            p.cup_dark.paint(&"─".repeat(pad_in), use_color)
        } else {
            " ".repeat(pad_in)
        };

        let h_start = CUP_W + 2;
        let h_bar = render_glaze_span(
            p,
            pattern,
            h_start,
            gy,
            3,
            CUP_W + 7,
            total_glaze_h,
            true,
            use_color,
        );
        let v_bar = render_glaze_span(
            p,
            pattern,
            h_start + 3,
            gy,
            2,
            CUP_W + 7,
            total_glaze_h,
            true,
            use_color,
        );
        let handle_str = match row {
            0 => p.cup.paint("─────╮", use_color),
            1 | 6 => format!("{h_bar}{v_bar}{}", p.cup.paint("│", use_color)),
            2 => format!(
                "{}{v_bar}{}",
                p.cup.paint("──╮", use_color),
                p.cup.paint("│", use_color)
            ),
            3..=4 => format!(
                "  {}{v_bar}{}",
                p.cup_dark.paint("│", use_color),
                p.cup_dark.paint("│", use_color)
            ),
            5 => format!(
                "{}{v_bar}{}",
                p.cup_dark.paint("──╯", use_color),
                p.cup_dark.paint("│", use_color)
            ),
            7 => p.cup_dark.paint("─────╯", use_color),
            _ => String::new(),
        };

        lines.push(format!(
            "{pad}{}{wall_l}{inner}{wall_r}{right_pad}{handle_str}",
            " ".repeat(CUP_OFF + pad_in)
        ));
    }

    let bot_w = width_at(BODY_ROWS - 1);
    let bot_pad = CUP_OFF + (CUP_W - bot_w) / 2;
    lines.push(format!(
        "{pad}{}{}{}{}",
        " ".repeat(bot_pad),
        p.cup_dark.paint("╰", use_color),
        p.cup_dark.paint(&"─".repeat(bot_w), use_color),
        p.cup_dark.paint("╯", use_color),
    ));
    lines.push(format!(
        "{pad}{}{}{}",
        p.saucer_edge.paint("╘", use_color),
        p.saucer.paint(&"═".repeat(SAUCER_W), use_color),
        p.saucer_edge.paint("╛", use_color),
    ));

    append_footer_lines(
        &mut lines, p, title, caption, meta, hint, BLOCK_W, &pad, term_width, align, use_color,
    );
    lines
}

#[allow(clippy::too_many_arguments)]
fn append_footer_lines(
    lines: &mut Vec<String>,
    p: Palette,
    title: &str,
    caption: &str,
    meta: &str,
    hint: Option<&str>,
    block_w: usize,
    pad: &str,
    term_width: usize,
    align: Align,
    use_color: bool,
) {
    let text_width = match align {
        Align::Center => term_width,
        Align::Left => term_width.saturating_sub(pad.len()),
    };
    let push_centered = |out: &mut Vec<String>, text: &str, paint_fn: &dyn Fn(&str) -> String| {
        let fit = truncate_chars(text, text_width);
        let len = fit.chars().count();
        let line = match align {
            Align::Center => {
                let cup_center_pad = pad.len() + block_w.saturating_sub(len) / 2;
                let left = if len <= block_w && cup_center_pad + len <= term_width {
                    cup_center_pad
                } else {
                    term_width.saturating_sub(len) / 2
                };
                format!("{}{}", " ".repeat(left), paint_fn(&fit))
            }
            Align::Left => {
                let inner_pad = if len <= block_w {
                    block_w.saturating_sub(len) / 2
                } else {
                    0
                };
                format!("{pad}{}{}", " ".repeat(inner_pad), paint_fn(&fit))
            }
        };
        out.push(line);
    };

    lines.push(String::new());
    push_centered(lines, title, &|s| paint_gradient_text(s, p.ramp, use_color));
    if !caption.is_empty() {
        push_centered(lines, caption, &|s| p.meta.paint(s, use_color));
    }
    if !meta.is_empty() {
        push_centered(lines, meta, &|s| p.meta.paint(s, use_color));
    }
    if let Some(h) = hint {
        push_centered(lines, h, &|s| p.meta.paint(s, use_color));
    }
}

fn entropy_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x517c_c1b7_2722_0a95);
    let pid = u64::from(std::process::id());
    hash_u64(nanos ^ pid.rotate_left(17))
}

fn hash_u64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}
