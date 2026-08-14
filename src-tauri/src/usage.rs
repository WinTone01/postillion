//! Hesabın kalan kullanım payı.
//!
//! Kaynak `/usage` slash komutu. Yapılandırılmış bir API yok; komut headless
//! modda da çalışıyor ve metin döndürüyor (ölçüldü: ~3 sn, sıfır token — yerel
//! bir komut, API çağrısı yapmıyor).
//!
//! Sorgu yalnızca **etkin** hesap için yapılabiliyor: `claude` kimliği
//! `~/.claude/.credentials.json`'dan okuyor ve bu uygulamada hesap değiştirmek
//! sistem geneli bir işlem. Diğer hesapların değerleri en son etkin
//! olduklarında ölçülüp önbelleğe yazılıyor; arayüz onları ölçüm zamanıyla
//! birlikte gösteriyor.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::paths;

/// `claude -p "/usage"` bu süreden uzun sürerse bırakılıyor.
///
/// Ölçülen süre ~3 sn; 30 sn yalnızca bir şeyin takıldığı anlamına gelir ve
/// arayüzün süresiz beklememesi gerekiyor.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Tek bir limit penceresi, ör. "session" ya da "week (all models)".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// `/usage` çıktısındaki İngilizce etiket; çeviri arayüzde.
    pub label: String,
    pub percent: u8,
    /// "Aug 19, 7am (Europe/Istanbul)" — sıfır kullanımda bu satır gelmiyor.
    pub resets: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub windows: Vec<Window>,
    pub measured_at_ms: u64,
    /// Komutun tam çıktısı; arayüz ayrıntı kartında gösteriyor.
    pub detail: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Etkin hesabın kullanımını ölçer.
pub fn query() -> Result<Usage> {
    let text = run_usage_command()?;

    Ok(Usage {
        windows: parse(&text),
        measured_at_ms: now_ms(),
        detail: text,
    })
}

fn run_usage_command() -> Result<String> {
    let mut child = Command::new(paths::claude_bin())
        .arg("-p")
        .arg("/usage")
        .arg("--output-format")
        .arg("json")
        // Claude alt süreçlerini PATH'ten arıyor; menüden başlatıldığında dar.
        .env("PATH", paths::augmented_path())
        // Proje dizinine bağlı bir şey sorulmuyor; ev dizini en nötr yer.
        .current_dir(paths::home()?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Other(format!("kullanım sorgulanamadı: {e}")))?;

    // std'de zaman aşımlı bekleme yok; yoklayarak bekliyoruz.
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                return Err(Error::Other("kullanım sorgusu zaman aşımına uğradı".into()));
            }
            None => std::thread::sleep(Duration::from_millis(80)),
        }
    }

    let output = child.wait_with_output()?;
    let envelope: Value = serde_json::from_slice(&output.stdout)?;

    envelope
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Other("kullanım çıktısı beklenen biçimde değil".into()))
}

/// `/usage` metnindeki limit satırlarını ayıklar.
///
/// Beklenen biçim:
/// ```text
/// Current session: 39% used · resets Aug 14, 2pm (Europe/Istanbul)
/// Current week (all models): 12% used
/// ```
/// Sıfır kullanımda "· resets …" kısmı gelmiyor; plana göre fazladan satır
/// (ör. Opus için ayrı bir hafta limiti) olabiliyor, o yüzden satır sayısı
/// varsayılmıyor.
fn parse(text: &str) -> Vec<Window> {
    let mut windows = Vec::new();

    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("Current ") else {
            continue;
        };
        let Some((label, tail)) = rest.split_once(':') else {
            continue;
        };
        let tail = tail.trim();

        let Some((percent, tail)) = tail.split_once("% used") else {
            continue;
        };
        let Ok(percent) = percent.trim().parse::<u8>() else {
            continue;
        };

        // Ayraç `·` (U+00B7); yoksa sıfırlanma bilgisi verilmemiş.
        let resets = tail
            .split_once("resets")
            .map(|(_, when)| when.trim().to_string())
            .filter(|when| !when.is_empty());

        windows.push(Window {
            label: label.trim().to_string(),
            percent: percent.min(100),
            resets,
        });
    }

    windows
}

// ------------------------------------------------------------- önbellek

/// Hesap kısa adı → son ölçüm.
pub type Cache = HashMap<String, Usage>;

fn cache_path() -> Result<std::path::PathBuf> {
    Ok(paths::accounts_root()?.join("usage-cache.json"))
}

/// Önbelleği okur. Bozuk ya da eksik dosya boş önbellek demek — bu veri
/// tamamen yeniden üretilebilir, okuma hatası kullanıcıya taşınmamalı.
pub fn read_cache() -> Cache {
    cache_path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn write_cache(slug: &str, usage: &Usage) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cache = read_cache();
    cache.insert(slug.to_string(), usage.clone());
    std::fs::write(path, serde_json::to_vec_pretty(&cache)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    const REAL: &str = "You are currently using your subscription to power your Claude Code usage

Current session: 39% used · resets Aug 14, 2pm (Europe/Istanbul)
Current week (all models): 12% used · resets Aug 20, 4pm (Europe/Istanbul)

What's contributing to your limits usage?
Approximate, based on local sessions on this machine — does not include other devices or claude.ai.

Last 24h · 599 requests · 9 sessions
  91% of your usage was at >150k context";

    #[test]
    fn gercek_ciktiyi_ayristirir() {
        let windows = parse(REAL);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "session");
        assert_eq!(windows[0].percent, 39);
        assert_eq!(
            windows[0].resets.as_deref(),
            Some("Aug 14, 2pm (Europe/Istanbul)")
        );
        assert_eq!(windows[1].label, "week (all models)");
        assert_eq!(windows[1].percent, 12);
    }

    /// Sıfır kullanımda sıfırlanma bilgisi gelmiyor (ölçüldü).
    #[test]
    fn sifirlanma_bilgisi_olmayabilir() {
        let windows = parse("Current session: 0% used\nCurrent week (all models): 34% used");

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].percent, 0);
        assert!(windows[0].resets.is_none());
        assert_eq!(windows[1].percent, 34);
    }

    /// Plana göre fazladan pencere gelebiliyor; satır sayısı varsayılmamalı.
    #[test]
    fn bilinmeyen_pencereler_de_okunur() {
        let windows = parse("Current week (Opus): 7% used · resets Sep 1, 9am");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "week (Opus)");
        assert_eq!(windows[0].percent, 7);
        assert_eq!(windows[0].resets.as_deref(), Some("Sep 1, 9am"));
    }

    /// Çıktı biçimi değişirse boş dönmeli, çöp değil.
    #[test]
    fn alakasiz_metin_pencere_uretmez() {
        assert!(parse("Current mood: excellent").is_empty());
        assert!(parse("Current session: çok% used").is_empty());
        assert!(parse("").is_empty());
    }
}
