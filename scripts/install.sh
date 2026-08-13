#!/usr/bin/env bash
#
# Install Postillion on Linux.
#
# Installs to ~/.local by default, which needs no root and follows the XDG
# layout. Pass --system for /usr/local (requires root).
#
# The release binary is built if it is missing. Bundling (deb/AppImage) is
# skipped on purpose: this script places the binary, desktop entry and icons
# itself, so no extra packaging tooling is required.

set -euo pipefail

APP_NAME="Postillion"
BIN_NAME="postillion"
COMMENT="Switch Claude accounts without losing the conversation"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_BIN="$REPO_ROOT/src-tauri/target/release/$BIN_NAME"
ICON_DIR="$REPO_ROOT/src-tauri/icons"

PREFIX="$HOME/.local"
FORCE_BUILD=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --system       Install to /usr/local for all users (requires root)
  --prefix PATH  Install under a custom prefix
  --build        Rebuild the release binary even if one exists
  -h, --help     Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --system) PREFIX="/usr/local"; shift ;;
    --prefix) PREFIX="${2:?--prefix needs a path}"; shift 2 ;;
    --build)  FORCE_BUILD=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

BIN_DIR="$PREFIX/bin"
DESKTOP_DIR="$PREFIX/share/applications"
ICON_ROOT="$PREFIX/share/icons/hicolor"

log()  { printf '\033[1;36m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# Whether the prefix can be created/written.
#
# Checking "is it under $HOME" would be wrong: /tmp is writable and /home/other
# is not. Walk up to the nearest existing ancestor, since that is what decides
# whether the tree can be created.
can_write() {
  local dir="$1"
  while [[ ! -e "$dir" && "$dir" != "/" ]]; do
    dir="$(dirname "$dir")"
  done
  [[ -w "$dir" ]]
}

# Fail early with a clear message rather than halfway through.
if ! can_write "$PREFIX"; then
  die "cannot write to $PREFIX — re-run with sudo"
fi

# --- binary ------------------------------------------------------------------

if [[ ! -x "$RELEASE_BIN" || "$FORCE_BUILD" -eq 1 ]]; then
  log "Building release binary (this takes a few minutes)"

  command -v npm >/dev/null   || die "npm not found — needed to build"
  command -v cargo >/dev/null || die "cargo not found — needed to build"

  [[ -d "$REPO_ROOT/node_modules" ]] || (cd "$REPO_ROOT" && npm install)
  (cd "$REPO_ROOT" && npx tauri build --no-bundle)
fi

[[ -x "$RELEASE_BIN" ]] || die "release binary missing at $RELEASE_BIN"

# Claude Code is the engine; without it the app starts but every session fails.
command -v claude >/dev/null || warn "\`claude\` not found on PATH — install Claude Code before using Postillion"

# --- install -----------------------------------------------------------------

log "Installing binary to $BIN_DIR"
install -Dm755 "$RELEASE_BIN" "$BIN_DIR/$BIN_NAME"

log "Installing icons to $ICON_ROOT"
declare -A ICONS=(
  ["32x32"]="32x32.png"
  ["128x128"]="128x128.png"
  ["256x256"]="128x128@2x.png"
  ["512x512"]="icon.png"
)
for size in "${!ICONS[@]}"; do
  src="$ICON_DIR/${ICONS[$size]}"
  [[ -f "$src" ]] || continue
  install -Dm644 "$src" "$ICON_ROOT/$size/apps/$BIN_NAME.png"
done

log "Installing desktop entry to $DESKTOP_DIR"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/$BIN_NAME.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=$COMMENT
Exec=$BIN_DIR/$BIN_NAME
Icon=$BIN_NAME
Terminal=false
Categories=Development;
StartupWMClass=$BIN_NAME
EOF
chmod 644 "$DESKTOP_DIR/$BIN_NAME.desktop"

# Refresh caches so the launcher picks the app up without a re-login.
# Both tools are optional; a missing cache only delays the menu entry.
if command -v update-desktop-database >/dev/null; then
  update-desktop-database -q "$DESKTOP_DIR" || true
fi
if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -qtf "$ICON_ROOT" 2>/dev/null || true
fi

# --- report ------------------------------------------------------------------

log "Installed $APP_NAME"
echo "  binary : $BIN_DIR/$BIN_NAME"
echo "  desktop: $DESKTOP_DIR/$BIN_NAME.desktop"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) warn "$BIN_DIR is not on your PATH — add it to launch from a terminal" ;;
esac

echo
echo "Launch it from your application menu, or run: $BIN_NAME"
