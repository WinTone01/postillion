mod accounts;
mod agent;
mod auth;
mod catalog;
mod error;
mod paths;
mod processes;
mod screenshot;
mod session_prefs;
mod sessions;
mod usage;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager as _, State};

use accounts::Account;
use error::{Error, Result};
use sessions::Session;

struct AppState {
    agent: agent::SharedManager,
    auth: Arc<auth::Manager>,
    sessions: Arc<sessions::Cache>,
}

/// Hesap geçişi ile kullanım sorgusunu birbirinden ayırır.
///
/// `switch` çalışan bir `claude` süreci görürse geçişi reddediyor — kimlik
/// dosyası altından değişen bir süreç bozuk duruma düşer. Kullanım sorgusu da
/// kısa ömürlü bir `claude` süreci; kilitlenmezse arka plandaki yoklama
/// kullanıcının geçişini rastgele anlarda başarısız kılardı.
static EXCLUSIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Kilidi alır; başka bir iş panikleyip zehirlemişse yine de devam eder —
/// koruduğumuz şey bir veri değil, sıralama.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------- hesaplar

#[tauri::command]
fn list_accounts() -> Result<Vec<Account>> {
    accounts::list()
}

/// Sistem genelinde etkin hesabı değiştirir; terminaldeki `claude` de etkilenir.
#[tauri::command]
fn switch_account(slug: String) -> Result<Account> {
    let _guard = exclusive();
    accounts::switch(&slug)
}

#[tauri::command]
fn remove_account(slug: String) -> Result<()> {
    accounts::remove(&slug)
}

// --------------------------------------------------------------------- giriş

#[tauri::command]
fn login_start(app: AppHandle, state: State<'_, AppState>, email: Option<String>) -> Result<()> {
    state.auth.start(app, email.as_deref())
}

#[tauri::command]
fn login_submit_code(state: State<'_, AppState>, code: String) -> Result<()> {
    state.auth.submit_code(&code)
}

#[tauri::command]
fn login_cancel(state: State<'_, AppState>) -> Result<()> {
    state.auth.cancel()
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


#[tauri::command]
fn agent_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    cwd: Option<String>,
    resume: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    mcp_servers: Option<Vec<String>>,
) -> Result<String> {
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
            cwd: cwd.as_deref(),
            resume: resume.as_deref(),
            model: model.as_deref(),
            effort: effort.as_deref(),
            mcp_servers: mcp_servers.as_deref(),
        },
    )
}

// ---------------------------------------------------------------- süreçler

/// Oturumun `claude` sürecinin altında çalışan her şey.
///
/// Arka planda çalıştırılan komutlar arayüzde yalnızca "çalışıyor" olarak
/// görünüyordu; burada ne olduğu görülüyor ve tek tek durdurulabiliyor.
#[tauri::command]
fn agent_processes(state: State<'_, AppState>, id: String) -> Vec<processes::Proc> {
    match state.agent.session_pid(&id) {
        Some(pid) => processes::descendants(pid),
        None => Vec::new(),
    }
}

/// Bir alt süreci durdurur; `force` ile SIGKILL.
#[tauri::command]
fn agent_kill_process(
    state: State<'_, AppState>,
    id: String,
    pid: u32,
    force: bool,
) -> Result<()> {
    let root = state
        .agent
        .session_pid(&id)
        .ok_or_else(|| Error::SessionNotFound(id.clone()))?;

    processes::kill(root, pid, force)
}

/// Modeli süren oturumda değiştirir; bağlam korunuyor.
#[tauri::command]
fn agent_set_model(state: State<'_, AppState>, id: String, model: String) -> Result<()> {
    state.agent.set_model(&id, &model)
}

#[tauri::command]
fn agent_send(
    state: State<'_, AppState>,
    id: String,
    text: String,
    images: Option<Vec<agent::Image>>,
) -> Result<()> {
    state
        .agent
        .send_user_message(&id, &text, &images.unwrap_or_default())
}

// --------------------------------------------------------- ekran görüntüsü

/// Bölge seçtirip ekran görüntüsü alır; iptal edilirse `None`.
///
/// Pencere yakalama sırasında gizleniyor: Claude Desktop da böyle yapıyor ve
/// aksi halde uygulamanın kendisi görüntünün içinde kalıyor. Gizleme her
/// durumda geri alınıyor — araç hata verse bile pencere kaybolmamalı.
#[tauri::command]
async fn capture_screenshot(window: tauri::Window) -> Result<Option<screenshot::Shot>> {
    let _ = window.hide();

    // Bileşik yöneticinin pencereyi gerçekten kaldırması bir kare sürüyor;
    // beklenmezse uygulama görüntüye giriyor.
    std::thread::sleep(std::time::Duration::from_millis(220));

    let result = screenshot::capture_region();

    let _ = window.show();
    let _ = window.set_focus();

    result
}

