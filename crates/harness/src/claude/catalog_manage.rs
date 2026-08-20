//! Claude Code'un eklenti, marketplace ve skill kataloğu.
//!
//! Hepsi `claude` alt komutlarına devrediliyor, yapılandırma dosyaları elle
//! kurcalanmıyor: bu şemalar Claude Code'un kendi malı ve sürümler arasında
//! değişiyor. CLI'ı çağırmak sözleşmeyi sahibine bırakıyor.
//!
//! Kimlikler doğrudan komut satırına giriyor, o yüzden hepsi çağrılmadan önce
//! doğrulanıyor — bayrak gibi görünen bir kimlik komutu yanıltırdı.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::mcp::run_claude;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: String,
    pub version: Option<String>,
    /// Hangi kapsamda kurulu; kurulabilir listede yok.
    pub scope: Option<String>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
    pub install_path: Option<String>,
    /// Yalnızca kurulabilir listede dolu.
    pub marketplace: Option<String>,
    pub install_count: Option<u64>,
    /// Eklentinin getirdiği MCP sunucularının **isimleri**. Tanımlar kasıtlı
    /// olarak taşınmıyor: jeton içerebiliyorlar.
    #[serde(default)]
    pub mcp_server_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marketplace {
    pub name: String,
    pub source: Option<String>,
    pub url: Option<String>,
    pub install_location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    /// "user" ya da eklenti kimliği.
    pub source: String,
    pub path: String,
}

/// Kimlik komut satırında güvenli mi.
///
/// Eklenti kimlikleri `ad@marketplace` biçiminde; skill adları sade. İkisi de
/// `-` ile başlayamaz, yoksa CLI onları bayrak sanar.
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("kimlik boş olamaz".into());
    }
    if id.starts_with('-') {
        return Err("kimlik '-' ile başlayamaz".into());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/'))
    {
        return Err("kimlik yalnızca harf, rakam ve '-_.@/' içerebilir".into());
    }
    Ok(())
}

/// `claude … --json` çalıştırıp çıktıyı ayrıştırır.
fn run_json(args: &[&str]) -> Result<Value, String> {
    let exe = super::resolve_claude_executable().ok_or_else(|| "claude bulunamadı".to_string())?;
    let output = std::process::Command::new(&exe)
        .args(args)
        .output()
        .map_err(|e| format!("{} çalıştırılamadı: {e}", exe.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("claude {} başarısız", args.join(" "))
        } else {
            stderr
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("çıktı ayrıştırılamadı: {e}"))
}

pub fn list_plugins() -> Result<Vec<Plugin>, String> {
    let raw = run_json(&["plugin", "list", "--json"])?;
    Ok(parse_installed(&raw))
}

/// Marketplace'lerdeki kurulabilir eklentiler.
pub fn list_available_plugins() -> Result<Vec<Plugin>, String> {
    let raw = run_json(&["plugin", "list", "--json", "--available"])?;
    Ok(parse_available(&raw))
}

/// Kurulu eklenti listesi. Saf.
pub fn parse_installed(raw: &Value) -> Vec<Plugin> {
    raw.as_array()
        .map(|entries| entries.iter().map(parse_plugin).collect())
        .unwrap_or_default()
}

/// Kurulabilir eklenti listesi. Saf.
///
/// `--available` verildiğinde çıktı **dizi değil**, `{installed, available}`
/// zarfı oluyor ve `available` girdileri `id` yerine `pluginId` kullanıyor.
/// Düz diziye de tolerans var: bayrağın davranışı değişirse liste boşalmasın.
pub fn parse_available(raw: &Value) -> Vec<Plugin> {
    let entries = raw
        .get("available")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| raw.as_array().cloned())
        .unwrap_or_default();

    entries
        .iter()
        .map(|value| {
            let str_at =
                |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
            Plugin {
                // `available` tarafında kimlik alanının adı farklı.
                id: str_at("pluginId").or_else(|| str_at("id")).unwrap_or_default(),
                version: str_at("version"),
                scope: None,
                enabled: None,
                description: str_at("description"),
                install_path: None,
                marketplace: str_at("marketplaceName"),
                install_count: value.get("installCount").and_then(Value::as_u64),
                mcp_server_names: Vec::new(),
            }
        })
        .collect()
}

