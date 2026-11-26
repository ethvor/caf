#!/usr/bin/env bash
set -euo pipefail

# Standalone macOS & Linux installer for caf (+ decaf symlink).
# Usage:
#   ./scripts/install.sh                    # build & install caf (+ decaf) into ~/.local/bin
#   ./scripts/install.sh --bin-dir /usr/local/bin
#   ./scripts/install.sh --shell            # also install managed shell reminder hook

BIN_DIR="${CAF_BIN_DIR:-$HOME/.local/bin}"
INSTALL_SHELL=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin-dir) BIN_DIR="${2:?--bin-dir requires a directory}"; shift 2 ;;
    --shell) INSTALL_SHELL=1; shift ;;
    -h|--help)
      cat <<'EOF'
Usage: ./scripts/install.sh [OPTIONS]
  --bin-dir <dir>   Target directory for caf and decaf (default: ~/.local/bin)
  --shell           Run `caf shell install` after install to add the new-terminal reminder hook
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "==> Building caf (release)..."
cargo build --release --manifest-path "$REPO_DIR/Cargo.toml"

mkdir -p "$BIN_DIR"
cp "$REPO_DIR/target/release/caf" "$BIN_DIR/caf.new"
chmod 755 "$BIN_DIR/caf.new"
mv -f "$BIN_DIR/caf.new" "$BIN_DIR/caf"
ln -sf "$BIN_DIR/caf" "$BIN_DIR/decaf"

echo "==> Installed $BIN_DIR/caf and $BIN_DIR/decaf ($("$BIN_DIR/caf" --version))"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo "Note: $BIN_DIR is not on your PATH. Add this to your shell rc file:"
    echo "  export PATH=\"$BIN_DIR:\$PATH\""
    ;;
esac

if [[ "$INSTALL_SHELL" -eq 1 ]]; then
  "$BIN_DIR/caf" shell install
fi
