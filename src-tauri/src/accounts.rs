//! Sistem geneli hesap değiştirme.
//!
//! Model: tek bir gerçek yapılandırma var — `~/.claude` — ve `claude` komutu
//! her zaman onu kullanıyor. Hesaplar yapılandırmanın kopyaları değil, yalnızca
//! saklanmış **kimlikler**. Hesap değiştirmek, aktif kimliği profiline geri
//! yazıp hedefin kimliğini yerine koymak demek.
//!
//! Böylece terminalden çalıştırılan düz `claude` de seçili hesabı kullanıyor.
//! Oturumlar, projeler, eklentiler ve ayarlar tek kopya olarak paylaşılıyor —
//! zaten hiç taşınmıyorlar.
//!
//! ```text
//! ~/.claude/.credentials.json   ← aktif jeton
//! ~/.claude.json                ← aktif kimlik (oauthAccount, userID)
//!
//! ~/.claude-accounts/<slug>/
//!   .credentials.json           ← saklanmış jeton
//!   identity.json               ← saklanmış kimlik
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::paths::{self, IDENTITY_KEYS};

const CREDENTIALS: &str = ".credentials.json";
const IDENTITY: &str = "identity.json";

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Dizin adı; komutlarda kimlik olarak bu kullanılıyor.
    pub slug: String,
    /// Arayüzde görünen ad — e-postanın kullanıcı kısmı ya da görünen ad.
    pub label: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub organization_role: Option<String>,
    pub seat_tier: Option<String>,
    /// Şu anda sistem genelinde etkin olan hesap bu mu.
    pub is_active: bool,
    /// Jetonu saklanmış mı; değilse geçiş yapılamaz.
    pub has_credentials: bool,
}

// ------------------------------------------------------------------- okuma

/// Aktif yapılandırmadaki kimlik bloğu.
fn active_identity() -> Result<Option<Value>> {
    let path = paths::config_json(&paths::default_config_dir()?)?;
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(config) = serde_json::from_str::<Value>(&raw) else {
        return Ok(None);
    };
    Ok(config.get("oauthAccount").cloned())
}

/// Kimliğin sahibi olan hesabın slug'ı.
///
/// E-posta üzerinden türetiliyor: aynı hesap her zaman aynı profile düşsün.
fn slug_for(identity: &Value) -> Option<String> {
    let email = identity.get("emailAddress").and_then(Value::as_str)?;
    Some(slugify(email))
}

/// `ozan.kaya@ornek.com` → `ozan-kaya`
pub fn slugify(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    let cleaned: String = local
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "hesap".into()
    } else {
        trimmed
    }
}

fn label_for(identity: &Value) -> String {
    let display = identity
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(name) = display {
        return name.to_string();
    }

    identity
        .get("emailAddress")
        .and_then(Value::as_str)
        .map(|e| e.split('@').next().unwrap_or(e).to_string())
        .unwrap_or_else(|| "hesap".into())
}

fn read_identity(dir: &Path) -> Option<Value> {
    let raw = fs::read_to_string(dir.join(IDENTITY)).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

/// Tüm hesaplar. Aktif olan da listede; profili yoksa oradan üretiliyor.
pub fn list() -> Result<Vec<Account>> {
    // Aktif kimliği her listelemede profiline yazıyoruz: jeton yenilendiğinde
    // saklanan kopya bayatlamasın.
    let _ = capture_active();

    let root = paths::accounts_root()?;
    let active_slug = active_identity()?.as_ref().and_then(slug_for);

    let mut out = Vec::new();

    if root.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&root)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let slug = entry.file_name().to_string_lossy().into_owned();
            let dir = entry.path();

            let identity = read_identity(&dir);
            let field = |key: &str| -> Option<String> {
                identity
                    .as_ref()?
                    .get(key)?
                    .as_str()
                    .map(str::to_string)
            };

            out.push(Account {
                label: identity.as_ref().map(label_for).unwrap_or_else(|| slug.clone()),
                email: field("emailAddress"),
                display_name: field("displayName"),
                organization_role: field("organizationRole"),
                seat_tier: field("seatTier"),
                is_active: active_slug.as_deref() == Some(slug.as_str()),
                has_credentials: dir.join(CREDENTIALS).exists(),
                slug,
            });
        }
    }

    Ok(out)
}

