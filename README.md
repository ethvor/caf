# caf

**Keep your machine awake with the lid closed, and see it at a glance.**

`caf` (`caf` / `decaf`) is a simple Rust CLI for macOS, Linux, and Windows that keeps your computer awake using native OS power controls (`caffeinate` and `pmset` on macOS, `systemd-inhibit` on Linux, `SetThreadExecutionState` and `powercfg` on Windows). It handles timed shutoff and immediate or delayed display sleep, with adaptive terminal art and an optional new-terminal reminder that you are caffeinated.

---

## Why caf?

- **Close the lid, and work keeps running:** Lid-closed stay-awake is on by default so whatever you or your agent is getting up to continues while the laptop is shut. Pass `--no-lid` when you only want open-lid caffeination.
- **Control display sleep separately:** Keep the screen awake by default for GUI and computer-use agents, put the display to sleep right away with `--dark`, or keep it on for a set window before going dark while the system stays awake (`caf 2h --dark-in 10m`).
- **Know when sleep is overridden:** Live truecolor terminal art scales cleanly from wide windows down to narrow split panes (`←`/`→` palette, `↑`/`↓` pattern, `/` or `?` random, `s` save default, `r` reset random, `x` hide palette, `q` exit while keeping `caf` running), paired with an optional shell hook that shows a banner in new terminals whenever `caf` is active.

---

## Usage

```bash
caf                                  # keep awake (lid-close safe) with live animation
caf 2h                               # keep awake for 2 hours (bare number = minutes: `caf 90`)
caf 2h --dark                        # awake 2h, put display to sleep immediately
caf --dark-in 10m                    # awake indefinitely, display sleeps after 10m
caf 2h --dark-in 10m -r "agent run"  # awake 2h, display sleeps after 10m, with a reason label
caf --no-lid                         # lid-open only keep-awake (skips pmset/sudo)
caf status                           # plain-text status (`--json` supported)
caf logo --animate                   # interactive mug (`--gallery`, `--list`, alias `caf art`)
decaf                                # turn off and restore normal sleep (or `caf off`)
```

Edit `~/.config/caf/config.toml` directly or run `caf config` (`caf config --help`) to change defaults, hide palettes or patterns, add custom hex/RGB palettes, or more!

---

## Setup

Requires Rust `1.88+` (`cargo`).

### Setup with an agent

Paste this prompt into any coding agent to handle installation and initial setup:

```text
Set up caf (https://github.com/ethvor/caf) on this machine:
1. Check that Rust 1.88+ (cargo) is installed.
2. Clone https://github.com/ethvor/caf.git and install the release binary using ./scripts/install.sh (macOS/Linux), .\scripts\install.ps1 (Windows PowerShell), or `cargo install --path caf` (symlink `decaf` -> `caf` if using cargo install).
3. Ask me whether I want the new-terminal reminder hook (`caf shell install`) and, on macOS, whether to install the scoped sudoers rule for passwordless lid-closed mode and timed restore (`caf authorize-lid`), then run whichever I approve.
4. Verify installation with `caf --version` and `caf status`.
```

### Manual setup

#### 1. Install

**macOS / Linux:**

```bash
git clone https://github.com/ethvor/caf.git
cd caf && ./scripts/install.sh
```

**Windows (PowerShell):**

```powershell
git clone https://github.com/ethvor/caf.git
cd caf
.\scripts\install.ps1
```

**Any OS (Cargo):**

```bash
git clone https://github.com/ethvor/caf.git
cargo install --path caf
```

#### 2. Optional: shell reminder and passwordless lid mode

```bash
caf shell install        # show active banner in new interactive terminals (removable via `caf shell uninstall`)
caf authorize-lid        # macOS: visudo-validated sudoers rule for passwordless lid-closed mode
```

---

## License

MIT. See [LICENSE](LICENSE).
