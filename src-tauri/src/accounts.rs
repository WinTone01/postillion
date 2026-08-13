//! Hesap yaşam döngüsü: listeleme, oluşturma, tohumlama, silme.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::paths::{self, IDENTITY_KEYS, SHARED_ENTRIES};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub dir: PathBuf,
    /// `~/.claude` — silinemez, paylaşılan verinin kaynağı.
    pub is_default: bool,
    pub logged_in: bool,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub organization_role: Option<String>,
    pub seat_tier: Option<String>,
    pub billing_type: Option<String>,
    /// Paylaşılan öğelerden symlink'i eksik/kırık olanlar. Boş değilse
    /// hesap izole kalmış demektir; frontend uyarı gösterir.
    pub broken_links: Vec<String>,
}

/// `.claude.json` üzerinde Claude'un kendi kilit protokolü.
///
/// Deneyle doğrulandı: Claude `.claude.json.lock` adında bir *dizin* oluşturuyor.
/// `mkdir` POSIX'te atomiktir, yani bu bir mutex. Aynı protokolü kullanmazsak
/// Claude çalışırken yazdığımızda 47 KB'lik ayar dosyasını kaybettirebiliriz.
struct ConfigLock {
    path: PathBuf,
}

impl ConfigLock {
    fn acquire(config_json: &Path) -> Result<Self> {
        let path = with_suffix(config_json, ".lock");

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

        // create_dir atomiktir: zaten varsa hata verir, bu da mutex'in ta kendisi.
        match fs::create_dir(&path) {
            Ok(()) => Ok(Self { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(Error::LockBusy(
                config_json.display().to_string(),
            )),
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// Tüm hesaplar: önce default (`~/.claude`), sonra `~/.claude-accounts/*`.
pub fn list() -> Result<Vec<Account>> {
    let mut out = vec![read_account("default", paths::default_config_dir()?, true)];

    let root = paths::accounts_root()?;
    if root.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&root)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if paths::validate_name(&name).is_err() {
                continue; // elle bırakılmış tuhaf dizinleri yok say
            }
            out.push(read_account(&name, entry.path(), false));
        }
    }

    Ok(out)
}

fn read_account(name: &str, dir: PathBuf, is_default: bool) -> Account {
    // Default hesapta bu ~/.claude.json, diğerlerinde <dir>/.claude.json.
    let config = paths::config_json(&dir).unwrap_or_else(|_| dir.join(".claude.json"));
    let oauth = fs::read_to_string(&config)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.get("oauthAccount").cloned());

    let field = |key: &str| -> Option<String> {
        oauth
            .as_ref()?
            .get(key)?
            .as_str()
            .map(|s| s.to_string())
    };

    // Giriş yapılmış sayılması için hem token dosyası hem hesap bloğu gerekli.
    let has_creds = dir.join(".credentials.json").exists();

    Account {
        name: name.to_string(),
        is_default,
        logged_in: has_creds && oauth.is_some(),
        email: field("emailAddress"),
        display_name: field("displayName"),
        organization_role: field("organizationRole"),
        seat_tier: field("seatTier"),
        billing_type: field("billingType"),
        broken_links: if is_default {
            Vec::new() // default zaten kaynak, symlink'i yok
        } else {
            broken_links(&dir)
        },
        dir,
    }
}

/// Paylaşılan öğelerden hedefi çözülemeyenleri döndürür.
fn broken_links(dir: &Path) -> Vec<String> {
    SHARED_ENTRIES
        .iter()
        .filter(|entry| {
            let link = dir.join(entry);
            // symlink_metadata link'in kendisini görür; metadata hedefi izler.
            // Link var ama hedef yoksa (kırık), ikincisi başarısız olur.
            match fs::symlink_metadata(&link) {
                Ok(_) => fs::metadata(&link).is_err(),
                Err(_) => {
                    // Hiç yok. Kaynakta da yoksa sorun değil.
                    paths::default_config_dir()
                        .map(|d| d.join(entry).exists())
                        .unwrap_or(false)
                }
            }
        })
        .map(|s| s.to_string())
        .collect()
}

/// Yeni hesap dizini oluşturur, paylaşılan öğeleri symlink'ler ve
/// `.claude.json`'ı kimlik alanları çıkarılmış olarak tohumlar.
///
/// Tohumlama önemli: aksi halde yeni hesapta her proje için trust dialog'unu
/// yeniden onaylamanız ve MCP sunucularını yeniden eklemeniz gerekir.
pub fn create(name: &str) -> Result<Account> {
    let name = paths::validate_name(name)?.to_string();
    let dir = paths::account_dir(&name)?;

    if dir.exists() {
        return Err(Error::AccountExists(name));
    }

    let source = paths::default_config_dir()?;
    fs::create_dir_all(&dir)?;

    // Oluşturmanın ortasında hata alırsak yarım dizin bırakmayalım.
    if let Err(e) = populate(&dir, &source) {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }

    Ok(read_account(&name, dir, false))
}

fn populate(dir: &Path, source: &Path) -> Result<()> {
    for entry in SHARED_ENTRIES {
        let target = source.join(entry);
        if !target.exists() {
            continue; // kaynakta yoksa symlink'lemeye çalışma
        }
        symlink(&target, dir.join(entry))?;
    }

    seed_config(dir, source)?;
    Ok(())
}

/// `~/.claude.json`'ı kopyalar ama kimliği taşımaz.
///
/// Korunan en değerli anahtar `projects`: içinde her proje için
/// `hasTrustDialogAccepted`, `allowedTools` ve `mcpServers` var.
fn seed_config(dir: &Path, source_dir: &Path) -> Result<()> {
    let dest = dir.join(".claude.json");
    let source = paths::config_json(source_dir)?;

    let mut seeded = Map::new();

    if let Ok(raw) = fs::read_to_string(&source) {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) {
            for (key, value) in map {
                if IDENTITY_KEYS.contains(&key.as_str()) {
                    continue;
                }
                seeded.insert(key, value);
            }
        }
    }

