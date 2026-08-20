//! Sohbete özel MCP sunucu seçimi.
//!
//! Claude Code MCP sunucularını `~/.claude.json` içinde tutuyor: kullanıcı
//! genelindeki `mcpServers` ve her proje girdisinin kendi `mcpServers`'ı.
//! Varsayılan davranış hepsini açmak, ama her bağlı sunucunun araç tanımları
//! **her turda** bağlamda taşınıyor — yani kullanılmayan bir sunucu her mesajın
//! bedelini artırıyor.
//!
//! Seçim yapıldığında yalnızca seçilenleri içeren geçici bir config yazılıp
//! `--mcp-config` ile veriliyor ve `--strict-mcp-config` diğer kaynakları
//! kapatıyor. `/mcp enable|disable` headless modda çalışmıyor ("MCP controls
//! aren't available right now"), dolayısıyla süreç başlatma anı seçimin
//! uygulanabildiği tek an.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Kullanıcının tanımlı MCP sunucuları: isim → tanım.
///
/// Proje girdileri kullanıcı genelindekilerin üzerine yazıyor; Claude Code'un
/// kendi önceliği bu. Okunamayan ya da bozuk yapılandırma boş sayılıyor —
/// seçim tamamen isteğe bağlı bir özellik ve hata koşuyu engellememeli.
pub fn definitions(config_path: &Path) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();

    let Ok(raw) = std::fs::read(config_path) else {
        return out;
    };
    let Ok(config) = serde_json::from_slice::<Value>(&raw) else {
        return out;
    };

    if let Some(servers) = config.get("mcpServers").and_then(Value::as_object) {
        for (name, def) in servers {
            out.insert(name.clone(), def.clone());
        }
    }

    if let Some(projects) = config.get("projects").and_then(Value::as_object) {
        for entry in projects.values() {
            let Some(servers) = entry.get("mcpServers").and_then(Value::as_object) else {
                continue;
            };
            for (name, def) in servers {
                out.insert(name.clone(), def.clone());
            }
        }
    }

    out
}

/// Varsayılan yapılandırma yolu (`$CLAUDE_CONFIG_DIR` ya da `~/.claude.json`).
pub fn default_config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join(".claude.json"));
    }
    Some(dirs_home()?.join(".claude.json"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Seçilen sunucuları içeren geçici config'i yazar ve yolunu döndürür.
///
/// Tanımı bulunamayan isimler sessizce atlanıyor: kullanıcı bir sunucuyu
/// sildiğinde eski seçimini taşıyan sohbetlerin açılmayı reddetmesi yanlış
/// olurdu.
pub fn write_config(chat_id: &str, selected: &[String], defined: &BTreeMap<String, Value>) -> std::io::Result<PathBuf> {
    let mut servers = serde_json::Map::new();
    for name in selected {
        if let Some(def) = defined.get(name) {
            servers.insert(name.clone(), def.clone());
        }
    }

    let safe: String = chat_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let path = std::env::temp_dir().join(format!("zeron-mcp-{safe}.json"));

    // Tanımlar API anahtarı taşıyabiliyor ve geçici dizin paylaşılan bir yer.
    // `create_new` + 0600: yalnızca bu kullanıcı okuyabilsin ve önceden var
    // olan (belki başkasına ait) bir dosyanın üzerine yazılmasın.
    let _ = std::fs::remove_file(&path);
    let mut file = {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        options.open(&path)?
    };

    file.write_all(&serde_json::to_vec(&json!({ "mcpServers": servers }))?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tanimlar_kullanici_ve_proje_kapsamindan_okunur() {
        let dir = std::env::temp_dir().join(format!("zeron-mcp-defs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "mcpServers": { "genel": { "command": "a" }, "ortak": { "command": "kullanici" } },
                "projects": {
                    "/bir/proje": { "mcpServers": { "projeye-ozel": { "command": "b" } } },
                    // Proje tanımı kullanıcı genelindekinin üzerine yazmalı.
                    "/baska": { "mcpServers": { "ortak": { "command": "proje" } } }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let defs = definitions(&path);
        assert_eq!(defs.len(), 3);
        assert!(defs.contains_key("genel"));
        assert!(defs.contains_key("projeye-ozel"));
        assert_eq!(defs["ortak"]["command"], "proje");

        // Okunamayan ya da bozuk dosya boş; koşu engellenmemeli.
        assert!(definitions(&dir.join("yok.json")).is_empty());
        std::fs::write(&path, b"{bozuk").unwrap();
        assert!(definitions(&path).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn config_yalnizca_secilenleri_icerir() {
        let defined: BTreeMap<String, Value> = [
            ("bir".to_string(), json!({ "command": "1" })),
            ("iki".to_string(), json!({ "command": "2" })),
        ]
        .into_iter()
        .collect();

        // Silinmiş bir sunucunun adı seçimde kalmış olabilir; atlanmalı.
        let path = write_config(
            "chat/1",
            &["bir".into(), "yok-artik".into()],
            &defined,
        )
        .unwrap();

        let written: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"].as_object().unwrap().len(), 1);
        assert_eq!(written["mcpServers"]["bir"]["command"], "1");

        // Boş seçim "hiçbiri": geçerli bir istek, boş sunucu listesi.
        let empty = write_config("chat/1", &[], &defined).unwrap();
        let written: Value = serde_json::from_slice(&std::fs::read(&empty).unwrap()).unwrap();
        assert!(written["mcpServers"].as_object().unwrap().is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            // Tanımlar gizli değer taşıyabiliyor.
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "config 0600 olmalı");
        }

        let _ = std::fs::remove_file(&path);
    }
}
