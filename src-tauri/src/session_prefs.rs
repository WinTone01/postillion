//! Oturum başına kalıcı tercihler.
//!
//! Şimdilik tek şey var: sohbetin kullandığı MCP sunucuları. Seçim yalnızca
//! sekme açıkken bellekte tutuluyordu, dolayısıyla uygulama kapanıp açılınca
//! ya da oturum listeden yeniden açılınca unutuluyordu — kullanıcı her
//! seferinde baştan seçmek zorunda kalıyordu.
//!
//! Anahtar `sessionId`; transcript'in kendisiyle aynı kimlik, yani seçim
//! sohbetle birlikte yaşıyor.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPrefs {
    /// Seçilen MCP sunucuları. `None` genel yapılandırma demek.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<String>>,
}

pub type Store = HashMap<String, SessionPrefs>;

fn store_path() -> Result<std::path::PathBuf> {
    Ok(paths::accounts_root()?.join("session-prefs.json"))
}

/// Tüm tercihler. Bozuk ya da eksik dosya boş kabul ediliyor: bu veri
/// tamamen isteğe bağlı ve okuma hatası kullanıcıya taşınmamalı.
pub fn read() -> Store {
    store_path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Bir oturumun MCP seçimini yazar.
///
/// `None` "genel yapılandırma" demek ve varsayılan olduğu için kaydı tamamen
/// siliyor — dosya kullanıcının gerçekten seçim yaptığı oturumlarla sınırlı
/// kalıyor.
pub fn set_mcp(session_id: &str, servers: Option<Vec<String>>) -> Result<()> {
    let path = store_path()?;
    let mut store = read();

    match servers {
        Some(list) => {
            store.entry(session_id.to_string()).or_default().mcp_servers = Some(list);
        }
        None => {
            store.remove(session_id);
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Yazma yarıda kalırsa bir sonraki açılış bozuk JSON okumasın.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&store)?)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `POSTILLION_ROOT` testler arasında paylaşıldığı için tek test içinde
    /// tüm akış deneniyor; ayrı testler birbirinin dosyasını ezerdi.
    #[test]
    fn secim_yazilip_geri_okunur() {
        let root = std::env::temp_dir().join(format!("po-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // SAFETY: tek iş parçacıklı test; başka test bu değişkeni okumuyor.
        unsafe { std::env::set_var("POSTILLION_ROOT", &root) };

        assert!(read().is_empty(), "boş başlamalı");

        set_mcp("abc", Some(vec!["figbridge".into()])).unwrap();
        let store = read();
        assert_eq!(
            store.get("abc").and_then(|p| p.mcp_servers.clone()),
            Some(vec!["figbridge".into()])
        );

        // Boş liste "hiçbiri" demek ve varsayılandan farklı; korunmalı.
        set_mcp("abc", Some(vec![])).unwrap();
        assert_eq!(
            read().get("abc").and_then(|p| p.mcp_servers.clone()),
            Some(vec![])
        );

        // `None` varsayılana dönüş: kayıt tamamen kalkıyor.
        set_mcp("abc", None).unwrap();
        assert!(!read().contains_key("abc"));

        unsafe { std::env::remove_var("POSTILLION_ROOT") };
        std::fs::remove_dir_all(&root).unwrap();
    }
}