    // Yeni hesap onboarding'i tekrar görmesin.
    seeded.insert("hasCompletedOnboarding".into(), Value::Bool(true));

    let _lock = ConfigLock::acquire(&dest)?;
    write_private(&dest, &serde_json::to_vec_pretty(&Value::Object(seeded))?)?;
    Ok(())
}

/// 0600 izinle yazar — dosya proje ayarlarını içeriyor, dünyaya açık olmamalı.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;

    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}

/// Kırık ya da eksik symlink'leri yeniden kurar.
pub fn repair(name: &str) -> Result<Account> {
    let name = paths::validate_name(name)?.to_string();
    let dir = paths::account_dir(&name)?;
    if !dir.is_dir() {
        return Err(Error::AccountNotFound(name));
    }

    let source = paths::default_config_dir()?;
    for entry in SHARED_ENTRIES {
        let link = dir.join(entry);
        let target = source.join(entry);

        if !target.exists() {
            continue;
        }
        // Sadece symlink'leri kaldır. Gerçek dosya/dizinse dokunma —
        // kullanıcının verisi olabilir.
        if let Ok(meta) = fs::symlink_metadata(&link) {
            if meta.file_type().is_symlink() {
                fs::remove_file(&link)?;
            } else {
                continue;
            }
        }
        symlink(&target, &link)?;
    }

    Ok(read_account(&name, dir, false))
}

/// Hesabı siler. Symlink'ler önce tek tek kaldırılır, böylece paylaşılan
/// verinin silinme ihtimali sıfırlanır.
pub fn delete(name: &str) -> Result<()> {
    let name = paths::validate_name(name)?.to_string();
    let dir = paths::account_dir(&name)?;

    if !dir.is_dir() {
        return Err(Error::AccountNotFound(name));
    }
    // validate_name zaten "default"ı reddediyor ama silme yıkıcı bir işlem,
    // ikinci bir savunma katmanı bırakıyorum.
    if dir == paths::default_config_dir()? {
        return Err(Error::CannotDeleteDefault);
    }

    // remove_dir_all symlink'leri izlemez, ama 412 MB transcript söz konusu
    // olduğu için buna güvenmek yerine linkleri açıkça kaldırıyorum.
    for entry in SHARED_ENTRIES {
        let link = dir.join(entry);
        if let Ok(meta) = fs::symlink_metadata(&link) {
            if meta.file_type().is_symlink() {
                fs::remove_file(&link)?;
            }
        }
    }

    fs::remove_dir_all(&dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimlik_anahtarlari_tohumlanmaz() {
        let raw = r#"{
            "oauthAccount": {"emailAddress": "a@b.com"},
            "userID": "abc",
            "machineID": "xyz",
            "projects": {"/tmp/p": {"hasTrustDialogAccepted": true}},
            "migrationVersion": 13
        }"#;
        let map: Map<String, Value> = serde_json::from_str(raw).unwrap();

        let seeded: Map<String, Value> = map
            .into_iter()
            .filter(|(k, _)| !IDENTITY_KEYS.contains(&k.as_str()))
            .collect();

        assert!(!seeded.contains_key("oauthAccount"));
        assert!(!seeded.contains_key("userID"));
        assert!(!seeded.contains_key("machineID"));
        // Asıl mesele: proje onayları korunmalı.
        assert!(seeded.contains_key("projects"));
        assert!(seeded.contains_key("migrationVersion"));
    }

    #[test]
    fn kilit_ikinci_kez_alinamaz() {
        let tmp = std::env::temp_dir().join(format!("po-lock-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let cfg = tmp.join(".claude.json");

        let first = ConfigLock::acquire(&cfg).unwrap();
        assert!(ConfigLock::acquire(&cfg).is_err(), "kilit iki kez alındı");

        drop(first);
        // Bırakıldıktan sonra tekrar alınabilmeli.
        assert!(ConfigLock::acquire(&cfg).is_ok());

        let _ = fs::remove_dir_all(&tmp);
    }
}
