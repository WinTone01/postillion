#!/usr/bin/env bash
# Linux packaging: build the release binary and produce
#   target/package/postillion-<version>-linux-<arch>.tar.gz
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
STAGE="$OUT_DIR/postillion-$VERSION-linux-$ARCH"
TARBALL="$STAGE.tar.gz"

cd "$ROOT"
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p postillion
  BIN="$ROOT/target/release/postillion"
else
  cargo build -p postillion
  BIN="$ROOT/target/debug/postillion"
fi

# --- glibc tabanı doğrulaması ------------------------------------------------
# Dinamik bağlanan bir glibc ikilisi, derlendiği sürümden eskisinde yüklenirken
# kesin olarak düşüyor; dolayısıyla derleme makinesinin glibc'i sessizce
# desteklenen en düşük Ubuntu'yu belirliyor. Tabanı burada doğruluyoruz ki
# bunu bir kullanıcının "cannot open shared object file" raporundan öğrenmeyelim.
#
# Ölçü, programın o glibc'e gerçekten İHTİYACI olup olmadığı değil,
# `.gnu.version_r` içindeki sürüm REFERANSI: Rust std, 2.39 başlıklarıyla
# derlendiğinde `pidfd_spawnp`/`pidfd_getpid` için zayıf ve çalışma anında
# yoklanan bir referans yazıyor, `ld.so` ise sembol zayıf olsun olmasın
# `.gnu.version_r`'daki her girdiyi doğruluyor. Eski dağıtımda derlemek
# referansın hiç yazılmamasını sağlıyor.
#
# Üst akıştan alındı: zeronsh/comet#197.
GLIBC_MAX="${GLIBC_MAX:-2.35}" # Ubuntu 22.04 — 2027'ye kadar standart destek
needed="$(readelf -V "$BIN" | sed -n '/Version needs/,$p' \
  | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' | sort -Vu | tail -1)"
if [[ -n "$needed" && "$(printf '%s\nGLIBC_%s\n' "$needed" "$GLIBC_MAX" | sort -V | tail -1)" != "GLIBC_$GLIBC_MAX" ]]; then
  echo "package-linux.sh: ikili $needed istiyor ama taban GLIBC_$GLIBC_MAX." >&2
  echo "  $needed'den eski hiçbir sistemde yüklenmeyecek. Bunu çeken semboller:" >&2
  objdump -T "$BIN" | grep -F "($needed)" | sed 's/^/    /' >&2
  # Yayınlanan bir tarball yüklenemiyorsa hata; kendi makinesi için paket
  # yapan bir geliştiriciyi durdurmak ise anlamsız. Güncel bir dağıtımda
  # (ölçüldü: glibc 2.44) her yerel derleme bu tabanı aşıyor ve katı bir
  # kontrol geliştiricinin kendi kurulumunu imkânsız kılardı.
  if [[ -n "${CI:-}" ]]; then
    echo "  Çözüm: desteklenen en eski koşucuda derleyin (.github/workflows/release.yml)," >&2
    echo "  ya da eski dağıtımları bilerek bırakmak için GLIBC_MAX'ı yükseltin." >&2
    exit 1
  fi
  echo "  Yerel derleme: paketleme sürüyor. Bu tarball YALNIZCA bu makine ve" >&2
  echo "  daha yenisi için geçerli; yayınlanacak sürüm CI'da üretilmeli." >&2
else
  echo "glibc tabanı uygun: ${needed:-yok} isteniyor (izin verilen en yüksek GLIBC_$GLIBC_MAX)"
fi

rm -rf "$STAGE" "$TARBALL"
mkdir -p "$STAGE"
install -m 755 "$BIN" "$STAGE/postillion"
install -m 644 "$ROOT/dist/postillion.desktop" "$STAGE/postillion.desktop"
install -m 644 "$ROOT/dist/postillion.svg" "$STAGE/postillion.svg"
# Her boyut vektörden ayrı üretiliyor: tek bir büyüğü küçültmek ince
# çizgileri bulanıklaştırıyor.
mkdir -p "$STAGE/icons"
if command -v rsvg-convert >/dev/null 2>&1; then
  for size in 16 22 24 32 48 64 96 128 256 512 1024; do
    rsvg-convert -w "$size" -h "$size" "$ROOT/dist/postillion.svg" \
      -o "$STAGE/icons/postillion-${size}.png"
  done
fi
install -m 644 "$ROOT/dist/postillion.png" "$STAGE/postillion.png"

cat >"$STAGE/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Install Postillion into ~/.local (no root needed).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin/postillion"
install -Dm755 "$HERE/postillion" "$BIN"

# The desktop entry names the binary by ABSOLUTE path. A bare `Exec=postillion`
# needs ~/.local/bin on the PATH of the DESKTOP SESSION, which is a different
# environment from the user's shell — adding the directory in a shell rc file
# (the usual advice, and what the installer used to print) does not put it
# there, so the launcher silently refused to start.
#
# POSTILLION_DESKTOP_ENV bakes environment into the launcher for machines that need
# it, e.g. POSTILLION_DESKTOP_ENV="POSTILLION_BACKEND=x11" where gpui's Wayland path
# leaves the window unmapped. Empty by default: nothing is forced.
if [[ -n "${POSTILLION_DESKTOP_ENV:-}" ]]; then
  EXEC_LINE="env ${POSTILLION_DESKTOP_ENV} $BIN"
else
  EXEC_LINE="$BIN"
fi

sed -e "s|^Exec=.*|Exec=$EXEC_LINE|" -e "s|^TryExec=.*|TryExec=$BIN|" \
  "$HERE/postillion.desktop" > "$HOME/.local/share/applications/postillion.desktop.tmp"
install -Dm644 "$HOME/.local/share/applications/postillion.desktop.tmp" \
  "$HOME/.local/share/applications/postillion.desktop"
rm -f "$HOME/.local/share/applications/postillion.desktop.tmp"

# İkon HER standart boyuta kuruluyor, yalnızca 1024'e değil.
#
# Görev çubuğu ve menü küçük boyutları seçiyor; tek bir büyük dosya
# bırakıldığında tema arayışı bir ÖNCEKİ kurulumdan kalan küçük ikonlara
# düşüyor ve eski logo görünmeye devam ediyor (ölçüldü: bu makinede Tauri
# sürümünden kalan 16–512 px ikonlar hâlâ oradaydı).
for size in 16 22 24 32 48 64 96 128 256 512 1024; do
  src="$HERE/icons/postillion-${size}.png"
  [[ -f "$src" ]] || src="$HERE/postillion.png"
  install -Dm644 "$src" \
    "$HOME/.local/share/icons/hicolor/${size}x${size}/apps/postillion.png"
done
if [[ -f "$HERE/postillion.svg" ]]; then
  install -Dm644 "$HERE/postillion.svg" \
    "$HOME/.local/share/icons/hicolor/scalable/apps/postillion.svg"
fi
# Tema önbelleği tazelenmezse yeni dosyalar görünmüyor.
command -v gtk-update-icon-cache >/dev/null 2>&1 \
  && gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database "$HOME/.local/share/applications" || true
echo "Installed: $BIN"
[[ -n "${POSTILLION_DESKTOP_ENV:-}" ]] && echo "Launcher environment: ${POSTILLION_DESKTOP_ENV}"
true
INSTALL
chmod 755 "$STAGE/install.sh"

tar -czf "$TARBALL" -C "$OUT_DIR" "$(basename "$STAGE")"
rm -rf "$STAGE"
echo "packaged: $TARBALL"
tar -tzf "$TARBALL"
