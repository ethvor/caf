use std::env;
use std::fs;
use std::io::{IsTerminal, Write, stdout};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};
use serde::{Deserialize, Serialize};

use crate::art::{
    Pattern, RenderRequest, build_art, detect_terminal_size, enabled_patterns, enabled_profiles,
    pick_random_combo, render_banner,
};
use crate::config::{CafConfig, load_config, save_config};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafState {
    pub pid: u32,
    pub lid: bool,
    pub dark: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark_at_unix: Option<u64>,
    pub started_unix: u64,
    pub expires_unix: Option<u64>,
    pub reason: Option<String>,
    pub platform: String,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayOverrides<'a> {
    pub preset: Option<&'a str>,
    pub lod: Option<&'a str>,
    pub palette: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub width: Option<usize>,
    pub align: Option<&'a str>,
}

pub fn state_path() -> Result<PathBuf> {
    if let Ok(val) = env::var("CAF_STATE")
        && !val.trim().is_empty()
    {
        return Ok(PathBuf::from(val.trim()));
    }
    let home = dirs::home_dir().context("could not resolve home directory")?;
    Ok(home.join(".cache").join("caf").join("state.json"))
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_on(
    duration: Option<&str>,
    lid_flag: bool,
    no_lid: bool,
    dark: bool,
    dark_in: Option<&str>,
    reason: Option<&str>,
    force_watch: bool,
    no_animate: bool,
    overrides: DisplayOverrides<'_>,
    as_json: bool,
) -> Result<()> {
    let cfg = load_config()?;
    let lid = if no_lid {
        false
    } else if lid_flag {
        true
    } else {
        cfg.lid_by_default
    };

    let secs = match duration {
        Some(raw) => Some(parse_duration_secs(raw)?),
        None => None,
    };
    let dark_in_secs = match dark_in {
        Some(raw) => {
            if dark {
                bail!("cannot combine `--dark` (immediate) with `--dark-in` (delayed)");
            }
            let d = parse_duration_secs(raw)?;
            if d == 0 {
                bail!("--dark-in duration must be > 0 (use `--dark` for immediate dark mode)");
            }
            if let Some(total) = secs
                && d >= total
            {
                bail!(
                    "--dark-in ({raw}) must be shorter than total caffeinate duration ({})",
                    duration.unwrap_or_default()
                );
            }
            Some(d)
        }
        None => None,
    };

    if load_live_state()?.is_some() {
        let _ = stop_existing(true);
    }

    let pid = if cfg!(target_os = "macos") {
        start_macos(secs, lid, dark, dark_in_secs)?
    } else if cfg!(target_os = "linux") {
        start_linux(secs, lid, dark, dark_in_secs, reason)?
    } else if cfg!(target_os = "windows") {
        start_windows(secs, lid, dark, dark_in_secs)?
    } else {
        bail!("unsupported OS for caf");
    };

    let now = now_unix();
    let state = CafState {
        pid,
        lid,
        dark,
        dark_at_unix: dark_in_secs.map(|d| now + d),
        started_unix: now,
        expires_unix: secs.map(|s| now + s),
        reason: reason.map(str::to_string),
        platform: env::consts::OS.to_string(),
    };
    save_state(&state)?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    let should_animate = !no_animate
        && (force_watch || (cfg.animate && !dark))
        && stdout().is_terminal()
        && cfg.display != "off";

    if should_animate {
        let (locked_palette, locked_pattern, expired_while_watching) =
            run_animated_caf(&cfg, &state, &overrides)?;
        let pat_name = locked_pattern.name();
        if expired_while_watching {
            run_animated_decaf(&cfg, Some(&locked_palette), Some(pat_name), &overrides)?;
            let _ = render_banner(
                &cfg,
                &RenderRequest {
                    active: false,
                    lid: false,
                    dark: false,
                    dark_in: None,
                    timer: None,
                    reason: Some("timer finished · sleep restored"),
                    hint: None,
                    is_shell: false,
                    preset_override: overrides.preset,
                    lod_override: overrides.lod,
                    palette_override: Some(&locked_palette),
                    pattern_override: Some(pat_name),
                    width_override: overrides.width,
                    height_override: None,
                    align_override: overrides.align,
                    frame: 0,
                    cool_blend: None,
                    steam_fade: 1.0,
                },
            )?;
        } else {
            let now2 = now_unix();
            let rem = state
                .expires_unix
                .map(|e| format_remaining(e.saturating_sub(now2)));
            let (is_dark_now, dark_in_str) = dark_display_state(&state, now2);
            let _ = render_banner(
                &cfg,
                &RenderRequest {
                    active: true,
                    lid,
                    dark: is_dark_now,
                    dark_in: dark_in_str.as_deref(),
                    timer: rem.as_deref().or(duration),
                    reason,
                    hint: None,
                    is_shell: false,
                    preset_override: overrides.preset,
                    lod_override: overrides.lod,
                    palette_override: Some(&locked_palette),
                    pattern_override: Some(pat_name),
                    width_override: overrides.width,
                    height_override: None,
                    align_override: overrides.align,
                    frame: 0,
                    cool_blend: None,
                    steam_fade: 0.0,
                },
            )?;
        }
    } else {
        let _ = render_banner(
            &cfg,
            &RenderRequest {
                active: true,
                lid,
                dark,
                dark_in,
                timer: duration,
                reason,
                hint: None,
                is_shell: false,
                preset_override: overrides.preset,
                lod_override: overrides.lod,
                palette_override: overrides.palette,
                pattern_override: overrides.pattern,
                width_override: overrides.width,
                height_override: None,
                align_override: overrides.align,
                frame: 0,
                cool_blend: None,
                steam_fade: 0.0,
            },
        )?;
    }
    Ok(())
}

pub fn cmd_off(no_animate: bool, overrides: DisplayOverrides<'_>, as_json: bool) -> Result<()> {
    let stopped = stop_existing(false)?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "active": false,
                "stopped_previous": stopped,
            }))?
        );
        return Ok(());
    }
    let cfg = load_config()?;
    if stopped {
        let should_animate =
            !no_animate && cfg.animate && stdout().is_terminal() && cfg.display != "off";
        let (pal, pat) = if should_animate {
            let (p, pt) = run_animated_decaf(&cfg, None, None, &overrides)?;
            (Some(p), Some(pt))
        } else {
            (None, None)
        };
        let _ = render_banner(
            &cfg,
            &RenderRequest {
                active: false,
                lid: false,
                dark: false,
                dark_in: None,
                timer: None,
                reason: Some("sleep restored"),
                hint: None,
                is_shell: false,
                preset_override: overrides.preset,
                lod_override: overrides.lod,
                palette_override: pal.as_deref().or(overrides.palette),
                pattern_override: pat.as_deref().or(overrides.pattern),
                width_override: overrides.width,
                height_override: None,
                align_override: overrides.align,
                frame: 0,
                cool_blend: None,
                steam_fade: 1.0,
            },
        )?;
    } else {
        println!("caf is already off (sleep normal).");
    }
    Ok(())
}

