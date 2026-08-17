//! Hesabın kalan kullanım payı.
//!
//! İki kaynak var, sırayla deneniyor.
//!
//! Birincisi `~/.claude.json` içindeki `cachedUsageUtilization`: `/usage`'ın
//! gösterdiği verinin ta kendisi, sunucudan gelen `limits` dizisi olarak.
//! Claude Code onu kendi çalışırken tazeliyor, yani sohbet sürerken uygulamanın
//! hiç süreç başlatmasına gerek kalmıyor.
//!
//! İkincisi `/usage` slash komutu. Yapılandırılmış bir API yok; komut headless
//! modda da çalışıyor ve metin döndürüyor (ölçüldü: ~3 sn, sıfır token — yerel
//! bir komut, API çağrısı yapmıyor). Yalnızca yerel değer yoksa, başka hesaba
//! aitse ya da bayatladığında çalıştırılıyor.
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

/// Yerel önbellek bu yaştan eskiyse komuta düşülüyor.
///
/// Arayüzün yoklama aralığıyla aynı büyüklükte: sohbet sürerken `claude`
/// değeri kendisi tazeliyor, dolayısıyla asıl kullanım senaryosunda süreç hiç
/// başlatılmıyor.
const MAX_LOCAL_AGE: Duration = Duration::from_secs(10 * 60);

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

/// Etkin hesabın kullanımını okur.
///
/// Önce Claude Code'un kendi tuttuğu yerel önbelleğe bakılıyor; orada
/// kullanılabilir bir değer varsa hiç süreç başlatılmıyor. Komut yalnızca
/// önbellek yoksa, başka hesaba aitse ya da penceresi dolmuşsa çalışıyor.
pub fn query() -> Result<Usage> {
    if let Some(usage) = read_local() {
        return Ok(usage);
    }

    let text = run_usage_command()?;

    Ok(Usage {
        windows: parse(&text),
        measured_at_ms: now_ms(),
        detail: text,
    })
}

// --------------------------------------------------- yerel önbellekten okuma

/// Claude Code'un kendi tuttuğu kullanım değeri.
///
/// `~/.claude.json` içindeki `cachedUsageUtilization`, `/usage`'ın gösterdiği
/// verinin kaynağı: sunucudan gelen `limits` dizisi. Claude Code bunu her
/// oturumda tazeliyor ve kullanım ancak bir oturum çalışırken artabildiği için
/// değer, kullanımı artıran her olayda kendiliğinden güncelleniyor.
///
/// Değer yoksa ya da güvenilmezse `None`; çağıran komuta düşüyor.
pub fn read_local() -> Option<Usage> {
    let path = paths::home().ok()?.join(".claude.json");
    let config: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    read_local_from(&config, now_ms())
}