// ------------------------------------------------------------------ yazma

/// Aktif kimliği ve jetonu kendi profiline kaydeder.
///
/// Geçişten önce çağrılıyor: aksi halde o hesabın yenilenmiş jetonu kaybolur
/// ve geri dönüldüğünde tekrar giriş gerekir.
pub fn capture_active() -> Result<Option<String>> {
    let Some(identity) = active_identity()? else {
        return Ok(None);
    };
    let Some(slug) = slug_for(&identity) else {
        return Ok(None);
    };

    let dir = paths::accounts_root()?.join(&slug);
    fs::create_dir_all(&dir)?;

    // Kimlik: oauthAccount + hesaba bağlı diğer alanlar.
    let config_path = paths::config_json(&paths::default_config_dir()?)?;
    let config: Value = fs::read_to_string(&config_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Value::Null);

    let mut stored = Map::new();
    for key in IDENTITY_KEYS {
        if let Some(value) = config.get(*key) {
            stored.insert((*key).to_string(), value.clone());
        }
    }
    // Etiket ve e-posta bu blokta; ayrıca düzleştirip saklıyoruz ki listeleme
    // tek dosya okumakla yetinsin.
    if let Some(obj) = identity.as_object() {
        for key in ["emailAddress", "displayName", "organizationRole", "seatTier"] {
            if let Some(value) = obj.get(key) {
                stored.insert(key.to_string(), value.clone());
            }
        }
    }

    write_private(&dir.join(IDENTITY), &serde_json::to_vec_pretty(&Value::Object(stored))?)?;

    // Jeton.
    let creds = paths::default_config_dir()?.join(CREDENTIALS);
    if creds.exists() {
        fs::copy(&creds, dir.join(CREDENTIALS))?;
        set_private(&dir.join(CREDENTIALS))?;
    }

    Ok(Some(slug))
}

