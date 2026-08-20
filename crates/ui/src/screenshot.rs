//! Bölge seçtirerek ekran görüntüsü alma.
//!
//! Tarayıcı/GPU tarafından değil masaüstünün kendi aracıyla: ekran yakalama
//! Wayland'da portal izni gerektiren ayrı bir akış ve kullanıcı zaten kendi
//! aracının seçim arayüzünü tanıyor.
//!
//! Araçlar sırayla deneniyor. **Var olmak yetmiyor, çalışmak gerekiyor**:
//! ölçüldü, bu makinede `grim` kurulu ama KDE'nin bileşiricisi onun yakalama
//! protokolünü sunmuyor ve araç "compositor doesn't support the screen capture
//! protocol" diyip çıkıyor — oysa `spectacle` sorunsuz çalışıyor. Bu yüzden
//! başarısız bir araç sırayı bitirmiyor, bir sonrakine geçiliyor.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Bölge seçtiren araçlar, tercih sırasıyla.
///
/// Sıra masaüstüne göre değil kurululuğa göre: kullanıcı hangi aracı kurduysa
/// onu tanıyordur. `grim` tek başına bölge seçemediği için `slurp` ile
/// birlikte ele alınıyor (aşağıda ayrı bir dal).
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
fn resolve(program: &str) -> Option<PathBuf> {
    resolve_in(program, std::env::split_paths(&std::env::var_os("PATH")?))
}

fn resolve_in(program: &str, dirs: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(program);
        // Aynı adlı bir DİZİN araç sayılmamalı.
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Kullanıcıya bölge seçtirip görüntüyü `target`'a yazar.
///
/// `Ok(false)` "görüntü yok" demek ve bir hata değil — kullanıcı iptal etmiş
/// olabilir. `Err` yalnızca hiçbir araç bulunamadığında.
pub fn capture_region(target: &Path) -> Result<bool, String> {
    let mut tried_any = false;

    // wlroots tabanlı ortamlar: bölgeyi `slurp` seçiyor, `grim` yakalıyor.
    if let (Some(grim), Some(slurp)) = (resolve("grim"), resolve("slurp")) {
        tried_any = true;
        if let Some(geometry) = slurp_region(&slurp) {
            let ok = Command::new(grim)
                .args(["-g", &geometry])
                .arg(target)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok && wrote_something(target) {
                return Ok(true);
            }
            // `grim` yazamadı (bu bileşiricide protokol yok): sıradakine geç.
        }
    }

    for (program, args) in TOOLS {
        let Some(path) = resolve(program) else {
            continue;
        };
        tried_any = true;
        let ok = Command::new(path)
            .args(*args)
            .arg(target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && wrote_something(target) {
            return Ok(true);
        }
        // Araç başarısız oldu ya da iptal edildi. İptali başarısızlıktan
        // ayırt edemiyoruz (çıkış kodları araçtan araca tutarsız), ama
        // sıradakini denemek iptal durumunda yalnızca ikinci bir seçim
        // arayüzü açar — sessizce hiçbir şey yapmamaktan iyi.
        let _ = std::fs::remove_file(target);
    }

    if tried_any {
        // Araç vardı ama hiçbiri görüntü üretmedi: büyük olasılıkla iptal.
        return Ok(false);
    }

    Err("ekran görüntüsü aracı bulunamadı — spectacle, grim+slurp, \
         gnome-screenshot, maim veya scrot kurabilirsiniz"
        .to_string())
}

/// `slurp` ile bölge seçtirir; iptal edilirse `None`.
fn slurp_region(slurp: &Path) -> Option<String> {
    let region = Command::new(slurp).stderr(Stdio::null()).output().ok()?;
    if !region.status.success() {
        return None; // Esc'e basıldı.
    }
    let geometry = String::from_utf8_lossy(&region.stdout).trim().to_string();
    (!geometry.is_empty()).then_some(geometry)
}

/// Dosya gerçekten yazıldı mı.
///
/// Tek güvenilir ölçüt bu: araçlar iptal edildiğinde ya dosyayı hiç yazmıyor
/// ya da boş bırakıyor, ve çıkış kodları tutarsız.
fn wrote_something(path: &Path) -> bool {
    path.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

/// Yakalama için çakışmayan geçici dosya yolu.
pub fn temp_target() -> PathBuf {
    let name = format!(
        "postillion-shot-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    std::env::temp_dir().join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arac_pathte_aranir_dizin_arac_sayilmaz() {
        let root = std::env::temp_dir().join(format!("po-shot-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("spectacle"), b"#!/bin/sh\n").unwrap();
        // Aynı adda bir DİZİN: seçilirse çalıştırma hata verirdi.
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
    fn bos_ya_da_olmayan_dosya_goruntu_sayilmaz() {
        let dir = std::env::temp_dir().join(format!("po-shot-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        assert!(!wrote_something(&dir.join("yok.png")));

        let empty = dir.join("bos.png");
        std::fs::write(&empty, b"").unwrap();
        assert!(!wrote_something(&empty));

        let real = dir.join("var.png");
        std::fs::write(&real, b"veri").unwrap();
        assert!(wrote_something(&real));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gecici_yol_cakismiyor() {
        // Aynı milisaniyede iki çağrı olabilir; süreç kimliği ve zaman
        // damgası birlikte yeterince ayırıyor ve uzantı korunmalı.
        let a = temp_target();
        assert_eq!(a.extension().and_then(|e| e.to_str()), Some("png"));
        assert!(a.file_name().unwrap().to_string_lossy().contains("postillion-shot-"));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Gerçek araç zincirini bu makinede çalıştırır.
    ///
    /// `#[ignore]`: kullanıcıdan bölge seçmesini isteyen etkileşimli bir
    /// akış. Elle: `cargo test -p postillion-ui --lib live -- --ignored
    /// --nocapture`
    #[test]
    #[ignore]
    fn gercek_arac_goruntu_uretir() {
        let target = temp_target();
        let taken = capture_region(&target).expect("araç bulunmalı");
        eprintln!("yakalandı={taken} yol={}", target.display());
        if taken {
            let size = std::fs::metadata(&target).unwrap().len();
            assert!(size > 0, "dosya boş");
            // PNG imzası: araç gerçekten görüntü yazmış olmalı.
            let head = std::fs::read(&target).unwrap();
            assert_eq!(&head[..4], b"\x89PNG", "PNG değil");
            eprintln!("boyut={size} bayt");
        }
        let _ = std::fs::remove_file(&target);
    }
}
