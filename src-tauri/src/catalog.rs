//! Claude Code'un yapılandırma yüzeyleri: model, efor, MCP, eklenti, skill.
//!
//! Mutasyonlar `claude` alt komutlarına devrediliyor (`claude mcp add`,
//! `claude plugin install` …). Dosya biçimlerini kendimiz yazsaydık Claude'un
//! bir sonraki sürümüyle sessizce uyumsuz kalırdık.
//!
//! Listeleme ise mümkün olduğunca JSON çıktısından okunuyor. `claude mcp list`
//! JSON desteklemediği için MCP sunucuları doğrudan config'den okunuyor.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::paths;

/// `--effort` bayrağının kabul ettiği seviyeler (CLI yardımından doğrulandı).
pub const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    /// `--model` bayrağına geçilecek değer.
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    /// "http", "sse", "stdio"…
    pub transport: String,
    /// http/sse için URL, stdio için komut.
    pub target: String,
    /// Hangi projeye ait; `None` ise kullanıcı genelinde.
    pub scope: Option<String>,
    /// Gizli değer taşıyan alan isimleri. Değerler **asla** taşınmıyor.
    pub secret_fields: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub install_path: Option<String>,
    /// Hangi marketplace'ten geldiği (yalnızca kurulabilir listede).
    #[serde(default)]
    pub marketplace: Option<String>,
    /// Kurulum sayısı — listeyi popülerliğe göre sıralamaya yarıyor.
    #[serde(default)]
    pub install_count: Option<u64>,
    /// Eklentinin getirdiği MCP sunucularının isimleri (değerleri değil).
    #[serde(default, skip_deserializing)]
    pub mcp_server_names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Marketplace {
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub install_location: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    /// "user" (~/.claude/skills) ya da eklenti adı.
    pub source: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

// ------------------------------------------------------------------ yardımcı

/// `claude <args>` çalıştırır ve stdout döndürür.
///
/// `config_dir` `None` ise `CLAUDE_CONFIG_DIR` set edilmez — default hesap için
/// bu şart (bkz. `lib.rs::config_dir_for`).
fn run_claude(config_dir: Option<&Path>, args: &[&str]) -> Result<String> {
    // Mutlak yol; PATH'e güvenmek menüden başlatınca çöküyor.
    let mut cmd = Command::new(paths::claude_bin());
    cmd.env("PATH", paths::augmented_path());
    cmd.args(args);
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }

    let output = cmd
        .output()
        .map_err(|e| Error::Other(format!("claude çalıştırılamadı: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(Error::Other(format!(
            "claude {} başarısız: {}",
            args.join(" "),
            detail.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_claude_json<T: for<'de> Deserialize<'de>>(
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<T> {
    let stdout = run_claude(config_dir, args)?;
    // CLI bazen JSON'dan önce uyarı basıyor; ilk '[' ya da '{'ten itibaren oku.
    let start = stdout
        .find(['[', '{'])
        .ok_or_else(|| Error::Other("JSON çıktısı bulunamadı".into()))?;
    Ok(serde_json::from_str(&stdout[start..])?)
}

/// Hesaba ait `.claude.json`.
fn config_json_for(config_dir: Option<&Path>) -> Result<PathBuf> {
    match config_dir {
        Some(dir) => Ok(dir.join(".claude.json")),
        None => paths::config_json(&paths::default_config_dir()?),
    }
}

/// Hesaba ait `settings.json`.
///
/// Paylaşılan öğelerden biri: ek hesaplarda bu bir symlink, dolayısıyla
/// yazınca default hesabın ayarı da değişir. Kasıtlı — kullanıcı tek bir
/// deneyim istedi.
fn settings_json_for(config_dir: Option<&Path>) -> Result<PathBuf> {
    let dir = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => paths::default_config_dir()?,
    };
    Ok(dir.join("settings.json"))
}

// -------------------------------------------------------------------- model

/// Seçilebilir modeller.
///
/// Takma adlar CLI yardımından; ek seçenekler hesabın önbelleğinden geliyor.
pub fn list_models(config_dir: Option<&Path>) -> Result<Vec<ModelOption>> {
    let mut out = vec![
        ModelOption {
            value: "opus".into(),
            label: "Opus".into(),
            description: Some("En yetenekli; karmaşık işler için".into()),
        },
        ModelOption {
            value: "sonnet".into(),
            label: "Sonnet".into(),
            description: Some("Dengeli hız ve yetenek".into()),
        },
        ModelOption {
            value: "haiku".into(),
            label: "Haiku".into(),
            description: Some("En hızlı; basit işler için".into()),
        },
    ];

    // Hesabın gördüğü ek modeller (ör. Fable) önbellekte duruyor.
    if let Ok(raw) = std::fs::read_to_string(config_json_for(config_dir)?) {
        if let Ok(config) = serde_json::from_str::<Value>(&raw) {
            if let Some(extra) = config.get("additionalModelOptionsCache").and_then(Value::as_array)
            {
                for item in extra {
                    let Some(value) = item.get("value").and_then(Value::as_str) else {
                        continue;
                    };
                    if out.iter().any(|m| m.value == value) {
                        continue;
                    }
                    out.push(ModelOption {
                        value: value.to_string(),
                        label: item
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or(value)
                            .to_string(),
                        description: item
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    });
                }
            }
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------- tercihler

pub fn read_preferences(config_dir: Option<&Path>) -> Result<Preferences> {
    let path = settings_json_for(config_dir)?;
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(Preferences::default());
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// `settings.json`'ı **birleştirerek** günceller.
///
/// Dosyayı baştan yazmak `enabledPlugins`, `extraKnownMarketplaces` gibi
/// bilmediğimiz anahtarları siler.
pub fn write_preferences(config_dir: Option<&Path>, prefs: &Preferences) -> Result<()> {
    let path = settings_json_for(config_dir)?;

    let mut root = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    if let Some(model) = &prefs.model {
        root.insert("model".into(), Value::String(model.clone()));
    }
    if let Some(effort) = &prefs.effort_level {
        if !EFFORT_LEVELS.contains(&effort.as_str()) {
            return Err(Error::Other(format!("geçersiz efor seviyesi: {effort}")));
        }
        root.insert("effortLevel".into(), Value::String(effort.clone()));
    }
    if let Some(theme) = &prefs.theme {
        root.insert("theme".into(), Value::String(theme.clone()));
    }

    let body = serde_json::to_vec_pretty(&Value::Object(root))?;
    // Symlink'i takip ederek yaz; ek hesaplarda bu dosya paylaşılan gerçeğe
    // işaret ediyor ve linki dosyayla değiştirmek paylaşımı bozardı.
    std::fs::write(&path, body)?;
    Ok(())
}

// ---------------------------------------------------------------------- MCP

/// Yapılandırılmış MCP sunucuları.
///
/// `claude mcp list` JSON vermediği için config doğrudan okunuyor.
/// **Gizli değerler taşınmıyor** — yalnızca hangi alanların gizli olduğu.
pub fn list_mcp_servers(config_dir: Option<&Path>) -> Result<Vec<McpServer>> {
    let path = config_json_for(config_dir)?;
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let Ok(config) = serde_json::from_str::<Value>(&raw) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();

    if let Some(servers) = config.get("mcpServers").and_then(Value::as_object) {
        for (name, def) in servers {
            out.push(describe_server(name, def, None));
        }
    }

    if let Some(projects) = config.get("projects").and_then(Value::as_object) {
        for (project, entry) in projects {
            let Some(servers) = entry.get("mcpServers").and_then(Value::as_object) else {
                continue;
            };
            for (name, def) in servers {
                out.push(describe_server(name, def, Some(project.clone())));
            }
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.scope.cmp(&b.scope)));
    Ok(out)
}

fn describe_server(name: &str, def: &Value, scope: Option<String>) -> McpServer {
    let transport = def
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(if def.get("command").is_some() { "stdio" } else { "http" })
        .to_string();

    let target = def
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            def.get("command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();

    // Header ve env değerleri token taşıyor; isimlerini gösterip değerlerini
    // saklıyoruz. Arayüze sızarsa ekran görüntüsüyle birlikte sızar.
    let mut secret_fields = Vec::new();
    for group in ["headers", "env"] {
        if let Some(map) = def.get(group).and_then(Value::as_object) {
            for key in map.keys() {
                secret_fields.push(format!("{group}.{key}"));
            }
        }
    }

    McpServer {
        name: name.to_string(),
        transport,
        target,
        scope,
        secret_fields,
    }
}

/// HTTP/SSE MCP sunucusu ekler.
///
/// `headers` `Ad: Değer` biçiminde; token'lar burada geçiyor ama hiçbir yere
/// kaydedilmiyor — doğrudan `claude mcp add`'e veriliyor.
pub fn mcp_add_http(
    config_dir: Option<&Path>,
    name: &str,
    url: &str,
    transport: &str,
    headers: &[String],
    project_scope: bool,
) -> Result<()> {
    validate_mcp_name(name)?;
    if !matches!(transport, "http" | "sse") {
        return Err(Error::Other(format!("geçersiz transport: {transport}")));
    }

    let mut args: Vec<String> = vec![
        "mcp".into(),
        "add".into(),
        "--transport".into(),
        transport.into(),
    ];
    if project_scope {
        args.push("--scope".into());
        args.push("project".into());
    }
    args.push(name.into());
    args.push(url.into());
    for header in headers {
        args.push("--header".into());
        args.push(header.clone());
    }

    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_claude(config_dir, &refs)?;
    Ok(())
}

/// stdio MCP sunucusu ekler.
pub fn mcp_add_stdio(
    config_dir: Option<&Path>,
    name: &str,
    command: &str,
    command_args: &[String],
    env: &[String],
    project_scope: bool,
) -> Result<()> {
    validate_mcp_name(name)?;

    let mut args: Vec<String> = vec!["mcp".into(), "add".into()];
    if project_scope {
        args.push("--scope".into());
        args.push("project".into());
    }
    for pair in env {
        args.push("-e".into());
        args.push(pair.clone());
    }
    args.push(name.into());
    // `--` sonrası her şey alt sürecin komutu; kullanıcı girdisinin bayrak
    // olarak yorumlanmasını engelliyor.
    args.push("--".into());
    args.push(command.into());
    args.extend(command_args.iter().cloned());

    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_claude(config_dir, &refs)?;
    Ok(())
}

pub fn mcp_remove(config_dir: Option<&Path>, name: &str) -> Result<()> {
    validate_mcp_name(name)?;
    run_claude(config_dir, &["mcp", "remove", name])?;
    Ok(())
}

/// İsim doğrudan komut satırına giriyor; bayrak gibi görünmesini engelle.
fn validate_mcp_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Other("sunucu ismi boş olamaz".into()));
    }
    if name.starts_with('-') {
        return Err(Error::Other("sunucu ismi '-' ile başlayamaz".into()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(Error::Other(
            "sunucu ismi yalnızca harf, rakam, '-', '_' ve '.' içerebilir".into(),
        ));
    }
    Ok(())
}

// ----------------------------------------------------------------- eklenti

pub fn list_plugins(config_dir: Option<&Path>) -> Result<Vec<Plugin>> {
    let raw: Vec<Value> = run_claude_json(config_dir, &["plugin", "list", "--json"])?;
    Ok(raw.iter().map(parse_plugin).collect())
}

/// Marketplace'lerdeki kurulabilir eklentiler.
///
/// Dikkat: `--available` verildiğinde çıktı **dizi değil**, `{installed, available}`
/// zarfı oluyor ve `available` girdileri `id` yerine `pluginId` kullanıyor.
/// Bu makinede 2563 kayıt döndüğü için arayüz aramayla filtreliyor.
pub fn list_available_plugins(config_dir: Option<&Path>) -> Result<Vec<Plugin>> {
    let raw: Value = run_claude_json(config_dir, &["plugin", "list", "--json", "--available"])?;

    let entries = raw
        .get("available")
        .and_then(Value::as_array)
        .cloned()
        // Bayrağın davranışı değişirse düz diziye de tolerans göster.
        .or_else(|| raw.as_array().cloned())
        .unwrap_or_default();

    Ok(entries.iter().map(parse_available_plugin).collect())
}

fn parse_available_plugin(value: &Value) -> Plugin {
    let str_at = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);

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
        // Yalnızca isimler; MCP tanımları token içerebiliyor.
        mcp_server_names: value
            .get("mcpServers")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

pub fn plugin_install(config_dir: Option<&Path>, id: &str) -> Result<()> {
    validate_plugin_id(id)?;
    run_claude(config_dir, &["plugin", "install", id])?;
    Ok(())
}

pub fn plugin_uninstall(config_dir: Option<&Path>, id: &str) -> Result<()> {
    validate_plugin_id(id)?;
    run_claude(config_dir, &["plugin", "uninstall", id])?;
    Ok(())
}

pub fn plugin_set_enabled(config_dir: Option<&Path>, id: &str, enabled: bool) -> Result<()> {
    validate_plugin_id(id)?;
    let action = if enabled { "enable" } else { "disable" };
    run_claude(config_dir, &["plugin", action, id])?;
    Ok(())
}

fn validate_plugin_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.starts_with('-') {
        return Err(Error::Other(format!("geçersiz eklenti kimliği: {id}")));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.@/".contains(c))
    {
        return Err(Error::Other(format!("geçersiz eklenti kimliği: {id}")));
    }
    Ok(())
}

// ------------------------------------------------------------- marketplace

pub fn list_marketplaces(config_dir: Option<&Path>) -> Result<Vec<Marketplace>> {
    run_claude_json(config_dir, &["plugin", "marketplace", "list", "--json"])
}

pub fn marketplace_add(config_dir: Option<&Path>, source: &str) -> Result<()> {
    if source.trim().is_empty() || source.starts_with('-') {
        return Err(Error::Other("geçersiz marketplace kaynağı".into()));
    }
    run_claude(config_dir, &["plugin", "marketplace", "add", source])?;
    Ok(())
}

pub fn marketplace_remove(config_dir: Option<&Path>, name: &str) -> Result<()> {
    if name.trim().is_empty() || name.starts_with('-') {
        return Err(Error::Other("geçersiz marketplace ismi".into()));
    }
    run_claude(config_dir, &["plugin", "marketplace", "remove", name])?;
    Ok(())
}

pub fn marketplace_update(config_dir: Option<&Path>, name: Option<&str>) -> Result<()> {
    match name {
        Some(name) if !name.trim().is_empty() && !name.starts_with('-') => {
            run_claude(config_dir, &["plugin", "marketplace", "update", name])?;
        }
        Some(_) => return Err(Error::Other("geçersiz marketplace ismi".into())),
        None => {
            run_claude(config_dir, &["plugin", "marketplace", "update"])?;
        }
    }
    Ok(())
}

/// `~/.claude/skills/<isim>/` altında yeni bir skill iskeleti oluşturur.
///
/// `claude plugin init` kullanılıyor; dosya biçimini kendimiz yazsaydık
/// Claude'un sürümüyle uyumsuz kalırdık.
pub fn skill_create(
    config_dir: Option<&Path>,
    name: &str,
    description: Option<&str>,
) -> Result<()> {
    validate_skill_name(name)?;

    let mut args: Vec<String> = vec!["plugin".into(), "init".into()];
    if let Some(text) = description {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            args.push("--description".into());
            args.push(trimmed.to_string());
        }
    }
    args.push("--with".into());
    args.push("skills".into());
    args.push(name.into());

    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_claude(config_dir, &refs)?;
    Ok(())
}

/// Skill dizinini siler.
///
/// Yalnızca hesabın `skills/` dizini altındaki yollar kabul ediliyor; başka bir
/// yer verilirse reddediliyor.
pub fn skill_delete(config_dir: Option<&Path>, name: &str) -> Result<()> {
    validate_skill_name(name)?;

    let base = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => paths::default_config_dir()?,
    }
    .join("skills");

    let target = base.join(name);
    // İsim doğrulandı ama silme yıkıcı; yolun gerçekten içeride kaldığını da
    // kanonik hâli üzerinden teyit ediyoruz.
    let canonical = target
        .canonicalize()
        .map_err(|_| Error::Other(format!("skill bulunamadı: {name}")))?;
    if !canonical.starts_with(base.canonicalize()?) {
        return Err(Error::Other("skill yolu dizin dışında".into()));
    }

    std::fs::remove_dir_all(&canonical)?;
    Ok(())
}

fn validate_skill_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.starts_with('.') {
        return Err(Error::Other(format!("geçersiz skill ismi: {name}")));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Other(
            "skill ismi yalnızca harf, rakam, '-' ve '_' içerebilir".into(),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ skill

/// Kullanıcı skill'leri ve eklentilerin getirdikleri.
///
/// Skill'lerin kendi CLI listeleme komutu yok; dizin taranıyor.
pub fn list_skills(config_dir: Option<&Path>) -> Result<Vec<Skill>> {
    let base = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => paths::default_config_dir()?,
    };

    let mut out = Vec::new();
    collect_skills(&base.join("skills"), "user", &mut out);

    // Eklenti skill'leri kurulum dizinlerinde duruyor.
    if let Ok(plugins) = list_plugins(config_dir) {
        for plugin in plugins {
            let Some(path) = plugin.install_path else {
                continue;
            };
            collect_skills(&PathBuf::from(path).join("skills"), &plugin.id, &mut out);
        }
    }

    out.sort_by(|a, b| a.source.cmp(&b.source).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn collect_skills(dir: &Path, source: &str, out: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("SKILL.md");
        if !manifest.is_file() {
            continue;
        }

        out.push(Skill {
            name: entry.file_name().to_string_lossy().into_owned(),
            description: read_skill_description(&manifest),
            source: source.to_string(),
            path: path.display().to_string(),
        });
    }
}

/// SKILL.md frontmatter'ından `description` satırını çeker.
fn read_skill_description(manifest: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    let mut lines = raw.lines();

    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut fields = BTreeMap::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    fields.get("description").map(|d| {
        let cleaned = d.trim_matches(['"', '\'']).to_string();
        if cleaned.chars().count() > 200 {
            format!("{}…", cleaned.chars().take(200).collect::<String>())
        } else {
            cleaned
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayrak_gibi_gorunen_isimler_reddedilir() {
        for bad in ["--scope", "-e", "", "  ", "a b", "a;rm -rf /"] {
            assert!(validate_mcp_name(bad).is_err(), "kabul edildi: {bad:?}");
        }
        assert!(validate_mcp_name("heroui-pro").is_ok());
        assert!(validate_mcp_name("paddle_sandbox.v2").is_ok());
    }

    #[test]
    fn eklenti_kimligi_dogrulanir() {
        assert!(validate_plugin_id("paddle@claude-community").is_ok());
        assert!(validate_plugin_id("-rf").is_err());
        assert!(validate_plugin_id("a b").is_err());
    }

    /// Token'lar arayüze taşınmamalı; yalnızca alan isimleri.
    #[test]
    fn gizli_degerler_tasinmaz() {
        let def = serde_json::json!({
            "type": "http",
            "url": "https://mcp.heroui.pro/mcp",
            "headers": { "x-heroui-personal-token": "COK-GIZLI-DEGER" }
        });

        let server = describe_server("heroui-pro", &def, None);
        let encoded = serde_json::to_string(&server).unwrap();

        assert!(
            !encoded.contains("COK-GIZLI-DEGER"),
            "token serileştirmeye sızdı: {encoded}"
        );
        assert_eq!(server.secret_fields, vec!["headers.x-heroui-personal-token"]);
        assert_eq!(server.transport, "http");
    }

    /// `--available` çıktısı dizi değil `{installed, available}` zarfı;
    /// bu regresyon "invalid type: map, expected a sequence" hatasını verdi.
    #[test]
    fn kurulabilir_eklenti_zarfi_ayristirilir() {
        let raw: Value = serde_json::json!({
            "installed": [{ "id": "paddle@claude-community" }],
            "available": [
                {
                    "pluginId": "adobe-for-creativity@claude-plugins-official",
                    "name": "adobe-for-creativity",
                    "description": "Adobe araçları",
                    "marketplaceName": "claude-plugins-official",
                    "installCount": 42
                }
            ]
        });

        let entries = raw
            .get("available")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| raw.as_array().cloned())
            .unwrap_or_default();

        let plugins: Vec<Plugin> = entries.iter().map(parse_available_plugin).collect();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "adobe-for-creativity@claude-plugins-official");
        assert_eq!(plugins[0].marketplace.as_deref(), Some("claude-plugins-official"));
        assert_eq!(plugins[0].install_count, Some(42));
    }

    #[test]
    fn skill_ismi_dogrulanir() {
        assert!(validate_skill_name("benim-skillim").is_ok());
        for bad in ["", "-rf", "../escape", ".gizli", "a b", "a/b"] {
            assert!(validate_skill_name(bad).is_err(), "kabul edildi: {bad:?}");
        }
    }

    #[test]
    fn efor_seviyesi_dogrulanir() {
        assert!(EFFORT_LEVELS.contains(&"medium"));
        assert!(EFFORT_LEVELS.contains(&"max"));
        assert!(!EFFORT_LEVELS.contains(&"ultra"));
    }
}