/// Sistem genelinde etkin hesabı değiştirir.
pub fn switch(slug: &str) -> Result<Account> {
    let slug = validate_slug(slug)?;
    let dir = paths::accounts_root()?.join(slug);

    if !dir.is_dir() {
        return Err(Error::AccountNotFound(slug.to_string()));
    }
    if !dir.join(CREDENTIALS).exists() {
        return Err(Error::Other(format!(
            "{slug} için saklanmış oturum yok — önce giriş yapın"
        )));
    }

    // Çalışan bir Claude süreci varken takas etmek tehlikeli: süreç eski
    // jetonu bellekte tutuyor ama aynı yapılandırmaya yazmaya devam ediyor.
    if let Some(count) = running_claude_processes() {
        if count > 0 {
            return Err(Error::Other(format!(
                "{count} Claude süreci çalışıyor — hesap değiştirmeden önce kapatın"
            )));
        }
    }

    // Mevcut hesabın jetonu kaybolmasın.
    capture_active()?;

    let target_creds = dir.join(CREDENTIALS);
    let live_creds = paths::default_config_dir()?.join(CREDENTIALS);

    // Kimliği yapılandırmaya yaz. Kilit protokolü şart: Claude aynı dosyaya
    // yazıyor ve `.claude.json.lock` dizini onun mutex'i.
    let config_path = paths::config_json(&paths::default_config_dir()?)?;
    let stored = read_identity(&dir).unwrap_or(Value::Null);

    {
        let _lock = ConfigLock::acquire(&config_path)?;

        let mut config: Map<String, Value> = fs::read_to_string(&config_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

        // Önce eski kimliğin izlerini temizle, sonra yenisini koy. Birleştirmek
        // yetmez: hedefte olmayan bir anahtar eskisinden kalırdı.
        for key in IDENTITY_KEYS {
            config.remove(*key);
        }
        if let Some(obj) = stored.as_object() {
            for (key, value) in obj {
                if IDENTITY_KEYS.contains(&key.as_str()) {
                    config.insert(key.clone(), value.clone());
                }
            }
        }

        write_private(&config_path, &serde_json::to_vec_pretty(&Value::Object(config))?)?;
    }

    // Jetonu yerine koy.
    fs::copy(&target_creds, &live_creds)?;
    set_private(&live_creds)?;

    list()?
        .into_iter()
        .find(|a| a.slug == slug)
        .ok_or_else(|| Error::AccountNotFound(slug.to_string()))
}

/// Saklanmış hesabı siler. Aktif hesap silinemez.
pub fn remove(slug: &str) -> Result<()> {
    let slug = validate_slug(slug)?;
    let active = active_identity()?.as_ref().and_then(slug_for);

    if active.as_deref() == Some(slug) {
        return Err(Error::Other(
            "etkin hesap silinemez — önce başka bir hesaba geçin".into(),
        ));
    }

    let dir = paths::accounts_root()?.join(slug);
    if !dir.is_dir() {
        return Err(Error::AccountNotFound(slug.to_string()));
    }

    fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Geçici bir yapılandırma dizinindeki oturumu profil olarak saklar.
///
/// Giriş akışı bittikten sonra çağrılıyor.
pub fn adopt(temp_dir: &Path) -> Result<Account> {
    let config: Value = fs::read_to_string(temp_dir.join(".claude.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| Error::Other("giriş sonrası yapılandırma okunamadı".into()))?;

    let identity = config
        .get("oauthAccount")
        .cloned()
        .ok_or_else(|| Error::Other("giriş tamamlanmamış görünüyor".into()))?;

    let slug = slug_for(&identity)
        .ok_or_else(|| Error::Other("hesabın e-postası okunamadı".into()))?;

    let dir = paths::accounts_root()?.join(&slug);
    fs::create_dir_all(&dir)?;

    let mut stored = Map::new();
    for key in IDENTITY_KEYS {
        if let Some(value) = config.get(*key) {
            stored.insert((*key).to_string(), value.clone());
        }
    }
    if let Some(obj) = identity.as_object() {
        for key in ["emailAddress", "displayName", "organizationRole", "seatTier"] {
            if let Some(value) = obj.get(key) {
                stored.insert(key.to_string(), value.clone());
            }
        }
    }
    write_private(&dir.join(IDENTITY), &serde_json::to_vec_pretty(&Value::Object(stored))?)?;

    let creds = temp_dir.join(CREDENTIALS);
    if !creds.exists() {
        return Err(Error::Other("giriş jetonu oluşmadı".into()));
    }
    fs::copy(&creds, dir.join(CREDENTIALS))?;
    set_private(&dir.join(CREDENTIALS))?;

    let label = label_for(&identity);
    let field = |key: &str| identity.get(key).and_then(Value::as_str).map(str::to_string);

    Ok(Account {
        slug,
        label,
        email: field("emailAddress"),
        display_name: field("displayName"),
        organization_role: field("organizationRole"),
        seat_tier: field("seatTier"),
        is_active: false,
        has_credentials: true,
    })
}

// --------------------------------------------------------------- yardımcı

/// Çalışan `claude` süreçlerini sayar; `/proc` okunamazsa `None`.
fn running_claude_processes() -> Option<usize> {
    let target = paths::claude_bin();
    let entries = fs::read_dir("/proc").ok()?;

    let mut count = 0;
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let Some(pid) = name.to_str() else { continue };
        if !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Kendi süreçlerimizi de sayıyoruz; onlar da aynı yapılandırmayı
        // kullanıyor ve takas onları da bozar.
        if let Ok(exe) = fs::read_link(entry.path().join("exe")) {
            if exe == target {
                count += 1;
            }
        }
    }
    Some(count)
}

fn validate_slug(slug: &str) -> Result<&str> {
    let trimmed = slug.trim();
    if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.starts_with('-') {
        return Err(Error::InvalidName(format!("geçersiz hesap: {slug}")));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidName(format!("geçersiz hesap: {slug}")));
    }
    Ok(trimmed)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn set_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// `.claude.json` üzerinde Claude'un kendi kilit protokolü.
///
/// Deneyle doğrulandı: Claude `.claude.json.lock` adında bir *dizin* oluşturuyor.
/// `mkdir` POSIX'te atomik, yani bu bir mutex.
struct ConfigLock {
    path: PathBuf,
}

impl ConfigLock {
    fn acquire(config_json: &Path) -> Result<Self> {
        let mut raw = config_json.as_os_str().to_os_string();
        raw.push(".lock");
        let path = PathBuf::from(raw);

        // Bayat kilit: 60 saniyeden eskiyse sahibi ölmüş kabul et.
        if let Ok(meta) = fs::metadata(&path) {
            let stale = meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .map(|age| age.as_secs() > 60)
                .unwrap_or(false);
            if stale {
                let _ = fs::remove_dir(&path);
            }
        }

        match fs::create_dir(&path) {
            Ok(()) => Ok(Self { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Error::LockBusy(config_json.display().to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_epostadan_turetilir() {
        assert_eq!(slugify("ozan@ornek.com"), "ozan");
        assert_eq!(slugify("ozan.kaya@ornek.com"), "ozan-kaya");
        assert_eq!(slugify("WinTone01@gmail.com"), "wintone01");
        assert_eq!(slugify("mert_b+etiket@x.io"), "mert-b-etiket");
        // Tamamen ayraçtan oluşan yerel kısım boş slug üretmemeli.
        assert_eq!(slugify("...@x.io"), "hesap");
    }

    #[test]
    fn etiket_gorunen_ada_oncelik_verir() {
        let with_name = serde_json::json!({
            "displayName": "Ozan Kaya",
            "emailAddress": "ozan@ornek.com"
        });
        assert_eq!(label_for(&with_name), "Ozan Kaya");

        let email_only = serde_json::json!({ "emailAddress": "mert@ornek.com" });
        assert_eq!(label_for(&email_only), "mert");
    }

    #[test]
    fn gecersiz_slug_reddedilir() {
        for bad in ["", "..", "../kacis", ".gizli", "-rf", "a/b"] {
            assert!(validate_slug(bad).is_err(), "kabul edildi: {bad:?}");
        }
        assert!(validate_slug("ozan-kaya").is_ok());
    }

    /// Takas eski kimliğin izlerini bırakmamalı; birleştirme yetmez.
    #[test]
    fn eski_kimlik_anahtarlari_temizlenir() {
        let mut config: Map<String, Value> = serde_json::from_value(serde_json::json!({
            "oauthAccount": { "emailAddress": "eski@x.com" },
            "userID": "eski-id",
            "modelAccessCache": ["eski"],
            "projects": { "/tmp/p": { "hasTrustDialogAccepted": true } }
        }))
        .unwrap();

        let stored = serde_json::json!({
            "oauthAccount": { "emailAddress": "yeni@x.com" },
            "userID": "yeni-id"
        });

        for key in IDENTITY_KEYS {
            config.remove(*key);
        }
        for (key, value) in stored.as_object().unwrap() {
            if IDENTITY_KEYS.contains(&key.as_str()) {
                config.insert(key.clone(), value.clone());
            }
        }

        assert_eq!(config["oauthAccount"]["emailAddress"], "yeni@x.com");
        assert_eq!(config["userID"], "yeni-id");
        // Hedefte olmayan eski cache kalmamalı.
        assert!(!config.contains_key("modelAccessCache"));
        // Paylaşılan veri korunmalı.
        assert!(config.contains_key("projects"));
    }
}
