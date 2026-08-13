//! Disk yerleşimi.
//!
//! Tasarımın temeli: hiçbir veri taşınmaz. Mevcut `~/.claude` "default" hesap
//! olarak yerinde kalır ve paylaşılan verinin *kaynağıdır*. Ek hesaplar
//! `~/.claude-accounts/<isim>/` altında yaşar ve paylaşılan öğeleri
//! `~/.claude`'a symlink'ler.
//!
//! Böylece düz `claude` komutu hiç değişmeden çalışmaya devam eder.

use std::path::PathBuf;

use crate::error::{Error, Result};

/// Hesaplar arasında paylaşılan öğeler. Her biri `~/.claude` içindeki
/// gerçeğe symlink olur.
///
/// Kasıtlı olarak dışarıda bırakılanlar: `.claude.json` ve `.credentials.json`.
/// Hesabı hesap yapan tek şey bu ikisi.
pub const SHARED_ENTRIES: &[&str] = &[
    "projects",      // transcript'ler - projenin tüm amacı
    "plugins",
    "skills",
    "settings.json",
    "history.jsonl",
];

/// Yeni bir hesaba tohumlanırken `.claude.json` içinden atılan anahtarlar.
///
/// Bunlar ya kimliğin kendisi ya da hesaba özgü sunucu cevaplarının cache'i.
/// Taşınırlarsa yeni hesap kendini eski hesap sanır.
pub const IDENTITY_KEYS: &[&str] = &[
    "oauthAccount",
    "userID",
    "machineID",
    "claudeCodeFirstTokenDate",
    "modelAccessCache",
    "orgModelDefaultCache",
    "passesEligibilityCache",
    "groveConfigCache",
    "cachedExtraUsageDisabledReason",
    "clientDataCacheSlots",
    "additionalModelOptionsCache",
    "additionalModelCostsCache",
];

pub fn home() -> Result<PathBuf> {
    dirs::home_dir().ok_or(Error::NoHome)
}

/// Default hesap = mevcut `~/.claude`. Paylaşılan verinin kaynağı.
pub fn default_config_dir() -> Result<PathBuf> {
    Ok(home()?.join(".claude"))
}

/// Ek hesapların kökü. `POSTILLION_ROOT` ile geçersiz kılınabilir (test için).
pub fn accounts_root() -> Result<PathBuf> {
    if let Some(custom) = std::env::var_os("POSTILLION_ROOT") {
        return Ok(PathBuf::from(custom));
    }
    Ok(home()?.join(".claude-accounts"))
}

pub fn account_dir(name: &str) -> Result<PathBuf> {
    Ok(accounts_root()?.join(validate_name(name)?))
}

/// Transcript'lerin bulunduğu tek gerçek dizin. Her hesap buraya symlink'ler,
/// dolayısıyla oturum taraması her zaman burayı okur.
pub fn shared_projects_dir() -> Result<PathBuf> {
    Ok(default_config_dir()?.join("projects"))
}

/// Bir config dizini için `.claude.json`'ın gerçek yeri.
///
/// Burada bir asimetri var ve gözden kaçması kolay: default kurulumda
/// `.credentials.json` `~/.claude/` içindeyken `.claude.json` ev kökünde,
/// yani `~/.claude.json`. Ama `CLAUDE_CONFIG_DIR` set edildiğinde ikisi de
/// o dizinin içine düşer (deneyle doğrulandı).
pub fn config_json(dir: &std::path::Path) -> Result<PathBuf> {
    if dir == default_config_dir()? {
        Ok(home()?.join(".claude.json"))
    } else {
        Ok(dir.join(".claude.json"))
    }
}

/// Hesap ismini doğrular.
///
/// Bu sadece kozmetik değil: isim doğrudan dosya yoluna giriyor, yani
/// `../` içeren bir isim `accounts_root()` dışına yazmamıza yol açardı.
pub fn validate_name(name: &str) -> Result<&str> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(Error::InvalidName("isim boş olamaz".into()));
    }
    if trimmed == "default" {
        return Err(Error::InvalidName(
            "'default' ismi mevcut ~/.claude hesabına ayrılmış".into(),
        ));
    }
    if trimmed.starts_with('.') {
        return Err(Error::InvalidName("isim '.' ile başlayamaz".into()));
    }
    if trimmed.len() > 64 {
        return Err(Error::InvalidName("isim 64 karakteri aşamaz".into()));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidName(
            "isim yalnızca harf, rakam, '-' ve '_' içerebilir".into(),
        ));
    }

    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_traversal_reddedilir() {
        for bad in ["..", "../escape", "a/b", "a\0b", ".gizli", ""] {
            assert!(validate_name(bad).is_err(), "kabul edilmemeliydi: {bad:?}");
        }
    }

    #[test]
    fn default_ismi_korunur() {
        assert!(validate_name("default").is_err());
    }

    #[test]
    fn normal_isimler_gecer() {
        for ok in ["is", "kisisel", "musteri-a", "hesap_2"] {
            assert_eq!(validate_name(ok).unwrap(), ok);
        }
    }
}