/// Panodaki görüntü; yoksa `null`.
#[tauri::command]
fn clipboard_image() -> Option<screenshot::Shot> {
    screenshot::clipboard_image()
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

// ------------------------------------------------ oturum tercihleri

/// Oturum kimliği → kalıcı tercihler (şimdilik MCP seçimi).
#[tauri::command]
fn session_prefs() -> session_prefs::Store {
    session_prefs::read()
}

/// Bir sohbetin MCP seçimini kalıcı yapar; `null` genel yapılandırmaya döner.
#[tauri::command]
fn set_session_mcp(session_id: String, servers: Option<Vec<String>>) -> Result<()> {
    session_prefs::set_mcp(&session_id, servers)
}

// ------------------------------------------------------------- kullanım

/// Diskteki son ölçümler: hesap kısa adı → kullanım.
///
/// Etkin olmayan hesaplar için tek kaynak bu. Ölçüm zamanı da döndüğü için
/// arayüz değerin ne kadar eski olduğunu gösterebiliyor.
#[tauri::command]
fn usage_cache() -> usage::Cache {
    usage::read_cache()
}

/// Etkin hesabın kullanımını ölçüp önbelleğe yazar.
#[tauri::command]
fn refresh_usage() -> Result<Option<usage::Usage>> {
    let _guard = exclusive();

    let Some(active) = accounts::list()?.into_iter().find(|a| a.is_active) else {
        return Ok(None);
    };

    let measured = usage::query(Some(&active.slug))?;
    usage::write_cache(&active.slug, &measured)?;
    Ok(Some(measured))
}

/// Bütün hesapların kullanımını ölçüp önbelleğe yazar.
///
/// Resmi API her hesabı kendi saklanmış jetonuyla sorgulayabildiği için artık
/// ölçmek adına hesap değiştirmek gerekmiyor — eskiden etkin olmayan hesaplarda
/// gösterilen değer, o hesabın en son etkin olduğu andan kalmaydı.
///
/// Jetonu olmayan ya da süresi dolmuş hesaplar atlanıyor: önbellekteki eski
/// ölçümleri duruyor ve arayüz onları yaşıyla birlikte gösteriyor.
#[tauri::command]
fn refresh_all_usage() -> Result<usage::Cache> {
    let _guard = exclusive();

    for account in accounts::list()? {
        if !account.has_credentials {
            continue;
        }
        // Etkin hesap API'den okunamazsa yerel önbellek ve komut yolları
        // devrede; diğerleri için tek kaynak API.
        let measured = if account.is_active {
            usage::query(Some(&account.slug)).ok()
        } else {
            usage::fetch(&account.slug)
        };

        if let Some(measured) = measured {
            usage::write_cache(&account.slug, &measured)?;
        }
    }

    Ok(usage::read_cache())
}


/// Entegrasyon testlerinin çekirdeğe erişimi.
///
/// Tauri komutları `State` istediği için testlerden doğrudan çağrılamıyor;
/// bu ince katman aynı fonksiyonları çıplak haliyle açar.
pub mod testing {
    use super::*;

    pub use accounts::Account;
    pub use sessions::Session;

    pub fn switch_account(slug: &str) -> Result<Account> {
        accounts::switch(slug)
    }

    pub fn remove_account(slug: &str) -> Result<()> {
        accounts::remove(slug)
    }

    pub fn slugify(email: &str) -> String {
        accounts::slugify(email)
    }

    pub fn list_accounts() -> Result<Vec<Account>> {
        accounts::list()
    }

    pub fn scan_sessions() -> Result<Vec<Session>> {
        sessions::Cache::default().scan(None)
    }

    /// Soğuk ve ısıtılmış tarama süreleri (ms). Kalıcı önbelleğin ölçümü.
    pub fn scan_cold_then_warm() -> (u128, u128, usize) {
        let cold_cache = sessions::Cache::default();
        let start = std::time::Instant::now();
        let cold = cold_cache.scan(None).unwrap_or_default();
        let cold_ms = start.elapsed().as_millis();

        let warm_cache = sessions::Cache::default();
        warm_cache.warm();
        let start = std::time::Instant::now();
        let warm = warm_cache.scan(None).unwrap_or_default();
        let warm_ms = start.elapsed().as_millis();

        assert_eq!(cold.len(), warm.len(), "ısıtılmış tarama farklı sonuç verdi");
        (cold_ms, warm_ms, warm.len())
    }

    pub fn read_transcript(
        path: &std::path::Path,
        max_records: usize,
    ) -> Result<Vec<serde_json::Value>> {
        sessions::read_transcript(path, max_records)
    }

    pub use usage::Usage;

    pub fn descendants(root: u32) -> Vec<crate::processes::Proc> {
        processes::descendants(root)
    }

    pub fn clipboard_image() -> Option<crate::screenshot::Shot> {
        screenshot::clipboard_image()
    }

    pub fn query_usage() -> Result<Usage> {
        usage::query(None)
    }

    /// Yalnızca yerel önbellek yolu; komuta düşmüyor.
    pub fn local_usage() -> Option<Usage> {
        usage::read_local()
    }

    /// Yalnızca resmi API yolu.
    pub fn fetch_usage(slug: &str) -> Option<Usage> {
        usage::fetch(slug)
    }

    pub fn list_accounts_raw() -> Result<Vec<Account>> {
        accounts::list()
    }
}

// ---------------------------------------------------------------- katalog
//
// Hepsi tek yapılandırma üzerinde çalışıyor: etkin hesap sistem genelinde
// belirlendiği için `claude` alt komutlarına ayrıca hesap geçirmeye gerek yok.

#[tauri::command]
fn list_models() -> Result<Vec<catalog::ModelOption>> {
    catalog::list_models(None)
}

#[tauri::command]
fn effort_levels() -> Vec<String> {
    catalog::EFFORT_LEVELS.iter().map(|s| s.to_string()).collect()
}

#[tauri::command]
fn read_preferences() -> Result<catalog::Preferences> {
    catalog::read_preferences(None)
}

#[tauri::command]
fn write_preferences(preferences: catalog::Preferences) -> Result<()> {
    catalog::write_preferences(None, &preferences)
}

#[tauri::command]
fn list_mcp_servers() -> Result<Vec<catalog::McpServer>> {
    catalog::list_mcp_servers(None)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn mcp_add(
    name: String,
    transport: String,
    target: String,
    headers: Vec<String>,
    env: Vec<String>,
    command_args: Vec<String>,
    project_scope: bool,
) -> Result<()> {
    if transport == "stdio" {
        catalog::mcp_add_stdio(
            None,
            &name,
            &target,
            &command_args,
            &env,
            project_scope,
        )
    } else {
        catalog::mcp_add_http(
            None,
            &name,
            &target,
            &transport,
            &headers,
            project_scope,
        )
    }
}

#[tauri::command]
fn mcp_remove(name: String) -> Result<()> {
    catalog::mcp_remove(None, &name)
}

#[tauri::command]
fn list_plugins() -> Result<Vec<catalog::Plugin>> {
    catalog::list_plugins(None)
}

#[tauri::command]
fn list_available_plugins() -> Result<Vec<catalog::Plugin>> {
    catalog::list_available_plugins(None)
}

#[tauri::command]
fn plugin_install(id: String) -> Result<()> {
    catalog::plugin_install(None, &id)
}

#[tauri::command]
fn plugin_uninstall(id: String) -> Result<()> {
    catalog::plugin_uninstall(None, &id)
}

#[tauri::command]
fn plugin_set_enabled(id: String, enabled: bool) -> Result<()> {
    catalog::plugin_set_enabled(None, &id, enabled)
}

#[tauri::command]
fn list_marketplaces() -> Result<Vec<catalog::Marketplace>> {
    catalog::list_marketplaces(None)
}

#[tauri::command]
fn marketplace_add(source: String) -> Result<()> {
    catalog::marketplace_add(None, &source)
}

#[tauri::command]
fn marketplace_remove(name: String) -> Result<()> {
    catalog::marketplace_remove(None, &name)
}

#[tauri::command]
fn marketplace_update(name: Option<String>) -> Result<()> {
    catalog::marketplace_update(None, name.as_deref())
}

#[tauri::command]
fn list_skills() -> Result<Vec<catalog::Skill>> {
    catalog::list_skills(None)
}

#[tauri::command]
fn skill_create(name: String, description: Option<String>) -> Result<()> {
    catalog::skill_create(
        None,
        &name,
        description.as_deref(),
    )
}

#[tauri::command]
fn skill_delete(name: String) -> Result<()> {
    catalog::skill_delete(None, &name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Kod bloklarındaki kopyala butonu için: WebKitGTK'da
        // `navigator.clipboard` güvenli bağlam istiyor ve Tauri'nin özel
        // protokolü öyle sayılmıyor, çağrı sessizce düşüyordu.
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Önbellek diskten ısıtılıyor: ilk taramanın 412 MB'ı yeniden
            // okuması gerekmiyor, yalnızca dokunulmuş dosyalar ayrıştırılıyor.
            let sessions = Arc::new(sessions::Cache::default());
            sessions.warm();

            app.manage(AppState {
                agent: Arc::new(agent::Manager::default()),
                auth: Arc::new(auth::Manager::default()),
                sessions,
            });

            // Webview hatalarını görmenin tek pratik yolu; sürüm derlemede yok.
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            Ok(())
        })
        // Kapanışta `claude` çocukları öldürülüyor. Önceden hiçbir temizlik
        // yoktu: pencere kapanınca süreçler init'e evlat ediniliyor ve
        // arkada çalışmaya devam ediyordu.
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.agent.stop_all();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_accounts,
            switch_account,
            remove_account,
            login_start,
            login_submit_code,
            login_cancel,
            log_frontend,
            list_sessions,
            refresh_sessions,
            read_transcript,
            read_text_file,
            agent_start,
            agent_set_model,
            agent_send,
            capture_screenshot,
            clipboard_image,
            session_prefs,
            set_session_mcp,
            usage_cache,
            refresh_usage,
            refresh_all_usage,
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
            agent_processes,
            agent_kill_process,
        ])
        .run(tauri::generate_context!())
        .expect("postillion başlatılamadı");
}