fn parse_plugin(value: &Value) -> Plugin {
    let str_at = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    Plugin {
        id: str_at("id").unwrap_or_default(),
        version: str_at("version"),
        scope: str_at("scope"),
        enabled: value.get("enabled").and_then(Value::as_bool),
        description: str_at("description"),
        install_path: str_at("installPath"),
        marketplace: str_at("marketplaceName"),
        install_count: value.get("installCount").and_then(Value::as_u64),
        mcp_server_names: value
            .get("mcpServers")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

pub fn plugin_install(id: &str) -> Result<(), String> {
    validate_id(id)?;
    run_claude(&["plugin".into(), "install".into(), id.into()])
}

pub fn plugin_uninstall(id: &str) -> Result<(), String> {
    validate_id(id)?;
    run_claude(&["plugin".into(), "uninstall".into(), id.into()])
}

pub fn plugin_set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    validate_id(id)?;
    let verb = if enabled { "enable" } else { "disable" };
    run_claude(&["plugin".into(), verb.into(), id.into()])
}

pub fn list_marketplaces() -> Result<Vec<Marketplace>, String> {
    let raw = run_json(&["plugin", "marketplace", "list", "--json"])?;
    Ok(parse_marketplaces(&raw))
}

/// Marketplace listesi. Saf.
pub fn parse_marketplaces(raw: &Value) -> Vec<Marketplace> {
    raw.as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|value| {
                    let str_at =
                        |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
                    Marketplace {
                        name: str_at("name").unwrap_or_default(),
                        source: str_at("source").or_else(|| {
                            // Kaynak bir nesne olarak da gelebiliyor.
                            value
                                .get("source")
                                .and_then(|s| s.get("url").or_else(|| s.get("source")))
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        }),
                        url: str_at("url"),
                        install_location: str_at("installLocation"),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Marketplace ekler. Kaynak bir git URL'si ya da yerel yol olabiliyor, o
/// yüzden [`validate_id`]'den daha geniş: yalnızca bayrak gibi görünmesi
/// engelleniyor.
pub fn marketplace_add(source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("kaynak boş olamaz".into());
    }
    if source.starts_with('-') {
        return Err("kaynak '-' ile başlayamaz".into());
    }
    run_claude(&[
        "plugin".into(),
        "marketplace".into(),
        "add".into(),
        source.into(),
    ])
}

pub fn marketplace_remove(name: &str) -> Result<(), String> {
    validate_id(name)?;
    run_claude(&[
        "plugin".into(),
        "marketplace".into(),
        "remove".into(),
        name.into(),
    ])
}

pub fn list_skills() -> Result<Vec<Skill>, String> {
    let raw = run_json(&["skill", "list", "--json"])?;
    Ok(parse_skills(&raw))
}

/// Skill listesi. Saf.
pub fn parse_skills(raw: &Value) -> Vec<Skill> {
    raw.as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|value| {
                    let str_at =
                        |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
                    Skill {
                        name: str_at("name").unwrap_or_default(),
                        description: str_at("description"),
                        source: str_at("source").unwrap_or_else(|| "user".into()),
                        path: str_at("path").unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn skill_delete(name: &str) -> Result<(), String> {
    validate_id(name)?;
    run_claude(&["skill".into(), "delete".into(), name.into()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bayrak_gibi_gorunen_kimlik_reddediliyor() {
        assert!(validate_id("-force").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_id("rm -rf /").is_err());
        assert!(validate_id("$(id)").is_err());

        // Eklenti kimlikleri `ad@marketplace`; skill adları sade.
        assert!(validate_id("context7@claude-plugins-official").is_ok());
        assert!(validate_id("my-skill").is_ok());
        assert!(validate_id("scope/name").is_ok());
    }

    #[test]
    fn kurulabilir_liste_zarftan_okunuyor() {
        // `--available` dizi DEĞİL, zarf döndürüyor ve kimlik alanı farklı.
        let raw = json!({
            "installed": [],
            "available": [
                { "pluginId": "ctx7@official", "version": "1.0", "installCount": 42 },
            ]
        });
        let plugins = parse_available(&raw);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "ctx7@official");
        assert_eq!(plugins[0].install_count, Some(42));

        // Bayrak davranışı düz diziye dönerse de çalışmalı.
        let flat = json!([{ "id": "a@b" }]);
        assert_eq!(parse_available(&flat)[0].id, "a@b");

        // Beklenmedik biçim listeyi boşaltır, panik etmez.
        assert!(parse_available(&json!({})).is_empty());
    }

    #[test]
    fn kurulu_liste_mcp_isimlerini_tasiyor_tanimlari_degil() {
        let raw = json!([{
            "id": "ctx7@official",
            "enabled": true,
            "scope": "user",
            "mcpServers": { "context7": { "command": "npx", "env": { "KEY": "gizli" } } }
        }]);
        let plugins = parse_installed(&raw);
        assert_eq!(plugins[0].mcp_server_names, vec!["context7"]);
        assert_eq!(plugins[0].enabled, Some(true));
        // Tanım jeton taşıyabiliyor; hiçbir alanda görünmemeli.
        let encoded = serde_json::to_string(&plugins[0]).unwrap();
        assert!(!encoded.contains("gizli"), "gizli değer sızdı: {encoded}");
    }

    #[test]
    fn marketplace_kaynagi_duz_ya_da_nesne_olabiliyor() {
        let raw = json!([
            { "name": "a", "source": "https://github.com/x/y.git" },
            { "name": "b", "source": { "url": "https://github.com/p/q.git" } },
        ]);
        let list = parse_marketplaces(&raw);
        assert_eq!(list[0].source.as_deref(), Some("https://github.com/x/y.git"));
        assert_eq!(list[1].source.as_deref(), Some("https://github.com/p/q.git"));
    }

    #[test]
    fn skill_kaynagi_yoksa_kullanici_sayiliyor() {
        let raw = json!([{ "name": "dataviz", "path": "/x/dataviz" }]);
        let skills = parse_skills(&raw);
        assert_eq!(skills[0].source, "user");
        assert_eq!(skills[0].name, "dataviz");
    }

    #[test]
    fn marketplace_kaynagi_bayrak_olamaz() {
        assert!(marketplace_add("--force").is_err());
        assert!(marketplace_add("  ").is_err());
    }
}
