mod accounts;
mod agent;
mod catalog;
mod error;
mod paths;
mod sessions;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager as _, State};

use accounts::Account;
use error::{Error, Result};
use sessions::Session;

struct AppState {
    agent: agent::SharedManager,
    sessions: Arc<sessions::Cache>,
}

// ---------------------------------------------------------------- hesaplar

#[tauri::command]
fn list_accounts() -> Result<Vec<Account>> {
    accounts::list()
}

#[tauri::command]
fn create_account(name: String) -> Result<Account> {
    accounts::create(&name)
}

#[tauri::command]
fn repair_account(name: String) -> Result<Account> {
    accounts::repair(&name)
}

#[tauri::command]
fn delete_account(name: String) -> Result<()> {
    accounts::delete(&name)
}

/// Webview günlüklerini sürecin stderr'ine aktarır.
///
/// Webview konsoluna ulaşmanın tek pratik yolu devtools penceresi; bu köprü
/// sayesinde frontend hataları doğrudan `npm run tauri dev` çıktısında görünür.
#[tauri::command]
fn log_frontend(level: String, message: String) {
    eprintln!("[frontend/{level}] {message}");
}

// --------------------------------------------------------------- oturumlar

#[tauri::command]
fn list_sessions(state: State<'_, AppState>, project: Option<String>) -> Result<Vec<Session>> {
    state.sessions.scan(project.as_deref())
}

#[tauri::command]
fn refresh_sessions(state: State<'_, AppState>) -> Result<Vec<Session>> {
    state.sessions.clear();
    state.sessions.scan(None)
}

/// Bir oturumun geçmişini diskten okur.
///
/// `claude --resume` geçmişi stdout'a basmadığı için arayüzü buradan
/// dolduruyoruz. Yol, taramanın döndürdüğü `Session.path` olmalı.
#[tauri::command]
fn read_transcript(path: String, max_records: Option<usize>) -> Result<Vec<serde_json::Value>> {
    let path = PathBuf::from(path);

    // Yalnızca paylaşılan transcript dizini okunabilir; rastgele dosya değil.
    let root = paths::shared_projects_dir()?;
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root.canonicalize()?) {
        return Err(Error::Other(
            "transcript yolu proje dizininin dışında".into(),
        ));
    }

    sessions::read_transcript(&canonical, max_records.unwrap_or(600))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FileSnapshot {
    exists: bool,
    /// Dosya metin değilse ya da çok büyükse `None`.
    content: Option<String>,
    truncated: bool,
    size_bytes: u64,
}

/// Bir dosyanın mevcut içeriğini okur.
///
/// Amaç: onay bekleyen bir `Write` çağrısında diskteki hâl henüz "önceki"
/// sürüm olduğu için gerçek diff üretilebiliyor. Tamamlanmış yazımlarda bu
/// bilgi geri getirilemez — dosya çoktan üzerine yazılmıştır.
#[tauri::command]
fn read_text_file(path: String) -> Result<FileSnapshot> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;

    let path = PathBuf::from(path);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Ok(FileSnapshot {
            exists: false,
            content: None,
            truncated: false,
            size_bytes: 0,
        });
    };

    if !meta.is_file() {
        return Ok(FileSnapshot {
            exists: false,
            content: None,
            truncated: false,
            size_bytes: 0,
        });
    }

    let size = meta.len();
    if size > MAX_BYTES {
        // Diff görünümü zaten bu boyutta kullanışsız; belleği şişirmeyelim.
        return Ok(FileSnapshot {
            exists: true,
            content: None,
            truncated: true,
            size_bytes: size,
        });
    }

    // İkili dosyalarda hata vermek yerine içeriksiz dönüyoruz.
    let content = std::fs::read(&path).ok().and_then(|b| String::from_utf8(b).ok());

    Ok(FileSnapshot {
        exists: true,
        truncated: false,
        size_bytes: size,
        content,
    })
}

// ---------------------------------------------------------------- terminal

/// İsimden `CLAUDE_CONFIG_DIR` değerini çözer.
///
/// Default hesap için kasıtlı olarak `None` döner ve değişken **hiç set
/// edilmez**. Sebebi ince ama kritik: `CLAUDE_CONFIG_DIR` set edildiğinde
/// Claude `.claude.json`'ı o dizinin içinde arıyor. Default kurulumda ise
/// gerçek dosya ev kökünde (`~/.claude.json`). Dolayısıyla
/// `CLAUDE_CONFIG_DIR=~/.claude` vermek, Claude'u `~/.claude/.claude.json`
/// diye bomboş ikinci bir config'le başlatır — girişsiz, proje onayları yok.
fn config_dir_for(account: &str) -> Result<Option<PathBuf>> {
    if account == "default" {
        return Ok(None);
    }
    let dir = paths::account_dir(account)?;
    if !dir.is_dir() {
        return Err(Error::AccountNotFound(account.to_string()));
    }
    Ok(Some(dir))
}

