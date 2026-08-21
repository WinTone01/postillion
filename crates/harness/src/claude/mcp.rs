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

pub(crate) fn dirs_home() -> Option<PathBuf> {
    crate::home_dir()
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
    let path = std::env::temp_dir().join(format!("postillion-mcp-{safe}.json"));

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
        let dir = std::env::temp_dir().join(format!("postillion-mcp-defs-{}", std::process::id()));
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

// ── sunucu yönetimi ─────────────────────────────────────────────────────────
//
// Ekleme/silme `claude mcp` alt komutuna devrediliyor, `~/.claude.json`'a elle
// yazılmıyor: dosyanın şeması Claude Code'un kendi malı ve sürümler arasında
// değişebiliyor. CLI'ı çağırmak o sözleşmeyi sahibine bırakıyor.

use std::process::Command;

/// Sunucu adının komut satırında güvenli olduğunu doğrular.
///
/// Ad doğrudan argüman olarak geçiyor. `-` ile başlayan bir ad CLI tarafından
/// BAYRAK olarak okunurdu; kalan karakter kümesi de Claude Code'un kabul
/// ettiğiyle sınırlı tutuluyor ki hata mesajı bizden çıksın, komuttan değil.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("sunucu ismi boş olamaz".into());
    }
    if name.starts_with('-') {
        return Err("sunucu ismi '-' ile başlayamaz".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("sunucu ismi yalnızca harf, rakam, '-', '_' ve '.' içerebilir".into());
    }
    Ok(())
}

/// `claude mcp add` argümanları — HTTP/SSE taşıması. Saf; test edilebilir.
pub fn add_http_args(
    name: &str,
    url: &str,
    transport: &str,
    headers: &[String],
) -> Result<Vec<String>, String> {
    validate_name(name)?;
    if !matches!(transport, "http" | "sse") {
        return Err(format!("geçersiz taşıma: {transport}"));
    }

    let mut args = vec![
        "mcp".to_string(),
        "add".to_string(),
        "--transport".to_string(),
        transport.to_string(),
        name.to_string(),
        url.to_string(),
    ];
    for header in headers {
        args.push("--header".into());
        args.push(header.clone());
    }
    Ok(args)
}

/// `claude mcp add` argümanları — stdio taşıması.
///
/// Komut argümanları `--` sonrasına gidiyor: aksi halde `-v` gibi bir argüman
/// `claude`'un kendi bayrağı sanılırdı.
pub fn add_stdio_args(
    name: &str,
    command: &str,
    command_args: &[String],
    env: &[String],
) -> Result<Vec<String>, String> {
    validate_name(name)?;
    if command.trim().is_empty() {
        return Err("komut boş olamaz".into());
    }

    let mut args = vec!["mcp".to_string(), "add".to_string()];
    for entry in env {
        args.push("--env".into());
        args.push(entry.clone());
    }
    args.push(name.to_string());
    args.push("--".into());
    args.push(command.to_string());
    args.extend(command_args.iter().cloned());
    Ok(args)
}

pub fn remove_args(name: &str) -> Result<Vec<String>, String> {
    validate_name(name)?;
    Ok(vec!["mcp".into(), "remove".into(), name.into()])
}

/// `claude` çalıştırır ve hata durumunda stderr'i taşır.
pub fn run_claude(args: &[String]) -> Result<(), String> {
    let exe = crate::claude::resolve_claude_executable()
        .ok_or_else(|| "claude bulunamadı".to_string())?;
    let output = Command::new(&exe)
        .args(args)
        .output()
        .map_err(|e| format!("{} çalıştırılamadı: {e}", exe.display()))?;

    if output.status.success() {
        return Ok(());
    }
    // CLI'ın kendi mesajı kullanıcıya bizimkinden daha yararlı.
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!("claude mcp başarısız (çıkış {:?})", output.status.code())
    } else {
        stderr
    })
}

#[cfg(test)]
mod manage_tests {
    use super::*;

    #[test]
    fn bayrak_gibi_gorunen_ad_reddediliyor() {
        // Ad doğrudan argüman: `--scope` gibi bir ad CLI'ı yanıltırdı.
        assert!(validate_name("-scope").is_err());
        assert!(validate_name("--transport").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        // Boşluk ve kabuk karakterleri de dışarıda.
        assert!(validate_name("iki kelime").is_err());
        assert!(validate_name("rm;ls").is_err());
        assert!(validate_name("$(whoami)").is_err());

        assert!(validate_name("figbridge").is_ok());
        assert!(validate_name("heroui-pro").is_ok());
        assert!(validate_name("my_server.v2").is_ok());
    }

    #[test]
    fn http_argumanlari_tasima_ve_basliklari_tasiyor() {
        let args = add_http_args(
            "ctx7",
            "https://mcp.example/sse",
            "sse",
            &["Authorization: Bearer x".to_string()],
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--transport",
                "sse",
                "ctx7",
                "https://mcp.example/sse",
                "--header",
                "Authorization: Bearer x",
            ]
        );

        assert!(add_http_args("ctx7", "https://x", "stdio", &[]).is_err());
    }

    #[test]
    fn stdio_argumanlari_komutu_ayiriciyla_veriyor() {
        let args = add_stdio_args(
            "local",
            "npx",
            &["-y".to_string(), "some-server".to_string()],
            &["API_KEY=abc".to_string()],
        )
        .unwrap();
        // `--` olmadan `-y` claude'un kendi bayrağı sanılırdı.
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--env",
                "API_KEY=abc",
                "local",
                "--",
                "npx",
                "-y",
                "some-server",
            ]
        );

        assert!(add_stdio_args("local", "   ", &[], &[]).is_err());
    }

    #[test]
    fn silme_argumanlari_dogrulaniyor() {
        assert_eq!(remove_args("figbridge").unwrap(), vec!["mcp", "remove", "figbridge"]);
        assert!(remove_args("-x").is_err());
    }
}