fn read_local_from(config: &Value, now: u64) -> Option<Usage> {
    let cached = config.get("cachedUsageUtilization")?;

    // Önbellek başka bir hesaba ait olabilir: hesap değiştirmek `oauthAccount`'u
    // değiştiriyor ama bu alanı olduğu gibi bırakıyor. Eşleşmiyorsa okunan
    // değer yanlış hesabın payını gösterirdi.
    let owner = cached.get("accountUuid").and_then(Value::as_str)?;
    let active = config
        .get("oauthAccount")?
        .get("accountUuid")
        .and_then(Value::as_str)?;
    if owner != active {
        return None;
    }

    let measured_at_ms = cached.get("fetchedAtMs").and_then(Value::as_u64)?;

    // Claude Code bu değeri her turda değil aralıklarla tazeliyor. Ölçüldü:
    // yoğun kullanımda bir saatlik okuma %75 gösterirken gerçek %96'ydı. Yaşa
    // üst sınır koymak hatayı birkaç puanla sınırlıyor; sohbet sürerken zaten
    // `claude`'un kendisi tazelediği için bu yol neredeyse hep geçerli kalıyor.
    if now.saturating_sub(measured_at_ms) > MAX_LOCAL_AGE.as_millis() as u64 {
        return None;
    }

    let limits = cached.get("utilization")?.get("limits")?.as_array()?;

    let mut windows = Vec::new();
    for limit in limits {
        let (Some(kind), Some(percent)) = (
            limit.get("kind").and_then(Value::as_str),
            limit.get("percent").and_then(Value::as_u64),
        ) else {
            continue;
        };
        let resets = limit.get("resets_at").and_then(Value::as_str);

        // Pencere ölçümden sonra sıfırlandıysa yüzde artık yanlış: gerçekte
        // düşmüş, önbellek hâlâ eskisini gösteriyor. Tek bir pencere bile
        // dolduysa tüm okuma bırakılıyor, komut doğrusunu getirsin.
        if resets.and_then(epoch_ms).is_some_and(|at| at <= now) {
            return None;
        }

        windows.push(Window {
            label: window_label(kind),
            percent: percent.min(100) as u8,
            resets: resets.map(str::to_string),
        });
    }

    if windows.is_empty() {
        return None;
    }

    // Ayrıntı kartı `/usage` metnini gösteriyordu; aynı biçim yeniden kuruluyor
    // ki kaynak değişince kartın görünümü değişmesin.
    let detail = windows
        .iter()
        .map(|w| match &w.resets {
            Some(at) => format!("Current {}: {}% used · resets {at}", w.label, w.percent),
            None => format!("Current {}: {}% used", w.label, w.percent),
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(Usage {
        windows,
        measured_at_ms,
        detail,
    })
}

/// `limits[].kind` → `/usage` metnindeki etiket.
///
/// Arayüz etiketlere göre davranıyor ("week" ile başlayanlar haftalık pay), o
/// yüzden komutun ürettiği adlar birebir korunuyor.
fn window_label(kind: &str) -> String {
    match kind {
        "session" => "session".into(),
        "weekly_all" => "week (all models)".into(),
        "weekly_opus" => "week (Opus)".into(),
        "weekly_sonnet" => "week (Sonnet)".into(),
        other => other.replace('_', " "),
    }
}

/// ISO-8601 damgasını epoch milisaniyeye çevirir.
///
/// Tek kullanım için tarih kütüphanesi eklenmiyor. Beklenen biçim
/// `2026-08-17T04:49:59.914906+00:00`; saniye kesiri atlanıyor, saat dilimi
/// farkı uygulanıyor.
fn epoch_ms(text: &str) -> Option<u64> {
    let field = |from: usize, to: usize| -> Option<i64> { text.get(from..to)?.parse().ok() };

    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);

    // Saat dilimi kesirden sonra geliyor; `Z` ya da hiç yoksa UTC.
    let tail = text.get(19..)?;
    let offset = match tail.rfind(['+', '-']) {
        Some(at) => {
            let sign = if tail.as_bytes()[at] == b'-' { -1 } else { 1 };
            let rest = tail.get(at + 1..)?;
            let hours: i64 = rest.get(0..2)?.parse().ok()?;
            let minutes: i64 = rest.get(3..5).and_then(|m| m.parse().ok()).unwrap_or(0);
            sign * (hours * 3600 + minutes * 60)
        }
        None => 0,
    };

    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

/// Takvim tarihinden 1970-01-01'e göre gün sayısı (Howard Hinnant'ın algoritması).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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

    // Her `claude -p` çağrısı bir transcript bırakıyor. Yoklama beş dakikada
    // bir çalıştığı için oturum listesi kısa sürede bu boş kayıtlarla doluyordu
    // (ölçüldü: bir günde 28 tane). Kendi çöpümüzü topluyoruz.
    if let Some(id) = envelope.get("session_id").and_then(Value::as_str) {
        discard_transcript(id);
    }

    envelope
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Other("kullanım çıktısı beklenen biçimde değil".into()))
}