#[tauri::command]
fn agent_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    account: String,
    cwd: Option<String>,
    resume: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<String> {
    let config_dir = config_dir_for(&account)?;
    let cwd = cwd.map(PathBuf::from);

    if let Some(level) = effort.as_deref() {
        if !catalog::EFFORT_LEVELS.contains(&level) {
            return Err(Error::Other(format!("geçersiz efor seviyesi: {level}")));
        }
    }

    state.agent.start(
        app,
        agent::StartOptions {
            id,
            config_dir: config_dir.as_deref(),
            cwd: cwd.as_deref(),
            resume: resume.as_deref(),
            model: model.as_deref(),
            effort: effort.as_deref(),
        },
    )
}

/// Modeli süren oturumda değiştirir; bağlam korunuyor.
#[tauri::command]
fn agent_set_model(state: State<'_, AppState>, id: String, model: String) -> Result<()> {
    state.agent.set_model(&id, &model)
}

#[tauri::command]
fn agent_send(state: State<'_, AppState>, id: String, text: String) -> Result<()> {
    state.agent.send_user_message(&id, &text)
}

#[tauri::command]
fn agent_respond_permission(
    state: State<'_, AppState>,
    id: String,
    request_id: String,
    allow: bool,
    updated_input: Option<serde_json::Value>,
    message: Option<String>,
) -> Result<()> {
    state
        .agent
        .respond_permission(&id, &request_id, allow, updated_input, message.as_deref())
}

#[tauri::command]
fn agent_set_permission_mode(state: State<'_, AppState>, id: String, mode: String) -> Result<()> {
    state.agent.set_permission_mode(&id, &mode)
}

#[tauri::command]
fn agent_interrupt(state: State<'_, AppState>, id: String) -> Result<()> {
    state.agent.interrupt(&id)
}

#[tauri::command]
fn agent_stop(state: State<'_, AppState>, id: String) -> Result<()> {
    state.agent.stop(&id)
}

#[tauri::command]
fn agent_active(state: State<'_, AppState>) -> Vec<String> {
    state.agent.active_ids()
}