/// Plain operational status (no art by default; use `caf logo` for art).
pub fn cmd_status(as_json: bool) -> Result<()> {
    let live = load_live_state()?;
    let now = now_unix();
    if as_json {
        let val = match &live {
            Some(st) => serde_json::json!({
                "active": true,
                "state": st,
                "remaining_secs": st.expires_unix.map(|e| e.saturating_sub(now)),
                "dark_in_secs": st.dark_at_unix.map(|d| d.saturating_sub(now)),
            }),
            None => serde_json::json!({ "active": false }),
        };
        println!("{}", serde_json::to_string_pretty(&val)?);
        return Ok(());
    }

    match live {
        Some(st) => {
            let (is_dark_now, dark_in_str) = dark_display_state(&st, now);
            let mut flags = Vec::new();
            flags.push(if st.lid {
                "lid-close safe"
            } else {
                "lid-open only"
            });
            if is_dark_now {
                flags.push("dark");
            }
            println!("caf: AWAKE ({})", flags.join(", "));
            if let Some(exp) = st.expires_unix {
                println!(
                    "  remaining: {}",
                    format_remaining_precise(exp.saturating_sub(now))
                );
            } else {
                println!("  remaining: indefinite");
            }
            if let Some(d) = dark_in_str {
                println!("  dark-in:   {d}");
            }
            if let Some(r) = &st.reason {
                println!("  reason:    {r}");
            }
            println!("  pid:       {}", st.pid);
        }
        None => {
            println!("caf: OFF (normal sleep behavior)");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_logo(
    if_active: bool,
    is_shell: bool,
    animate: bool,
    gallery: Option<&str>,
    list_only: bool,
    overrides: DisplayOverrides<'_>,
) -> Result<()> {
    if list_only || gallery.is_some() {
        let cfg = load_config()?;
        return crate::art::cmd_logo_gallery(
            &cfg,
            gallery,
            list_only,
            overrides.palette,
            overrides.pattern,
            overrides.preset,
        );
    }

    if is_shell && !stdout().is_terminal() {
        return Ok(());
    }
    if if_active && !state_path()?.is_file() {
        return Ok(());
    }

    let cfg = load_config()?;
    if is_shell && overrides.lod.is_none() && cfg.shell_reminder.eq_ignore_ascii_case("off") {
        return Ok(());
    }

    let live = load_live_state()?;
    if if_active && live.is_none() {
        return Ok(());
    }

    if animate && stdout().is_terminal() {
        let st = live.unwrap_or(CafState {
            pid: 0,
            lid: cfg.lid_by_default,
            dark: false,
            dark_at_unix: None,
            started_unix: now_unix(),
            expires_unix: None,
            reason: None,
            platform: env::consts::OS.to_string(),
        });
        let _ = run_animated_caf(&cfg, &st, &overrides)?;
        return Ok(());
    }

    let now = now_unix();
    let (active, lid, dark, dark_in_str, rem, reason) = match &live {
        Some(st) => {
            let (is_dark, d_in) = dark_display_state(st, now);
            (
                true,
                st.lid,
                is_dark,
                d_in,
                st.expires_unix
                    .map(|e| format_remaining(e.saturating_sub(now))),
                st.reason.as_deref(),
            )
        }
        None => (false, false, false, None, None, None),
    };

    let _ = render_banner(
        &cfg,
        &RenderRequest {
            active,
            lid,
            dark,
            dark_in: dark_in_str.as_deref(),
            timer: rem.as_deref(),
            reason,
            hint: None,
            is_shell,
            preset_override: overrides.preset,
            lod_override: overrides.lod,
            palette_override: overrides.palette,
            pattern_override: overrides.pattern,
            width_override: overrides.width,
            height_override: None,
            align_override: overrides.align,
            frame: 0,
            cool_blend: None,
            steam_fade: if active { 0.0 } else { 1.0 },
        },
    )?;
    Ok(())
}

/// Installs a narrowly scoped, `visudo -c` validated macOS sudoers rule permitting ONLY
/// `/usr/bin/pmset -a disablesleep 0` and `/usr/bin/pmset -a disablesleep 1` without a password.
/// This makes default lid-closed `caf` instant and guarantees timed `--lid` timers restore sleep cleanly.
pub fn cmd_authorize_lid(dry_run: bool) -> Result<()> {
    if !cfg!(target_os = "macos") {
        println!(
            "`caf authorize-lid` is only needed on macOS (Linux systemd-inhibit and Windows powercfg do not require sudoers)."
        );
        return Ok(());
    }

    let user = env::var("USER")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .context("could not determine current macOS username from $USER")?;
    if !user
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("unsafe username `{user}`");
    }

    let uid_out = Command::new("/usr/bin/id")
        .args(["-u", &user])
        .output()
        .context("failed running /usr/bin/id")?;
    if !uid_out.status.success() {
        bail!("failed resolving UID for `{user}`");
    }
    let uid = String::from_utf8_lossy(&uid_out.stdout).trim().to_string();
    let target_file = format!("/etc/sudoers.d/caf-{uid}");
    let rule = format!(
        "# caf: permit only lid-closed sleep gate toggling (disablesleep 0/1).\n\
         {user} ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a disablesleep 1\n"
    );

    if dry_run {
        println!("Will install {target_file} (mode 0440, validated with visudo -c):\n");
        print!("{rule}");
        return Ok(());
    }

    let tmp_path = env::temp_dir().join(format!("caf-sudoers-{uid}.tmp"));
    fs::write(&tmp_path, &rule)?;
    let script = format!(
        "install -d -m 0755 /etc/sudoers.d && \
         install -m 0440 -o root -g wheel '{src}' '{dst}.new' && \
         /usr/sbin/visudo -c -f '{dst}.new' && \
         mv -f '{dst}.new' '{dst}'",
        src = tmp_path.display(),
        dst = target_file
    );
    let status = Command::new("sudo")
        .args(["sh", "-c", &script])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed running sudo to install sudoers rule")?;
    let _ = fs::remove_file(&tmp_path);
    if !status.success() {
        bail!("failed installing {target_file} (visudo validation or sudo aborted)");
    }
    println!("Installed {target_file} (validated with visudo).");
    println!("  `caf` lid-closed mode and timed sleep restoration now work passwordlessly.");
    Ok(())
}

fn dark_display_state(state: &CafState, now: u64) -> (bool, Option<String>) {
    if state.dark {
        return (true, None);
    }
    match state.dark_at_unix {
        Some(t) if now >= t => (true, None),
        Some(t) => (false, Some(format_remaining(t.saturating_sub(now)))),
        None => (false, None),
    }
}

/// RAII terminal guard for alternate-screen raw-mode animations; always restores cursor & screen.
struct AnimTerminalGuard;

impl AnimTerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed enabling terminal raw mode")?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for AnimTerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut out = stdout();
        let _ = execute!(out, cursor::Show, LeaveAlternateScreen);
    }
}

