#!/usr/bin/env bash
#
# Remove Postillion from Linux.
#
# Only files this project installed are touched. Your Claude data
# (~/.claude and ~/.claude-accounts) is never removed — sessions, credentials
# and account setup all live there and deleting them would be unrecoverable.

set -euo pipefail

APP_NAME="Postillion"
BIN_NAME="postillion"

PREFIX="$HOME/.local"
PURGE_ACCOUNTS=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --system            Remove from /usr/local (requires root)
  --prefix PATH       Remove from a custom prefix
  --purge-accounts    Also delete ~/.claude-accounts (extra accounts you added
                      through Postillion). Your default ~/.claude is never
                      touched. This cannot be undone.
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --system) PREFIX="/usr/local"; shift ;;
    --prefix) PREFIX="${2:?--prefix needs a path}"; shift 2 ;;
    --purge-accounts) PURGE_ACCOUNTS=1; shift ;;
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

if ! can_write "$PREFIX"; then
  die "cannot write to $PREFIX — re-run with sudo"
fi

removed=0

remove_file() {
  if [[ -e "$1" ]]; then
    rm -f "$1"
    echo "  removed $1"
    removed=$((removed + 1))
  fi
}

# Stop a running instance first, otherwise the binary stays busy and the
# desktop entry can be re-registered by the session.
if pgrep -x "$BIN_NAME" >/dev/null 2>&1; then
  log "Stopping running $APP_NAME"
  pkill -x "$BIN_NAME" || true
  sleep 1
fi

log "Removing files"
remove_file "$BIN_DIR/$BIN_NAME"
remove_file "$DESKTOP_DIR/$BIN_NAME.desktop"

for size in 16x16 22x22 24x24 32x32 48x48 64x64 96x96 128x128 256x256 512x512; do
  remove_file "$ICON_ROOT/$size/apps/$BIN_NAME.png"
done
remove_file "$PREFIX/share/pixmaps/$BIN_NAME.png"

if command -v update-desktop-database >/dev/null; then
  update-desktop-database -q "$DESKTOP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -qtf "$ICON_ROOT" 2>/dev/null || true
fi
if command -v kbuildsycoca6 >/dev/null; then
  kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi
if command -v xdg-desktop-menu >/dev/null; then
  xdg-desktop-menu forceupdate >/dev/null 2>&1 || true
fi

if [[ "$PURGE_ACCOUNTS" -eq 1 ]]; then
  ACCOUNTS_DIR="$HOME/.claude-accounts"
  if [[ -d "$ACCOUNTS_DIR" ]]; then
    log "Deleting $ACCOUNTS_DIR"
    # Entries there are symlinks into ~/.claude. Unlink them explicitly so a
    # follow-through can never reach the real transcripts.
    while IFS= read -r -d '' link; do
      rm -f "$link"
    done < <(find "$ACCOUNTS_DIR" -maxdepth 2 -type l -print0)
    rm -rf "$ACCOUNTS_DIR"
    echo "  removed $ACCOUNTS_DIR"
  fi
fi

if [[ "$removed" -eq 0 ]]; then
  warn "nothing found under $PREFIX — was it installed elsewhere?"
else
  log "Removed $APP_NAME ($removed file(s))"
fi

echo
echo "Your Claude data is untouched:"
echo "  ~/.claude            sessions, credentials, settings"
if [[ "$PURGE_ACCOUNTS" -eq 0 && -d "$HOME/.claude-accounts" ]]; then
  echo "  ~/.claude-accounts   extra accounts (use --purge-accounts to delete)"
fi