/// `claude auth login`'i hesabın dizininde çalıştırır.
///
/// OAuth akışını Claude'un kendisi yürütür ve token'ı hesabın dizinine yazar;
/// bizim kodumuz gizli bilgiye hiç dokunmaz.
#[tauri::command]
fn account_login(account: String) -> Result<()> {
    let config_dir = config_dir_for(&account)?;

    let mut cmd = std::process::Command::new("claude");
    cmd.arg("auth").arg("login");
    if let Some(dir) = config_dir.as_deref() {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    cmd.spawn()
        .map_err(|e| Error::Other(format!("giriş akışı başlatılamadı: {e}")))?;

    Ok(())
}

/// Entegrasyon testlerinin çekirdeğe erişimi.
///
/// Tauri komutları `State` istediği için testlerden doğrudan çağrılamıyor;
/// bu ince katman aynı fonksiyonları çıplak haliyle açar.
pub mod testing {
    use super::*;

    pub use accounts::Account;
    pub use sessions::Session;

    pub fn create_account(name: &str) -> Result<Account> {
        accounts::create(name)
    }

    pub fn delete_account(name: &str) -> Result<()> {
        accounts::delete(name)
    }

    pub fn list_accounts() -> Result<Vec<Account>> {
        accounts::list()
    }

    pub fn scan_sessions() -> Result<Vec<Session>> {
        sessions::Cache::default().scan(None)
    }

    pub fn read_transcript(
        path: &std::path::Path,
        max_records: usize,
    ) -> Result<Vec<serde_json::Value>> {
        sessions::read_transcript(path, max_records)
    }
}

// ---------------------------------------------------------------- katalog
//
// Hepsi hesap kapsamlı: `config_dir_for` ne döndürüyorsa `claude` alt komutları
// da o hesapla çalışıyor.

#[tauri::command]
fn list_models(account: String) -> Result<Vec<catalog::ModelOption>> {
    catalog::list_models(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn effort_levels() -> Vec<String> {
    catalog::EFFORT_LEVELS.iter().map(|s| s.to_string()).collect()
}

#[tauri::command]
fn read_preferences(account: String) -> Result<catalog::Preferences> {
    catalog::read_preferences(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn write_preferences(account: String, preferences: catalog::Preferences) -> Result<()> {
    catalog::write_preferences(config_dir_for(&account)?.as_deref(), &preferences)
}

#[tauri::command]
fn list_mcp_servers(account: String) -> Result<Vec<catalog::McpServer>> {
    catalog::list_mcp_servers(config_dir_for(&account)?.as_deref())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn mcp_add(
    account: String,
    name: String,
    transport: String,
    target: String,
    headers: Vec<String>,
    env: Vec<String>,
    command_args: Vec<String>,
    project_scope: bool,
) -> Result<()> {
    let dir = config_dir_for(&account)?;

    if transport == "stdio" {
        catalog::mcp_add_stdio(
            dir.as_deref(),
            &name,
            &target,
            &command_args,
            &env,
            project_scope,
        )
    } else {
        catalog::mcp_add_http(
            dir.as_deref(),
            &name,
            &target,
            &transport,
            &headers,
            project_scope,
        )
    }
}

#[tauri::command]
fn mcp_remove(account: String, name: String) -> Result<()> {
    catalog::mcp_remove(config_dir_for(&account)?.as_deref(), &name)
}

#[tauri::command]
fn list_plugins(account: String) -> Result<Vec<catalog::Plugin>> {
    catalog::list_plugins(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn list_available_plugins(account: String) -> Result<Vec<catalog::Plugin>> {
    catalog::list_available_plugins(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn plugin_install(account: String, id: String) -> Result<()> {
    catalog::plugin_install(config_dir_for(&account)?.as_deref(), &id)
}

#[tauri::command]
fn plugin_uninstall(account: String, id: String) -> Result<()> {
    catalog::plugin_uninstall(config_dir_for(&account)?.as_deref(), &id)
}

#[tauri::command]
fn plugin_set_enabled(account: String, id: String, enabled: bool) -> Result<()> {
    catalog::plugin_set_enabled(config_dir_for(&account)?.as_deref(), &id, enabled)
}

#[tauri::command]
fn list_marketplaces(account: String) -> Result<Vec<catalog::Marketplace>> {
    catalog::list_marketplaces(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn marketplace_add(account: String, source: String) -> Result<()> {
    catalog::marketplace_add(config_dir_for(&account)?.as_deref(), &source)
}

#[tauri::command]
fn marketplace_remove(account: String, name: String) -> Result<()> {
    catalog::marketplace_remove(config_dir_for(&account)?.as_deref(), &name)
}

#[tauri::command]
fn marketplace_update(account: String, name: Option<String>) -> Result<()> {
    catalog::marketplace_update(config_dir_for(&account)?.as_deref(), name.as_deref())
}

#[tauri::command]
fn list_skills(account: String) -> Result<Vec<catalog::Skill>> {
    catalog::list_skills(config_dir_for(&account)?.as_deref())
}

#[tauri::command]
fn skill_create(account: String, name: String, description: Option<String>) -> Result<()> {
    catalog::skill_create(
        config_dir_for(&account)?.as_deref(),
        &name,
        description.as_deref(),
    )
}

#[tauri::command]
fn skill_delete(account: String, name: String) -> Result<()> {
    catalog::skill_delete(config_dir_for(&account)?.as_deref(), &name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                agent: Arc::new(agent::Manager::default()),
                sessions: Arc::new(sessions::Cache::default()),
            });

            // Webview hatalarını görmenin tek pratik yolu; sürüm derlemede yok.
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_accounts,
            create_account,
            repair_account,
            delete_account,
            account_login,
            log_frontend,
            list_sessions,
            refresh_sessions,
            read_transcript,
            read_text_file,
            agent_start,
            agent_set_model,
            agent_send,
            list_models,
            effort_levels,
            read_preferences,
            write_preferences,
            list_mcp_servers,
            mcp_add,
            mcp_remove,
            list_plugins,
            list_available_plugins,
            plugin_install,
            plugin_uninstall,
            plugin_set_enabled,
            list_marketplaces,
            marketplace_add,
            marketplace_remove,
            marketplace_update,
            list_skills,
            skill_create,
            skill_delete,
            agent_respond_permission,
            agent_set_permission_mode,
            agent_interrupt,
            agent_stop,
            agent_active,
        ])
        .run(tauri::generate_context!())
        .expect("postillion başlatılamadı");
}