/// Live animated CAF screen:
/// - Drifting steam, shimmering coffee waves, ticking countdown
/// - Dynamic real-time LOD + re-centering on `Event::Resize`
/// - Arrow keys: Left/Right cycle palettes, Up cycles glaze patterns, Down rolls random combo
/// - Transient startup hint disappears after ~3s; arrow change toast fades after ~2s
fn run_animated_caf(
    cfg: &CafConfig,
    state: &CafState,
    overrides: &DisplayOverrides<'_>,
) -> Result<(String, Pattern, bool)> {
    let initial = build_art(
        cfg,
        &RenderRequest {
            active: true,
            lid: state.lid,
            dark: state.dark,
            dark_in: None,
            timer: None,
            reason: state.reason.as_deref(),
            hint: None,
            is_shell: false,
            preset_override: overrides.preset,
            lod_override: overrides.lod,
            palette_override: overrides.palette,
            pattern_override: overrides.pattern,
            width_override: overrides.width,
            height_override: None,
            align_override: Some("center"),
            frame: 0,
            cool_blend: None,
            steam_fade: 0.0,
        },
    )?;
    let Some(first) = initial else {
        return Ok(("cyan-pink".to_string(), Pattern::Gradient, false));
    };

    let mut live_cfg = cfg.clone();
    let mut profiles = enabled_profiles(&live_cfg);
    let patterns = enabled_patterns(&live_cfg);

    let mut cur_palette = first.palette_name;
    let mut cur_pattern = first.pattern;

    let _guard = AnimTerminalGuard::enter()?;
    let (mut cols, mut rows) = detect_terminal_size();
    let frame_step = Duration::from_millis(125);
    let mut frame: usize = 0;

    // Transient hint fades after 24 frames (~3s); key toast fades after 16-18 frames (~2s)
    const INITIAL_HINT_FRAMES: usize = 24;
    let mut toast: Option<(String, usize)> = None;

    loop {
        let now = now_unix();
        if let Some(exp) = state.expires_unix
            && now >= exp
        {
            return Ok((cur_palette, cur_pattern, true));
        }
        if state.pid != 0 && frame.is_multiple_of(8) && !pid_alive(state.pid) {
            return Ok((cur_palette, cur_pattern, true));
        }

        let rem = state.expires_unix.map(|e| {
            format!(
                "decaf in {}",
                format_remaining_precise(e.saturating_sub(now))
            )
        });
        let (is_dark_now, dark_in_str) = dark_display_state(state, now);
        let eff_w = overrides.width.unwrap_or(cols);

        let active_hint: Option<String> = if let Some((ref msg, until)) = toast {
            if frame < until {
                Some(msg.clone())
            } else {
                toast = None;
                None
            }
        } else if frame < INITIAL_HINT_FRAMES && rows >= 8 {
            if eff_w >= 54 {
                Some("←/→ palette · ↑/↓ pattern · ? rand · s save · q exit".to_string())
            } else if eff_w >= 30 {
                Some("←/→/↑/↓ style · ? · s/r/x · q".to_string())
            } else {
                Some("q to exit".to_string())
            }
        } else {
            None
        };

        let rendered = build_art(
            &live_cfg,
            &RenderRequest {
                active: true,
                lid: state.lid,
                dark: is_dark_now,
                dark_in: dark_in_str.as_deref(),
                timer: rem.as_deref(),
                reason: state.reason.as_deref(),
                hint: active_hint.as_deref(),
                is_shell: false,
                preset_override: overrides.preset,
                lod_override: overrides.lod,
                palette_override: Some(&cur_palette),
                pattern_override: Some(cur_pattern.name()),
                width_override: Some(eff_w),
                height_override: Some(rows),
                align_override: overrides.align.or(Some("center")),
                frame,
                cool_blend: Some(0.0),
                steam_fade: 0.0,
            },
        )?;

        if let Some(art) = rendered {
            draw_centered_frame(&art.lines, rows)?;
        }

        frame = frame.wrapping_add(1);
        let deadline = Instant::now() + frame_step;
        while let Some(wait) = deadline.checked_duration_since(Instant::now()) {
            if !event::poll(wait)? {
                break;
            }
            match event::read()? {
                Event::Resize(new_cols, new_rows) => {
                    cols = usize::from(new_cols.max(1));
                    rows = usize::from(new_rows.max(1));
                    break;
                }
                Event::Key(key) if is_exit_key(key.code, key.modifiers) => {
                    return Ok((cur_palette, cur_pattern, false));
                }
                Event::Key(key) if !key.modifiers.contains(KeyModifiers::CONTROL) => match key.code
                {
                    KeyCode::Right => {
                        if !profiles.is_empty() {
                            let idx = profiles
                                .iter()
                                .position(|p| p.name == cur_palette)
                                .unwrap_or(0);
                            cur_palette = profiles[(idx + 1) % profiles.len()].name.clone();
                            toast = Some((
                                format!("{cur_palette} · {}", cur_pattern.name()),
                                frame + 16,
                            ));
                            break;
                        }
                    }
                    KeyCode::Left => {
                        if !profiles.is_empty() {
                            let idx = profiles
                                .iter()
                                .position(|p| p.name == cur_palette)
                                .unwrap_or(0);
                            let prev = (idx + profiles.len() - 1) % profiles.len();
                            cur_palette = profiles[prev].name.clone();
                            toast = Some((
                                format!("{cur_palette} · {}", cur_pattern.name()),
                                frame + 16,
                            ));
                            break;
                        }
                    }
                    KeyCode::Up => {
                        if !patterns.is_empty() {
                            let idx = patterns.iter().position(|&p| p == cur_pattern).unwrap_or(0);
                            cur_pattern = patterns[(idx + 1) % patterns.len()];
                            toast = Some((
                                format!("{cur_palette} · {}", cur_pattern.name()),
                                frame + 16,
                            ));
                            break;
                        }
                    }
                    KeyCode::Down => {
                        if !patterns.is_empty() {
                            let idx = patterns.iter().position(|&p| p == cur_pattern).unwrap_or(0);
                            let prev = (idx + patterns.len() - 1) % patterns.len();
                            cur_pattern = patterns[prev];
                            toast = Some((
                                format!("{cur_palette} · {}", cur_pattern.name()),
                                frame + 16,
                            ));
                            break;
                        }
                    }
                    KeyCode::Char('/') | KeyCode::Char('?') => {
                        let (next_pal, next_pat) = pick_random_combo(&live_cfg);
                        cur_palette = next_pal;
                        cur_pattern = next_pat;
                        toast = Some((
                            format!("{cur_palette} · {}", cur_pattern.name()),
                            frame + 16,
                        ));
                        break;
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        live_cfg.palette = cur_palette.clone();
                        live_cfg.pattern = cur_pattern.name().to_string();
                        live_cfg.preset = None;
                        let msg = match save_config(&live_cfg) {
                            Ok(_) => {
                                format!("saved default: {cur_palette} · {}", cur_pattern.name())
                            }
                            Err(err) => format!("save failed: {err}"),
                        };
                        toast = Some((msg, frame + 18));
                        break;
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        live_cfg.palette = "random".to_string();
                        live_cfg.pattern = "random".to_string();
                        live_cfg.preset = None;
                        let msg = match save_config(&live_cfg) {
                            Ok(_) => "default reset to random".to_string(),
                            Err(err) => format!("reset failed: {err}"),
                        };
                        toast = Some((msg, frame + 18));
                        break;
                    }
                    KeyCode::Char('x') | KeyCode::Char('X') => {
                        if profiles.len() <= 1 {
                            toast = Some(("cannot hide last palette".to_string(), frame + 18));
                            break;
                        }
                        let idx = profiles
                            .iter()
                            .position(|p| p.name == cur_palette)
                            .unwrap_or(0);
                        let hidden = cur_palette.clone();
                        let hidden_lower = hidden.to_ascii_lowercase();
                        if !live_cfg
                            .disabled_palettes
                            .iter()
                            .any(|d| d.eq_ignore_ascii_case(&hidden_lower))
                        {
                            live_cfg.disabled_palettes.push(hidden_lower);
                            live_cfg.disabled_palettes.sort();
                        }
                        if live_cfg.palette.eq_ignore_ascii_case(&hidden) {
                            live_cfg.palette = "random".to_string();
                        }
                        let save_res = save_config(&live_cfg);
                        profiles = enabled_profiles(&live_cfg);
                        if !profiles.is_empty() {
                            cur_palette = profiles[idx % profiles.len()].name.clone();
                        }
                        let msg = match save_res {
                            Ok(_) => format!("hid {hidden} → {cur_palette}"),
                            Err(err) => format!("hide failed: {err}"),
                        };
                        toast = Some((msg, frame + 18));
                        break;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

/// Iconic 2-phase DECAF cool-down animation: steam dissipates, then warm gold blends to cool slate.
fn run_animated_decaf(
    cfg: &CafConfig,
    palette_lock: Option<&str>,
    pattern_lock: Option<&str>,
    overrides: &DisplayOverrides<'_>,
) -> Result<(String, String)> {
    let initial = build_art(
        cfg,
        &RenderRequest {
            active: false,
            lid: false,
            dark: false,
            dark_in: None,
            timer: None,
            reason: None,
            hint: None,
            is_shell: false,
            preset_override: overrides.preset,
            lod_override: overrides.lod,
            palette_override: palette_lock.or(overrides.palette),
            pattern_override: pattern_lock.or(overrides.pattern),
            width_override: overrides.width,
            height_override: None,
            align_override: Some("center"),
            frame: 0,
            cool_blend: Some(0.0),
            steam_fade: 0.0,
        },
    )?;
    let Some(first) = initial else {
        return Ok(("cyan-pink".to_string(), "gradient".to_string()));
    };
    let locked_palette = first.palette_name;
    let locked_pattern = first.pattern.name().to_string();

    let _guard = AnimTerminalGuard::enter()?;
    let (mut cols, mut rows) = detect_terminal_size();
    let frame_step = Duration::from_millis(110);

    let total_frames: usize = 16;
    for f in 0..total_frames {
        let (steam_fade, cool_blend) = if f < 9 {
            (f as f32 / 8.0, 0.0)
        } else {
            (1.0, (f - 8) as f32 / 7.0)
        };
        let eff_w = overrides.width.unwrap_or(cols);
        let rendered = build_art(
            cfg,
            &RenderRequest {
                active: false,
                lid: false,
                dark: false,
                dark_in: None,
                timer: None,
                reason: Some("sleep restored"),
                hint: None,
                is_shell: false,
                preset_override: overrides.preset,
                lod_override: overrides.lod,
                palette_override: Some(&locked_palette),
                pattern_override: Some(&locked_pattern),
                width_override: Some(eff_w),
                height_override: Some(rows),
                align_override: overrides.align.or(Some("center")),
                frame: f,
                cool_blend: Some(cool_blend),
                steam_fade,
            },
        )?;
        if let Some(art) = rendered {
            draw_centered_frame(&art.lines, rows)?;
        }

        let deadline = Instant::now() + frame_step;
        while let Some(wait) = deadline.checked_duration_since(Instant::now()) {
            if !event::poll(wait)? {
                break;
            }
            match event::read()? {
                Event::Resize(new_cols, new_rows) => {
                    cols = usize::from(new_cols.max(1));
                    rows = usize::from(new_rows.max(1));
                }
                Event::Key(key) if is_exit_key(key.code, key.modifiers) => {
                    return Ok((locked_palette, locked_pattern));
                }
                _ => {}
            }
        }
    }

    Ok((locked_palette, locked_pattern))
}

fn draw_centered_frame(lines: &[String], rows: usize) -> Result<()> {
    let mut out = stdout().lock();
    let top_pad = rows.saturating_sub(lines.len()) / 2;
    write!(out, "\x1b[H\x1b[2J")?;
    for _ in 0..top_pad {
        write!(out, "\r\n")?;
    }
    for (i, line) in lines.iter().enumerate() {
        if i + top_pad >= rows {
            break;
        }
        write!(out, "{line}\r\n")?;
    }
    out.flush()?;
    Ok(())
}

fn is_exit_key(code: KeyCode, mods: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc | KeyCode::Enter
    ) || (mods.contains(KeyModifiers::CONTROL)
        && matches!(code, KeyCode::Char('c') | KeyCode::Char('d')))
}

fn enable_macos_lid_gate() -> Result<()> {
    // Fast passwordless path when `caf authorize-lid` (or existing pmset sudoers) is installed
    let fast = Command::new("sudo")
        .args(["-n", "/usr/bin/pmset", "-a", "disablesleep", "1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if fast.is_ok_and(|s| s.success()) {
        return Ok(());
    }

    let status = Command::new("sudo")
        .args(["/usr/bin/pmset", "-a", "disablesleep", "1"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed running `sudo /usr/bin/pmset -a disablesleep 1`")?;
    if !status.success() {
        bail!(
            "failed enabling macOS lid-closed mode via pmset.\n  Run `caf authorize-lid` once for passwordless lid-closed mode, or pass `--no-lid`."
        );
    }
    Ok(())
}

fn start_macos(secs: Option<u64>, lid: bool, dark: bool, dark_in_secs: Option<u64>) -> Result<u32> {
    if lid {
        enable_macos_lid_gate()?;
    }

    // Delayed dark mode (`--dark-in D` [with optional total duration `secs`]):
    // Phase 1 keeps display+system awake for D, then triggers display sleep and holds `-ims`.
    if let Some(d) = dark_in_secs {
        let phase2 = match secs {
            Some(total) => format!("caffeinate -ims -t {}", total.saturating_sub(d)),
            None => "caffeinate -ims".to_string(),
        };
        let restore = if lid && secs.is_some() {
            "; sudo -n /usr/bin/pmset -a disablesleep 0 >/dev/null 2>&1 || true"
        } else {
            ""
        };
        let script = format!(
            "CPID=''; trap 'kill $CPID 2>/dev/null || true; exit 0' TERM INT; \
             caffeinate -dims -t {d} & CPID=$!; wait $CPID || exit 0; \
             /usr/bin/pmset displaysleepnow >/dev/null 2>&1 || true; \
             {phase2} & CPID=$!; wait $CPID{restore}"
        );
        let child = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed spawning macOS delayed-dark caf watcher")?;
        return Ok(child.id());
    }

    let flags = if dark { "-ims" } else { "-dims" };

    // On immediate `--dark`, trigger macOS display sleep after a 1s beat so releasing
    // the Enter key does not wake the display back up immediately.
    let spawn_immediate_dark = || {
        if dark {
            let _ = Command::new("sh")
                .args([
                    "-c",
                    "sleep 1; /usr/bin/pmset displaysleepnow >/dev/null 2>&1 || true",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    };

    if lid && let Some(s) = secs {
        let script = format!(
            "CPID=''; trap 'kill $CPID 2>/dev/null || true; exit 0' TERM INT; \
             caffeinate {flags} -t {s} & CPID=$!; wait $CPID; \
             sudo -n /usr/bin/pmset -a disablesleep 0 >/dev/null 2>&1 || true"
        );
        let child = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed spawning timed macOS caf watcher")?;
        spawn_immediate_dark();
        return Ok(child.id());
    }

    let mut cmd = Command::new("caffeinate");
    cmd.arg(flags);
    if let Some(s) = secs {
        cmd.args(["-t", &s.to_string()]);
    }
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed spawning macOS caffeinate")?;
    spawn_immediate_dark();
    Ok(child.id())
}

fn start_linux(
    secs: Option<u64>,
    lid: bool,
    dark: bool,
    dark_in_secs: Option<u64>,
    reason: Option<&str>,
) -> Result<u32> {
    let why = reason.unwrap_or("caf keep-awake");
    let full_what = if lid {
        "sleep:idle:handle-lid-switch"
    } else {
        "sleep:idle"
    };
    let dark_what = if lid {
        "sleep:handle-lid-switch"
    } else {
        "sleep"
    };

    if let Some(d) = dark_in_secs {
        let phase2_sleep = match secs {
            Some(total) => format!("sleep {}", total.saturating_sub(d)),
            None => "sleep infinity".to_string(),
        };
        let script = format!(
            "CPID=''; trap 'kill $CPID 2>/dev/null || true; exit 0' TERM INT; \
             systemd-inhibit --what={full_what} --who=caf --why=\"$1\" --mode=block sleep {d} & CPID=$!; wait $CPID || exit 0; \
             systemd-inhibit --what={dark_what} --who=caf --why=\"$1\" --mode=block {phase2_sleep} & CPID=$!; wait $CPID"
        );
        let child = Command::new("sh")
            .args(["-c", &script, "_", why])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed spawning Linux delayed-dark caf watcher")?;
        return Ok(child.id());
    }

    let what = if dark { dark_what } else { full_what };
    let hold_cmd = match secs {
        Some(s) => format!("sleep {s}"),
        None => "sleep infinity".to_string(),
    };
    let child = Command::new("systemd-inhibit")
        .args([
            &format!("--what={what}"),
            "--who=caf",
            &format!("--why={why}"),
            "--mode=block",
            "sh",
            "-c",
            &hold_cmd,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed spawning systemd-inhibit")?;
    Ok(child.id())
}

fn start_windows(
    secs: Option<u64>,
    lid: bool,
    dark: bool,
    dark_in_secs: Option<u64>,
) -> Result<u32> {
    if lid {
        let _ = Command::new("powercfg")
            .args([
                "/setacvalueindex",
                "SCHEME_CURRENT",
                "SUB_BUTTONS",
                "LIDACTION",
                "0",
            ])
            .status();
        let _ = Command::new("powercfg")
            .args(["/setactive", "SCHEME_CURRENT"])
            .status();
    }
    let restore = if lid {
        "powercfg /setacvalueindex SCHEME_CURRENT SUB_BUTTONS LIDACTION 1; powercfg /setactive SCHEME_CURRENT"
    } else {
        ""
    };
    let body = if let Some(d) = dark_in_secs {
        let phase2 = match secs {
            Some(total) => format!("Start-Sleep -Seconds {}", total.saturating_sub(d)),
            None => "while ($true) { Start-Sleep -Seconds 3600 }".to_string(),
        };
        format!(
            "[W.K]::SetThreadExecutionState(0x80000003) | Out-Null; Start-Sleep -Seconds {d}; \
             [W.K]::SetThreadExecutionState(0x80000001) | Out-Null; {phase2}"
        )
    } else {
        let flags = if dark { "0x80000001" } else { "0x80000003" };
        let sleep_stmt = match secs {
            Some(s) => format!("Start-Sleep -Seconds {s}"),
            None => "while ($true) { Start-Sleep -Seconds 3600 }".to_string(),
        };
        format!("[W.K]::SetThreadExecutionState({flags}) | Out-Null; {sleep_stmt}")
    };
    let ps = format!(
        "Add-Type -Name K -Namespace W -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern uint SetThreadExecutionState(uint esFlags);'; try {{ {body} }} finally {{ [W.K]::SetThreadExecutionState(0x80000000) | Out-Null; {restore} }}"
    );
    let child = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed spawning Windows keep-awake process")?;
    Ok(child.id())
}

fn stop_existing(quiet_restore: bool) -> Result<bool> {
    let path = state_path()?;
    if !path.is_file() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&path)?;
    let state: CafState = serde_json::from_str(&raw)?;

    let was_alive = pid_alive(state.pid);
    if was_alive {
        kill_pid(state.pid);
    }

    if state.lid && cfg!(target_os = "macos") {
        let fast = Command::new("sudo")
            .args(["-n", "/usr/bin/pmset", "-a", "disablesleep", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !fast.is_ok_and(|s| s.success()) && !quiet_restore {
            let status = Command::new("sudo")
                .args(["/usr/bin/pmset", "-a", "disablesleep", "0"])
                .status();
            if let Ok(s) = status
                && !s.success()
            {
                bail!("failed restoring `sudo /usr/bin/pmset -a disablesleep 0`; run it manually");
            }
        }
    } else if state.lid && cfg!(target_os = "windows") {
        let _ = Command::new("powercfg")
            .args([
                "/setacvalueindex",
                "SCHEME_CURRENT",
                "SUB_BUTTONS",
                "LIDACTION",
                "1",
            ])
            .status();
        let _ = Command::new("powercfg")
            .args(["/setactive", "SCHEME_CURRENT"])
            .status();
    }

    let _ = fs::remove_file(&path);
    Ok(was_alive || state.lid)
}

fn load_live_state() -> Result<Option<CafState>> {
    let path = state_path()?;
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    let state: CafState = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(_) => {
            let _ = fs::remove_file(&path);
            return Ok(None);
        }
    };
    if !pid_alive(state.pid) {
        if state.lid && cfg!(target_os = "macos") {
            let _ = Command::new("sudo")
                .args(["-n", "/usr/bin/pmset", "-a", "disablesleep", "0"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = fs::remove_file(&path);
        return Ok(None);
    }
    Ok(Some(state))
}

fn save_state(state: &CafState) -> Result<()> {
    let path = state_path()?;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn pid_alive(pid: u32) -> bool {
    if cfg!(target_os = "windows") {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        return out
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false);
    }
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn kill_pid(pid: u32) {
    if cfg!(target_os = "windows") {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    } else {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn parse_duration_secs(raw: &str) -> Result<u64> {
    let s = raw.trim().to_ascii_lowercase();
    if let Some(n) = s.strip_suffix('h') {
        return Ok(n.parse::<u64>().context("invalid hours")? * 3600);
    }
    if let Some(n) = s.strip_suffix('m') {
        return Ok(n.parse::<u64>().context("invalid minutes")? * 60);
    }
    if let Some(n) = s.strip_suffix('s') {
        return n.parse::<u64>().context("invalid seconds");
    }
    let mins: u64 = s
        .parse()
        .with_context(|| format!("invalid duration '{raw}' (use e.g. 30m, 2h, 90s)"))?;
    Ok(mins * 60)
}

fn format_remaining(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m left", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m left", secs / 60)
    } else {
        format!("{secs}s left")
    }
}

fn format_remaining_precise(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
