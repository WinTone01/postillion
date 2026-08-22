#!/bin/sh
# Postillion (native) headless installer.
#
# `postillion.invalid` is a placeholder, not a real host: this fork publishes no
# hosted install surface. `.invalid` is reserved by RFC 2606 and can never be
# registered, so a stale copy of this script cannot be pointed at someone
# else's server. Set POSTILLION_BASE_URL to your own release host to use it:
#
#   curl -fsSL https://your-host.example/install.sh | POSTILLION_BASE_URL=https://your-host.example sh
#
# Installs the native binary to ~/.postillion/app, puts `postillion` on PATH,
# and runs it as a local-only systemd user service that survives reboots.
# Signing in is optional and enables sync after a restart. Re-running upgrades
# in place; ~/.postillion state is preserved.
#
# It does need a couple of system libraries (libxkbcommon-x11, libxcb) that
# minimal server images omit — see the preflight check below.
#
# The binary ships with production endpoints baked in: no POSTILLION_EDGE_URL or
# client-id configuration needed. Overrides (if any) go in ~/.postillion/env.
set -eu

BASE="${POSTILLION_BASE_URL:-https://postillion.invalid}"

# --- platform ---------------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) plat=linux ;;
  Darwin)
    echo "postillion install: on macOS, download the desktop app instead:" >&2
    echo "  $BASE/releases/latest.txt → $BASE/releases/postillion-<version>-macos-arm64.dmg" >&2
    exit 1
    ;;
  *)
    echo "postillion install: unsupported OS '$os' — only Linux for now." >&2
    exit 1
    ;;
esac
case "$arch" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *)
    echo "postillion install: unsupported architecture '$arch'." >&2
    exit 1
    ;;
esac

# --- download ----------------------------------------------------------------
ver="$(curl -fsSL "$BASE/releases/latest.txt" | tr -d '[:space:]')"
[ -n "$ver" ] || { echo "postillion install: could not resolve latest version" >&2; exit 1; }
file="postillion-$ver-$plat-$arch.tar.gz"
data_root="$HOME/.postillion"
app_root="$data_root/app"
dest="$app_root/$ver"

if [ -x "$dest/postillion" ]; then
  echo "postillion $ver already downloaded — relinking."
else
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  echo "downloading postillion $ver ($plat-$arch)…"
  curl -fSL --progress-bar "$BASE/releases/$file" -o "$tmp/$file"
  mkdir -p "$dest"
  tar -xzf "$tmp/$file" -C "$dest" --strip-components=1
fi

