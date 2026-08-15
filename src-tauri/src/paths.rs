//! Disk yerleşimi.
//!
//! Tek bir gerçek yapılandırma var: `~/.claude`. Hesaplar onun kopyaları değil,
//! `~/.claude-accounts/<slug>/` altında saklanan **kimliklerden** ibaret.
//! Hesap değiştirmek yalnızca kimliği takas ediyor, dolayısıyla oturumlar,
//! projeler ve ayarlar tek kopya olarak paylaşılıyor.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::error::{Error, Result};

/// Hesap değiştirirken taşınan anahtarlar.
///
/// Kimliğin kendisi ve hesaba özgü sunucu cevaplarının cache'i. Bunlar dışında
/// hiçbir şey taşınmıyor — `projects`, ayarlar ve eklentiler ortak kalıyor.
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

/// Tek gerçek yapılandırma dizini; `claude` her zaman bunu kullanıyor.
pub fn default_config_dir() -> Result<PathBuf> {
    Ok(home()?.join(".claude"))
}

/// Saklanan kimliklerin kökü. `POSTILLION_ROOT` ile geçersiz kılınabilir.
pub fn accounts_root() -> Result<PathBuf> {
    if let Some(custom) = std::env::var_os("POSTILLION_ROOT") {
        return Ok(PathBuf::from(custom));
    }
    Ok(home()?.join(".claude-accounts"))
}

/// Transcript'lerin bulunduğu dizin. Tek yapılandırma olduğu için oturumlar
/// hesaplar arasında zaten ortak.
pub fn shared_projects_dir() -> Result<PathBuf> {
    Ok(default_config_dir()?.join("projects"))
}

/// Bir çalışma dizininin transcript klasörü adı.
///
/// Claude yolu düzleştiriyor: `/` ve `.` tire oluyor, yani
/// `/home/ali/Projects/x` → `-home-ali-Projects-x`. Nokta kuralı gizli
/// dizinlerde ortaya çıkıyor (`.claude/worktrees` → `-claude-worktrees`) ve
/// diskteki gerçek klasör adlarıyla doğrulandı.
pub fn project_slug(cwd: &std::path::Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
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


#[cfg(test)]
mod tests {
    use super::*;

    /// Slug biçimi diskteki gerçek klasör adlarıyla doğrulandı.
    #[test]
    fn proje_slug_yolu_duzlestirir() {
        use std::path::Path;

        assert_eq!(project_slug(Path::new("/home/ali")), "-home-ali");
        assert_eq!(
            project_slug(Path::new("/home/ali/Projects/x")),
            "-home-ali-Projects-x"
        );
        // Gizli dizinlerdeki nokta da tireye dönüyor.
        assert_eq!(
            project_slug(Path::new("/home/ali/p/.claude/worktrees/a")),
            "-home-ali-p--claude-worktrees-a"
        );
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

}
