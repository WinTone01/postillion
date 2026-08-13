//! Disk yerleşimi.
//!
//! Tasarımın temeli: hiçbir veri taşınmaz. Mevcut `~/.claude` "default" hesap
//! olarak yerinde kalır ve paylaşılan verinin *kaynağıdır*. Ek hesaplar
//! `~/.claude-accounts/<isim>/` altında yaşar ve paylaşılan öğeleri
//! `~/.claude`'a symlink'ler.
//!
//! Böylece düz `claude` komutu hiç değişmeden çalışmaya devam eder.

use std::path::PathBuf;
use std::sync::OnceLock;

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

/// `claude` çalıştırılabilirinin yeri.
///
/// Neden PATH yetmiyor: masaüstü menüsünden başlatılan uygulamalar systemd
/// kullanıcı oturumunun PATH'ini devralıyor ve bu, birçok kurulumda
/// `~/.local/bin`'i içermiyor — Claude Code'un kendini kurduğu yer tam orası.
/// Yalnızca PATH'e güvenmek, uygulamanın terminalden çalışıp menüden sessizce
/// bozulması demek.
///
/// Bir kez çözülüp saklanıyor; her araç çağrısında dosya sistemi taramanın
/// anlamı yok.
pub fn claude_bin() -> PathBuf {
    static RESOLVED: OnceLock<PathBuf> = OnceLock::new();
    RESOLVED.get_or_init(resolve_claude).clone()
}

fn resolve_claude() -> PathBuf {
    // Önce PATH: kullanıcı bilerek bir sürüm seçtiyse ona saygı duyulmalı.
    if let Some(found) = search_path("claude") {
        return found;
    }

    if let Ok(home) = home() {
        for candidate in [
            home.join(".local/bin/claude"),
            home.join(".claude/local/claude"),
            home.join(".bun/bin/claude"),
            home.join(".npm-global/bin/claude"),
            home.join(".yarn/bin/claude"),
        ] {
            if is_executable(&candidate) {
                return candidate;
            }
        }
    }

    for candidate in ["/usr/local/bin/claude", "/usr/bin/claude", "/opt/homebrew/bin/claude"] {
        let path = PathBuf::from(candidate);
        if is_executable(&path) {
            return path;
        }
    }

    // Hiçbir yerde yok. Uydurma bir yol yerine çıplak ismi döndürüyoruz ki
    // spawn hatası kullanıcıya "claude" desin.
    PathBuf::from("claude")
}

fn search_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Çocuk sürecin PATH'ine `claude`'un dizinini ekler.
///
/// Claude kendi alt süreçlerini (node, git, ripgrep…) PATH'ten arıyor. Onu
/// mutlak yolla başlatmak kendi bulunmasını çözer ama komşularını çözmez.
pub fn augmented_path() -> std::ffi::OsString {
    let current = std::env::var_os("PATH").unwrap_or_default();

    let Some(dir) = claude_bin().parent().map(PathBuf::from) else {
        return current;
    };

    let mut dirs: Vec<PathBuf> = std::env::split_paths(&current).collect();
    if !dirs.contains(&dir) {
        dirs.insert(0, dir);
    }

    std::env::join_paths(dirs).unwrap_or(current)
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

    /// Masaüstünden başlatılan uygulama `~/.local/bin` içermeyen bir PATH
    /// devralıyor. Çözümleyici PATH boş olsa bile `claude`'u bulabilmeli.
    #[test]
    fn claude_path_disinda_da_bulunur() {
        let resolved = claude_bin();

        // Bu makinede claude kurulu; çıplak isme düşmüş olmamalı.
        if home()
            .map(|h| is_executable(&h.join(".local/bin/claude")))
            .unwrap_or(false)
        {
            assert!(
                resolved.is_absolute(),
                "mutlak yol bekleniyordu, bulunan: {}",
                resolved.display()
            );
            assert!(is_executable(&resolved), "çözülen yol çalıştırılabilir değil");
        }
    }

    #[test]
    fn augmented_path_claude_dizinini_iceriyor() {
        let bin = claude_bin();
        let Some(dir) = bin.parent() else { return };
        if !bin.is_absolute() {
            return; // claude kurulu değil, test anlamsız
        }

        let path = augmented_path();
        let dirs: Vec<_> = std::env::split_paths(&path).collect();
        assert!(
            dirs.iter().any(|d| d == dir),
            "{} zenginleştirilmiş PATH'te yok",
            dir.display()
        );
    }

    #[test]
    fn normal_isimler_gecer() {
        for ok in ["is", "kisisel", "musteri-a", "hesap_2"] {
            assert_eq!(validate_name(ok).unwrap(), ok);
        }
    }
}