# --- preflight: sistem kütüphaneleri -----------------------------------------
# İkili tam anlamıyla kendine yeterli DEĞİL: gpui, libxkbcommon-x11 ve libxcb'yi
# katı `DT_NEEDED` girdileri olarak bağlıyor, dolayısıyla bunlar yoksa HİÇBİR
# alt komut yüklenemiyor — pencere açmayan `headless`, `login` ve `status`
# dahil. Sunucu ve bulut imajlarının çoğu bu kütüphaneleri getirmiyor.
#
# Kontrol systemd ikiliye yönlendirilmeden ÖNCE yapılıyor ki hata,
# `systemctl --user status postillion` altında bir yeniden başlatma döngüsü
# olarak değil, eksik paketin adıyla görünsün.
#
# Ayrıca `current` yeniden yönlendirilmeden önce: yükseltmede, yüklenemeyen bir
# ikili sorunsuz çalışan sürümün yerini almamalı.
#
# Üst akıştan alındı: zeronsh/comet#197.
if command -v ldd >/dev/null 2>&1; then
  ldd_out="$(ldd "$dest/postillion" 2>&1 || true)"

  # Yayınlanan sürümün derlendiği glibc'ten eski bir sistem: yükleyici
  # karşılayamadığı bir sürüm bildiriyor ve kullanıcının kurabileceği hiçbir
  # paket bunu düzeltmiyor.
  glibc_want="$(printf '%s\n' "$ldd_out" \
    | sed -n "s/.*version .\(GLIBC_[0-9.]*\). not found.*/\1/p" | head -1)"
  if [ -n "$glibc_want" ]; then
    echo "" >&2
    echo "postillion install: bu derleme $glibc_want istiyor; bu sistemdeki glibc" >&2
    echo "  daha eski ($(ldd --version 2>/dev/null | head -1))." >&2
    echo "" >&2
    echo "  Yayınlanan ikili bu dağıtım için fazla yeni — bu bir paketleme hatası," >&2
    echo "  kurarak aşabileceğiniz bir şey değil. Lütfen bildirin:" >&2
    echo "  https://github.com/WinTone01/postillion/issues" >&2
    exit 1
  fi

  missing="$(printf '%s\n' "$ldd_out" | awk '/=> not found/ { print $1 }' | sort -u)"
  if [ -n "$missing" ]; then
    if command -v apt-get >/dev/null 2>&1; then
      hint="sudo apt-get install -y libxkbcommon-x11-0 libxcb1"
    elif command -v dnf >/dev/null 2>&1; then
      hint="sudo dnf install -y libxkbcommon-x11 libxcb"
    elif command -v pacman >/dev/null 2>&1; then
      hint="sudo pacman -S --needed libxkbcommon-x11 libxcb"
    elif command -v zypper >/dev/null 2>&1; then
      hint="sudo zypper install -y libxkbcommon-x11-0 libxcb1"
    elif command -v apk >/dev/null 2>&1; then
      hint="sudo apk add libxkbcommon libxcb"
    else
      hint=""
    fi
    echo "" >&2
    echo "postillion install: eksik sistem kütüphaneleri:" >&2
    printf '  %s\n' $missing >&2
    if [ -n "$hint" ]; then
      echo "" >&2
      echo "  şununla kurun:" >&2
      echo "    $hint" >&2
      echo "" >&2
      echo "  sonra bu kurulumu yeniden çalıştırın." >&2
    else
      echo "" >&2
      echo "  bunları sağlayan paketleri kurup kurulumu yeniden çalıştırın." >&2
    fi
    exit 1
  fi
fi

ln -sfn "$dest" "$app_root/current"
mkdir -p "$HOME/.local/bin"
ln -sf "$app_root/current/postillion" "$HOME/.local/bin/postillion"

# --- service -----------------------------------------------------------------
# The daemon is useful before auth: without a saved session it serves the local
# profile. Login only changes which profile the next daemon start selects.

service=manual
if command -v systemctl >/dev/null 2>&1 && [ -n "${XDG_RUNTIME_DIR:-}" ]; then
  mkdir -p "$HOME/.config/systemd/user"
  cat >"$HOME/.config/systemd/user/postillion.service" <<'UNIT'
[Unit]
Description=Postillion native headless engine
After=network-online.target
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
ExecStart=%h/.postillion/app/current/postillion headless
Restart=on-failure
RestartSec=5
EnvironmentFile=-%h/.postillion/env

[Install]
WantedBy=default.target
UNIT
  systemctl --user daemon-reload
  systemctl --user enable postillion
  systemctl --user restart postillion
  service=running
  # Keep the user manager (and the engine) running without an active login.
  loginctl enable-linger "$USER" 2>/dev/null \
    || sudo -n loginctl enable-linger "$USER" 2>/dev/null \
    || echo "warn: could not enable linger — the engine stops when you log out (run: sudo loginctl enable-linger $USER)"
else
  echo "warn: systemd user session not available — run the engine manually with: postillion headless"
fi

# --- agent CLIs ---------------------------------------------------------------
command -v claude >/dev/null 2>&1 || \
  echo "note: Claude Code CLI not found — install it with: curl -fsSL https://claude.ai/install.sh | bash"

case ":$PATH:" in
  *":$HOME/.local/bin:"*) path_hint="" ;;
  *) path_hint=' (add ~/.local/bin to your PATH)' ;;
esac

echo ""
echo "✓ postillion $ver installed$path_hint"
echo ""
case "$service" in
  running)
    echo "the engine is running with the new version (local-only unless sync is enabled)."
    echo "  systemctl --user status postillion    check the service"
    echo ""
    echo "optional sync (local sessions stay local):"
    echo "  systemctl --user stop postillion"
    echo "  postillion login"
    echo "  systemctl --user restart postillion"
    ;;
  manual)
    echo "next: run the local-only engine with \`postillion headless\`."
    echo "optional sync: run \`postillion login\` before starting the engine."
    ;;
esac