/// Yoklamanın açtığı oturumun transcript'ini siler.
///
/// Sorgu ev dizininden çalıştırılıyor, yani dosya oraya karşılık gelen proje
/// klasöründe. Silinemezse sessizce geçiliyor: tarama tarafındaki süzgeç bu
/// kayıtları zaten listeye almıyor.
fn discard_transcript(session_id: &str) {
    // Kimlik `claude`'un ürettiği bir UUID; yine de yol ayracı içermediğini
    // doğruluyoruz — dosya adı olarak kullanılıyor.
    if session_id.is_empty() || session_id.contains(['/', '\\', '.']) {
        return;
    }

    let Ok(home) = paths::home() else { return };
    let Ok(projects) = paths::shared_projects_dir() else {
        return;
    };

    let slug = paths::project_slug(&home);
    let _ = std::fs::remove_file(projects.join(slug).join(format!("{session_id}.jsonl")));
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
mod local_tests {
    use super::{days_from_civil, epoch_ms, read_local_from};
    use serde_json::json;

    fn config(owner: &str, active: &str, resets: &str) -> serde_json::Value {
        config_at(owner, active, resets, FETCHED)
    }

    fn config_at(owner: &str, active: &str, resets: &str, fetched: u64) -> serde_json::Value {
        json!({
            "oauthAccount": { "accountUuid": active },
            "cachedUsageUtilization": {
                "accountUuid": owner,
                "fetchedAtMs": fetched,
                "utilization": { "limits": [
                    { "kind": "session", "percent": 75, "resets_at": resets },
                    { "kind": "weekly_all", "percent": 58, "resets_at": null },
                ]}
            }
        })
    }

    const UUID: &str = "653fbee6-ce03-4baa-867c-7e9551045380";
    /// 2026-08-17T04:49:59Z, yani 1 786 942 199 000.
    const RESETS: &str = "2026-08-17T04:49:59.914906+00:00";
    const FETCHED: u64 = 1_786_925_501_993;
    /// Ölçümden ~98 sn sonrası: taze ve sıfırlanma öncesi.
    const NOW: u64 = 1_786_925_600_000;

    #[test]
    fn yerel_deger_pencerelere_cevrilir() {
        let usage = read_local_from(&config(UUID, UUID, RESETS), NOW).unwrap();

        assert_eq!(usage.measured_at_ms, FETCHED);
        assert_eq!(usage.windows.len(), 2);
        // Arayüz "week" önekine göre davranıyor; komutun etiketleri korunmalı.
        assert_eq!(usage.windows[0].label, "session");
        assert_eq!(usage.windows[0].percent, 75);
        assert_eq!(usage.windows[1].label, "week (all models)");
        assert_eq!(usage.windows[1].resets, None);
        assert!(usage.detail.contains("Current session: 75% used · resets"));
    }

    #[test]
    fn baska_hesabin_degeri_kullanilmaz() {
        // Hesap değiştirmek `oauthAccount`'u değiştiriyor ama önbelleği değil.
        assert!(read_local_from(&config(UUID, "baska-hesap", RESETS), NOW).is_none());
    }

    #[test]
    fn eski_deger_reddedilir() {
        // Sınırın hemen altı geçerli, hemen üstü değil.
        let limit = super::MAX_LOCAL_AGE.as_millis() as u64;
        assert!(read_local_from(&config(UUID, UUID, RESETS), FETCHED + limit).is_some());
        assert!(read_local_from(&config(UUID, UUID, RESETS), FETCHED + limit + 1).is_none());
    }

    #[test]
    fn sifirlanmis_pencere_reddedilir() {
        // Sıfırlanma anı geçtiyse yüzde gerçekte düşmüştür; önbellek yanıltır.
        // Ölçüm taze tutuluyor ki reddin sebebi yaş değil sıfırlanma olsun.
        let reset_at = 1_786_942_199_000;
        let config = config_at(UUID, UUID, RESETS, reset_at - 1000);
        assert!(read_local_from(&config, reset_at - 500).is_some());
        assert!(read_local_from(&config, reset_at + 1).is_none());
    }

    #[test]
    fn eksik_alanlar_none_verir() {
        assert!(read_local_from(&json!({}), 0).is_none());
        assert!(read_local_from(&json!({ "cachedUsageUtilization": {} }), 0).is_none());
    }

    #[test]
    fn iso_damgasi_epoch_olur() {
        assert_eq!(epoch_ms("1970-01-01T00:00:00+00:00"), Some(0));
        assert_eq!(epoch_ms(RESETS), Some(1_786_942_199_000));
        // Saat dilimi farkı uygulanmalı: +03:00 üç saat geri gider.
        assert_eq!(
            epoch_ms("2026-08-17T07:49:59.914906+03:00"),
            Some(1_786_942_199_000)
        );
        assert_eq!(epoch_ms("2026-08-17T04:49:59Z"), Some(1_786_942_199_000));
        assert_eq!(epoch_ms("bozuk"), None);
    }

    #[test]
    fn artik_yil_gunleri_dogru() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        // 2024 artık yıl: 29 Şubat gerçek bir gün.
        assert_eq!(days_from_civil(2024, 2, 29) + 1, days_from_civil(2024, 3, 1));
    }
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
