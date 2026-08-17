//! Ekran görüntüsü alma.
//!
//! Tarayıcının `getDisplayMedia`'sı kullanılmıyor: WebKitGTK'da ekran paylaşımı
//! yok ve olsa bile Wayland altında portal izni ayrı bir akış. Bunun yerine
//! masaüstünün kendi bölge seçme aracı çalıştırılıyor — kullanıcı zaten o
//! arayüzü tanıyor.
//!
//! Araçlar sırayla deneniyor; ilk bulunan kazanıyor. Hiçbiri yoksa kullanıcıya
//! ne kuracağı söyleniyor.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::error::{Error, Result};

/// Base64'e çevrilmiş, doğrudan Claude'a gönderilebilir bir görüntü.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub media_type: String,
    /// Base64; ham ikili veri IPC'den geçemiyor.
    pub data: String,
}

/// Bölge seçtiren ekran görüntüsü araçları, tercih sırasıyla.
///
/// Sıralama masaüstüne göre değil kurululuğa göre: kullanıcı hangi aracı
/// kurduysa onu tanıyordur. `grim` tek başına bölge seçemediği için `slurp`
/// ile birlikte ele alınıyor.
const TOOLS: &[(&str, &[&str])] = &[
    // KDE. `-b` arayüzü açmadan, `-n` bildirim çıkarmadan.
    ("spectacle", &["-b", "-n", "-r", "-o"]),
    ("gnome-screenshot", &["-a", "-f"]),
    // X11.
    ("maim", &["-s"]),
    ("scrot", &["-s"]),
    // ImageMagick; seçim imleci veriyor.
    ("import", &[]),
];

/// Aracı PATH'te arar ve mutlak yolunu döndürür.
///
/// Mutlak yol, `claude_bin()` ile aynı gerekçeyle: uygulama masaüstü
/// menüsünden başlatıldığında PATH kabuktakinden dar oluyor ve
/// `~/.local/bin`'deki bir araç bulunamıyor.
fn resolve(program: &str) -> Option<PathBuf> {
    resolve_in(program, std::env::split_paths(&std::env::var_os("PATH")?))
}

fn resolve_in(program: &str, dirs: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(program);
        // Dizin adı eşleşmesi araç sayılmamalı.
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Bölge seçtirip ekran görüntüsü alır.
///
/// Kullanıcı seçimi iptal ederse `Ok(None)` döner — bu bir hata değil.
pub fn capture_region() -> Result<Option<Shot>> {
    // Aynı isim iki kez kullanılmasın diye süreç kimliği ve zaman damgası.
    let name = format!(
        "postillion-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let target = std::env::temp_dir().join(name);

    let taken = run_tool(&target)?;
    let result = if taken { read_shot(&target)? } else { None };

    // Görüntü artık bellekte; diskte kalması gereksiz.
    let _ = std::fs::remove_file(&target);

    Ok(result)
}

/// Yazılmış dosyayı okur.
///
/// Dosya yoksa ya da boşsa görüntü de yok: iptal edildiğinde araçların çıkış
/// kodu tutarsız, tek güvenilir ölçüt dosyanın kendisi.
fn read_shot(path: &Path) -> Result<Option<Shot>> {
    if !path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(None);
    }

    Ok(Some(Shot {
        media_type: "image/png".into(),
        data: encode_base64(&std::fs::read(path)?),
    }))
}

/// İlk bulunan aracı çalıştırır. Dönen değer aracın başarı bildirip
/// bildirmediği; dosyanın gerçekten yazıldığı ayrıca kontrol ediliyor.
fn run_tool(target: &Path) -> Result<bool> {
    // wlroots tabanlı ortamlar: bölgeyi `slurp` seçiyor, `grim` yakalıyor.
    if let (Some(grim), Some(slurp)) = (resolve("grim"), resolve("slurp")) {
        let region = Command::new(slurp).stderr(Stdio::null()).output()?;
        if !region.status.success() {
            return Ok(false); // Esc'e basıldı.
        }
        let geometry = String::from_utf8_lossy(&region.stdout).trim().to_string();
        if geometry.is_empty() {
            return Ok(false);
        }

        let status = Command::new(grim)
            .args(["-g", &geometry])
            .arg(target)
            .status()?;
        return Ok(status.success());
    }

    for (program, args) in TOOLS {
        let Some(path) = resolve(program) else {
            continue;
        };
        let status = Command::new(path).args(*args).arg(target).status()?;
        return Ok(status.success());
    }

    Err(Error::Other(
        "ekran görüntüsü aracı bulunamadı — spectacle, grim+slurp, \
         gnome-screenshot, maim veya scrot kurabilirsiniz"
            .into(),
    ))
}

/// Panodaki görüntüyü PNG olarak okur; görüntü yoksa `None`.
///
/// Neden Rust tarafında: JS yolu eklentiden ham RGBA alıp `ImageData` ve
/// canvas üzerinden PNG üretiyordu — dört ayrı yerde sessizce düşebilen bir
/// zincir. Burada tek adım var ve `arboard`'ın Wayland yolu ölçülerek
/// doğrulandı.
pub fn clipboard_image() -> Option<Shot> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;

    let width = u32::try_from(image.width).ok()?;
    let height = u32::try_from(image.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.bytes).ok()?;
    }

    Some(Shot {
        media_type: "image/png".into(),
        data: encode_base64(&png),
    })
}

