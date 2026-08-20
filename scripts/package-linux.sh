#!/usr/bin/env bash
# Linux packaging: build the release binary and produce
#   target/package/zeron-<version>-linux-<arch>.tar.gz
# containing the binary, the .desktop entry, and the icon, plus an install.sh
# that drops them into ~/.local (XDG) paths.
#
# Usage: scripts/package-linux.sh
# Env:   PROFILE=debug for a fast unoptimized package (CI smoke); default release.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v cargo >/dev/null 2>&1 || PATH="$HOME/.cargo/bin:$PATH"
PROFILE="${PROFILE:-release}"
ARCH="$(uname -m)"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
OUT_DIR="$ROOT/target/package"
STAGE="$OUT_DIR/zeron-$VERSION-linux-$ARCH"
TARBALL="$STAGE.tar.gz"

cd "$ROOT"
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p zeron
  BIN="$ROOT/target/release/zeron"
else
  cargo build -p zeron
  BIN="$ROOT/target/debug/zeron"
fi

rm -rf "$STAGE" "$TARBALL"
mkdir -p "$STAGE"
install -m 755 "$BIN" "$STAGE/zeron"
install -m 644 "$ROOT/dist/zeron.desktop" "$STAGE/zeron.desktop"
install -m 644 "$ROOT/dist/zeron.png" "$STAGE/zeron.png"

cat >"$STAGE/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Install Zeron into ~/.local (no root needed).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin/zeron"
install -Dm755 "$HERE/zeron" "$BIN"

# The desktop entry names the binary by ABSOLUTE path. A bare `Exec=zeron`
# needs ~/.local/bin on the PATH of the DESKTOP SESSION, which is a different
# environment from the user's shell — adding the directory in a shell rc file
# (the usual advice, and what the installer used to print) does not put it
# there, so the launcher silently refused to start.
#
# ZERON_DESKTOP_ENV bakes environment into the launcher for machines that need
# it, e.g. ZERON_DESKTOP_ENV="ZERON_BACKEND=x11" where gpui's Wayland path
# leaves the window unmapped. Empty by default: nothing is forced.
if [[ -n "${ZERON_DESKTOP_ENV:-}" ]]; then
  EXEC_LINE="env ${ZERON_DESKTOP_ENV} $BIN"
else
  EXEC_LINE="$BIN"
fi

sed -e "s|^Exec=.*|Exec=$EXEC_LINE|" -e "s|^TryExec=.*|TryExec=$BIN|" \
  "$HERE/zeron.desktop" > "$HOME/.local/share/applications/zeron.desktop.tmp"
install -Dm644 "$HOME/.local/share/applications/zeron.desktop.tmp" \
  "$HOME/.local/share/applications/zeron.desktop"
rm -f "$HOME/.local/share/applications/zeron.desktop.tmp"

install -Dm644 "$HERE/zeron.png" "$HOME/.local/share/icons/hicolor/1024x1024/apps/zeron.png"
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database "$HOME/.local/share/applications" || true
echo "Installed: $BIN"
[[ -n "${ZERON_DESKTOP_ENV:-}" ]] && echo "Launcher environment: ${ZERON_DESKTOP_ENV}"
true
INSTALL
chmod 755 "$STAGE/install.sh"

tar -czf "$TARBALL" -C "$OUT_DIR" "$(basename "$STAGE")"
rm -rf "$STAGE"
echo "packaged: $TARBALL"
tar -tzf "$TARBALL"