/// Base64 kodlar.
///
/// Tek kullanım için bir bağımlılık eklemeye değmiyor; alfabe standart.
fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);

        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{encode_base64, read_shot, resolve_in};
    use std::path::PathBuf;

    /// İptal edilen bir yakalama hata değil, "görüntü yok" demek: araçlar iptal
    /// edildiğinde ya dosyayı hiç yazmıyor ya da boş bırakıyor.
    #[test]
    fn eksik_ve_bos_dosya_goruntu_uretmez() {
        let dir = std::env::temp_dir().join(format!("postillion-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("yok.png");
        assert!(read_shot(&missing).unwrap().is_none());

        let empty = dir.join("bos.png");
        std::fs::write(&empty, b"").unwrap();
        assert!(read_shot(&empty).unwrap().is_none());

        let real = dir.join("var.png");
        std::fs::write(&real, b"foobar").unwrap();
        let shot = read_shot(&real).unwrap().unwrap();
        assert_eq!(shot.media_type, "image/png");
        assert_eq!(shot.data, "Zm9vYmFy");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Araç mutlak yolla çalıştırılıyor; dizin adı eşleşmesi araç sayılmamalı.
    #[test]
    fn arac_pathte_aranir() {
        let root = std::env::temp_dir().join(format!("postillion-resolve-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("spectacle"), b"#!/bin/sh\n").unwrap();
        // Aynı isimde bir dizin: yanlışlıkla seçilirse çalıştırma hata verir.
        std::fs::create_dir_all(root.join("tuzak").join("grim")).unwrap();

        let dirs = || {
            [root.join("tuzak"), bin.clone()]
                .into_iter()
                .collect::<Vec<PathBuf>>()
        };

        assert_eq!(
            resolve_in("spectacle", dirs().into_iter()),
            Some(bin.join("spectacle"))
        );
        assert_eq!(resolve_in("grim", dirs().into_iter()), None);
        assert_eq!(resolve_in("yok", dirs().into_iter()), None);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn base64_dolgulu_ve_dolgusuz_uzunluklari_kodlar() {
        // RFC 4648 test vektörleri; dolgu sınırları burada hata yapılır.
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_ikili_veriyi_kodlar() {
        // Yüksek bitli baytlar işaretli/işaretsiz karışınca bozulur.
        assert_eq!(encode_base64(&[0x00, 0xff, 0x80]), "AP+A");
        assert_eq!(encode_base64(&[0xfb, 0xff]), "+/8=");
    }
}
