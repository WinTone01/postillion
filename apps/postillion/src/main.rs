//! postillion — headed by default; `postillion headless` runs the engine alone. Both start
//! local-only without credentials. `postillion login` and `postillion logout` select the
//! profile used by the next engine start without mutating a live runtime.
//!
//! Windows'ta ikili GUI alt sistemiyle bağlanıyor: konsol alt sistemi, Başlat
//! menüsünden açılan her pencerenin arkasında boş bir cmd penceresi bırakıyor.
//! Bedeli, CLI alt komutlarının stdout'unu kaybetmesi — bunu `attach_console`
//! telafi ediyor (aşağıda).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod auth_cli;
mod daemon;
mod update_cli;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "postillion", about = "Multi-device controller for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the engine without a UI (local-only unless a saved session enables sync).
    Headless,
    /// Sign in and enable sync on the next engine start.
    Login,
    /// Remove the saved session and return to local-only on the next start.
    Logout,
    /// Show workspace mode, optional auth, and engine status.
    Status,
    /// Live sync introspection from the running engine: per-room connection
    /// state, last pushed-frame/ack ages, rejoin/probe/resync counters.
    Sync,
    /// Manage `postillion headless` as a background service (launchd / systemd --user).
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Check for a newer release and apply it (download → verify → swap →
    /// service restart). `--check` only reports (exits 1 when one is available).
    Update {
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Install, enable, and start the service (captures POSTILLION_* env).
    Install,
    /// Stop and remove the service.
    Uninstall,
    /// Start the installed service.
    Start,
    /// Stop the service.
    Stop,
    /// Restart the service.
    Restart,
    /// Show the service manager's view of the daemon.
    Status,
}

/// Sync endpoint. Empty by default: Postillion ships no hosted edge, so a bare
/// install stays local-only until `POSTILLION_EDGE_URL` points at a server you
/// run yourself (see `deploy/`).
///
/// This deliberately does not inherit the upstream project's endpoint. Keeping
/// it would have sent every synced chat to a host we do not operate.
const DEFAULT_EDGE_URL: &str = "";

/// WorkOS AuthKit client id. `None` by default for the same reason: the
/// upstream client id authenticates against an identity provider we do not
/// control. Set `POSTILLION_WORKOS_CLIENT_ID` to enable hosted sign-in.
const DEFAULT_WORKOS_CLIENT_ID: Option<&str> = None;

/// Eşitleme uç noktası ve jetonu: ortam değişkeni, yoksa panelden kaydedilmiş
/// yapılandırma.
///
/// Yalnızca ortama bakmak, masaüstü kısayolundan açılan bir uygulamada
/// eşitlemeyi sessizce kapalı bırakıyordu: kısayolun ortamında o değişkenler
/// yok. Artık ayar diske yazılıyor ve buradan okunuyor; ortam hâlâ kazanıyor,
/// böylece betikler ve testler tek seferlik yönlendirme yapabiliyor.
fn sync_config(data_dir: &std::path::Path) -> postillion_engine::sync_config::SyncConfig {
    let mut resolved = postillion_engine::sync_config::resolve(data_dir);
    if resolved.edge_url.trim().is_empty() {
        resolved.edge_url = DEFAULT_EDGE_URL.into();
    }
    resolved
}

/// WorkOS client id resolution: explicit env wins (empty string = dev mode);
/// otherwise a `POSTILLION_EDGE_TOKEN` dev bearer keeps dev mode (smoke tests,
/// local wrangler); otherwise `DEFAULT_WORKOS_CLIENT_ID`, which is `None` until
/// you configure your own AuthKit tenant.
fn workos_client_id_from_env(edge_token: &Option<String>) -> Option<String> {
    match std::env::var("POSTILLION_WORKOS_CLIENT_ID") {
        Ok(v) if v.trim().is_empty() => None,
        Ok(v) => Some(v),
        Err(_) if edge_token.is_some() => None,
        Err(_) => DEFAULT_WORKOS_CLIENT_ID.map(Into::into),
    }
}

/// mimalloc: system malloc (macOS libmalloc especially) never returns the
/// streaming churn's high-water pages, so transient allocation became
/// permanent RSS (docs/memory-plan.md §1).
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `POSTILLION_BACKEND=x11` verildiğinde pencere arka ucunu X11'e zorlar.
///
/// gpui arka ucu yalnızca ortama bakarak seçiyor: `WAYLAND_DISPLAY` doluysa
/// koşulsuz Wayland, başka bir anahtar yok. Bazı kurulumlarda (ölçüldü: KDE
/// Wayland + NVIDIA RTX 3060, sürücü Vulkan'ı sorunsuz veriyor) pencere
/// oluşturuluyor ama hiç haritalanmıyor — süreç sağlıklı çalışıyor, GPU
/// seçiliyor, tek bir hata satırı bile yok, ekranda hiçbir şey yok. Böyle bir
/// makinede uygulamayı açmanın tek yolu XWayland'e düşmek.
///
/// Değişkeni burada, gpui'ye dokunmadan önce silmek `guess_compositor()`'un
/// X11'i seçmesini sağlıyor. `wayland` değeri açıkça Wayland'da kalmak için;
/// tanınmayan değer yok sayılıyor ve normal otomatik seçim işliyor.
fn apply_backend_override() {
    let Ok(backend) = std::env::var("POSTILLION_BACKEND") else {
        return;
    };
    if !backend.eq_ignore_ascii_case("x11") {
        return;
    }
    // SAFETY: tek iş parçacıklı başlangıç; gpui henüz hiçbir şey okumadı ve
    // hiçbir iş parçacığı başlatılmadı.
    unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
}

/// GUI alt sistemiyle bağlanan ikili kendi konsolunu almıyor; onu başlatan
/// terminal varsa ona iliştiriyoruz ki `postillion status` çıktısı ve clap'in
/// `--help`/hata mesajları görünsün. Terminalden başlatılmadıysa (Başlat menüsü,
/// kısayol) `AttachConsole` başarısız olur ve hiçbir şey değişmez.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: argümansız FFI çağrısı; başarısızlığı anlamlı ve zararsız.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    attach_console();
    apply_backend_override();
    let cli = Cli::parse();
    // Long-running modes log at info, one-shot CLI commands at warn (RUST_LOG
    // overrides either).
    // loro's internal block-encode diagnostics log at info and flood
    // journald on every snapshot export — enough to fill a disk on a
    // long-running headless host. Quiet them by default (RUST_LOG still
    // overrides the whole filter).
    let long_running = matches!(&cli.command, None | Some(Command::Headless));
    let default_filter = if long_running {
        "info,loro_internal=warn,loro=warn"
    } else {
        "warn"
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());
    // Long-running modes mirror stdout logging to {data_dir}/logs — a headed
    // app launched from Finder has no visible stdout, which left every sync
    // wedge report ("stale until restart") with zero diagnostics even though
    // the engine logs the exact failure line. One file per launch, previous
    // launch kept as `.old`.
    let log_file = if long_running {
        let mode = if cli.command.is_some() {
            "headless"
        } else {
            "headed"
        };
        open_log_file(mode)
    } else {
        None
    };
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        let registry = tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer());
        match log_file {
            Some(file) => registry
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(std::sync::Arc::new(file)),
                )
                .init(),
            None => registry.init(),
        }
    }

    match cli.command {
        Some(Command::Headless) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(async {
                let engine = postillion_engine::Engine::new(engine_config_from_env());
                engine.run().await
            })
        }
        Some(Command::Login) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(auth_cli::login(engine_config_from_env()))
        }
        Some(Command::Logout) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(auth_cli::logout(engine_config_from_env()))
        }
        Some(Command::Status) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(auth_cli::status(engine_config_from_env()))
        }
        Some(Command::Sync) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(sync_cli(engine_config_from_env().ipc_port))
        }
        Some(Command::Update { check }) => {
            let runtime = tokio::runtime::Runtime::new()?;
            // Güncelleyici de aynı kaynağı kullanıyor: panelden yapılandırılmış
            // bir kurulumda `postillion update` boş adrese düşmemeli.
            runtime.block_on(update_cli::update(
                &sync_config(&std::env::var_os("POSTILLION_DATA_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(dirs_data_dir))
                .edge_url,
                check,
            ))
        }
        Some(Command::Daemon { command }) => match command {
            DaemonCommand::Install => daemon::install(&engine_config_from_env().data_dir),
            DaemonCommand::Uninstall => daemon::uninstall(),
            DaemonCommand::Start => daemon::start(),
            DaemonCommand::Stop => daemon::stop(),
            DaemonCommand::Restart => daemon::restart(),
            DaemonCommand::Status => daemon::status(),
        },
        None => {
            let data_dir = std::env::var_os("POSTILLION_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(dirs_data_dir);
            let sync = sync_config(&data_dir);
            let edge_token = sync.token.clone();
            // Headed: the UI probes POSTILLION_IPC_PORT and connects to a running
            // daemon, or embeds the engine in-process (ARCHITECTURE §1).
            postillion_ui::run_app(postillion_ui::UiConfig {
                data_dir,
                ipc_port: std::env::var("POSTILLION_IPC_PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(27654),
                edge_url: sync.edge_url,
                workos_client_id: workos_client_id_from_env(&edge_token),
                edge_token,
                org_id: std::env::var("POSTILLION_ORG_ID").ok(),
                default_harness: postillion_ui::HarnessId::ClaudeCode,
            });
            Ok(())
        }
    }
}

/// The env-resolved engine configuration shared by `headless`, `login`,
/// `logout`, and `status` — one resolution so the CLI auth commands always
/// operate on the exact session the daemon will load.
fn engine_config_from_env() -> postillion_engine::EngineConfig {
    let data_dir = std::env::var_os("POSTILLION_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(dirs_data_dir);
    // Dev-mode bearer (no WorkOS): an explicit token enables sync. The headed
    // app and the CLI resolve this the SAME way — a daemon started from the
    // CLI has to land on the endpoint the panel configured, or the two would
    // disagree about which server owns the data.
    let sync = sync_config(&data_dir);
    let edge_token = sync.token.clone();
    postillion_engine::EngineConfig {
        data_dir,
        edge_url: sync.edge_url,
        ipc_port: std::env::var("POSTILLION_IPC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(27654),
        default_harness: harness_from_env(),
        // WorkOS mode: the signed-in session's org wins; POSTILLION_ORG_ID (dev
        // default "dev-org") scopes the workspace room otherwise.
        org_id: std::env::var("POSTILLION_ORG_ID").ok(),
        // Real auth against production by default; see
        // `workos_client_id_from_env` for the dev-mode escape hatches.
        workos_client_id: workos_client_id_from_env(&edge_token),
        edge_token,
    }
}

/// `POSTILLION_HARNESS` (kebab-case id) picks the default harness for chats without a
/// config row — `mock` powers the e2e smoke; default `claude-code`.
fn harness_from_env() -> postillion_engine::HarnessId {
    match std::env::var("POSTILLION_HARNESS").as_deref().map(str::trim) {
        Ok("mock") => postillion_engine::HarnessId::Mock,
        Ok("codex") => postillion_engine::HarnessId::Codex,
        Ok("cursor") => postillion_engine::HarnessId::Cursor,
        Ok("grok") => postillion_engine::HarnessId::Grok,
        Ok("hermes") => postillion_engine::HarnessId::Hermes,
        Ok("pi") => postillion_engine::HarnessId::Pi,
        _ => postillion_engine::HarnessId::ClaudeCode,
    }
}

fn dirs_data_dir() -> std::path::PathBuf {
    // Windows'ta `HOME` yok; `home_dir` `USERPROFILE`'a düşüyor. Eskiden burada
    // doğrudan `HOME` okunuyordu ve uygulama açılışta panikliyordu.
    let home = postillion_harness::home_dir().expect("no home directory");
    let dir = home.join(".postillion");
    // One-shot migrations: adopt the pre-rename data dir (sign-in, device
    // identity, saved account credentials, prefs) instead of starting fresh.
    //
    // Newest first. The chain matters — a machine that never ran the `.zeron`
    // build still has `.comet-native` to inherit, and losing the agent-account
    // slots would mean re-authenticating every saved login. These are upstream
    // directory names on disk, so they stay spelled the old way on purpose.
    if !dir.exists() {
        for previous in [".zeron", ".comet-native"] {
            let old = home.join(previous);
            if old.exists() {
                match std::fs::rename(&old, &dir) {
                    Ok(()) => eprintln!("veri dizini taşındı: {} -> {}", old.display(), dir.display()),
                    Err(err) => eprintln!(
                        "veri dizini taşınamadı ({} -> {}): {err}",
                        old.display(),
                        dir.display()
                    ),
                }
                break;
            }
        }
    }
    dir
}

/// `postillion sync`: dial the running engine's IPC and print per-room sync state.
/// The introspection surface every 2026-08 incident was missing — "is this
/// device's workspace room actually receiving?" as a one-liner.
async fn sync_cli(ipc_port: u16) -> anyhow::Result<()> {
    let client = postillion_rpc::connect_ws(&format!("ws://127.0.0.1:{ipc_port}"))
        .await
        .map_err(|e| {
            anyhow::anyhow!("no engine listening on 127.0.0.1:{ipc_port} ({e}) — is postillion running?")
        })?;
    let status = client
        .call(postillion_rpc::methods::SYNC_STATUS, serde_json::json!({}))
        .await
        .map_err(|e| anyhow::anyhow!("SyncStatus failed: {e}"))?;
    let now = status.get("nowMs").and_then(|v| v.as_i64()).unwrap_or(0);
    let age = |ms: i64| -> String {
        if ms <= 0 {
            return "never".into();
        }
        let s = (now - ms).max(0) / 1000;
        if s >= 3600 {
            format!("{}h{}m ago", s / 3600, (s % 3600) / 60)
        } else if s >= 60 {
            format!("{}m{}s ago", s / 60, s % 60)
        } else {
            format!("{s}s ago")
        }
    };
    let room_line = |room: Option<&serde_json::Value>| -> String {
        let Some(room) = room else {
            return "no room (dialing or edge-less)".into();
        };
        let get = |k: &str| room.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        // REJECTED is loud and only shown when nonzero: rejected writes with
        // a fresh-looking room is exactly the latched-session wedge
        // (2026-08-04) this readout previously masked.
        let rejected = get("rejected");
        format!(
            "{} pushed {} · acked {} · rejoins {} probes {} resyncs {} drops {}{}",
            if room.get("connected").and_then(|v| v.as_bool()) == Some(true) {
                "connected ·"
            } else {
                "DISCONNECTED ·"
            },
            age(get("lastPushedMs")),
            age(get("lastAckMs")),
            get("rejoins"),
            get("probes"),
            get("fullResyncs"),
            get("disconnects"),
            if rejected > 0 {
                format!(" REJECTED {rejected}")
            } else {
                String::new()
            },
        )
    };
    println!(
        "Device:    {}",
        status
            .get("deviceId")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
    );
    println!(
        "Workspace: {}",
        room_line(status.get("workspace").filter(|v| !v.is_null()))
    );
    let chats = status
        .get("chats")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if chats.is_empty() {
        println!("Chats:     none open");
    }
    // Chat rooms speak chat2: cursor/head tell "am I caught up?", pending
    // tells "did my writes leave?", resets/rejected are the loud tells.
    let chat_line = |room: Option<&serde_json::Value>| -> String {
        let Some(room) = room else {
            return "no room (dialing or edge-less)".into();
        };
        let get = |k: &str| room.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        let resets = get("serverResets");
        let rejected = get("rejected");
        format!(
            "{} cursor {}/{} · pending {} · rows {} ({}KB) · rejoins {} drops {}{}{}",
            if room.get("connected").and_then(|v| v.as_bool()) == Some(true) {
                "connected ·"
            } else {
                "DISCONNECTED ·"
            },
            get("cursor"),
            get("headSeq"),
            get("pendingPushes"),
            get("rowCount"),
            get("rowBytes") / 1024,
            get("rejoins"),
            get("disconnects"),
            if resets > 0 {
                format!(" RESETS {resets}")
            } else {
                String::new()
            },
            if rejected > 0 {
                format!(" REJECTED {rejected}")
            } else {
                String::new()
            },
        )
    };
    for chat in &chats {
        println!(
            "Chat {}: {}",
            chat.get("chatId")
                .and_then(|v| v.as_str())
                .map(|s| &s[..s.len().min(8)])
                .unwrap_or("?"),
            chat_line(chat.get("room").filter(|v| !v.is_null()))
        );
    }
    Ok(())
}

/// `{data_dir}/logs/postillion-{mode}.log`, previous launch preserved as `.old`.
/// Headed and headless are separate files so an embedded-engine app and a
/// daemon on the same machine never interleave writes.
///
/// The returned file holds an exclusive `flock` for the process lifetime:
/// rotate-on-launch is only safe when nothing is still WRITING the current
/// file. On 2026-08-04 a dev build launched twice next to the running
/// installed app — the first rename put the daemon's live log at `.old`, the
/// second unlinked it entirely, and the daemon spent the rest of the incident
/// logging to an orphaned inode (an entire day of sync diagnostics gone at
/// the exact moment they were needed). A launch that finds the canonical file
/// locked logs to `postillion-{mode}.{pid}.log` instead; the next lock-holding
/// launch sweeps pid-suffixed files older than a week.
fn open_log_file(mode: &str) -> Option<std::fs::File> {
    let dir = std::env::var_os("POSTILLION_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(dirs_data_dir)
        .join("logs");
    open_log_file_in(&dir, mode)
}

/// Dir-parameterized body of [`open_log_file`] (unit-testable without env).
fn open_log_file_in(dir: &std::path::Path, mode: &str) -> Option<std::fs::File> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("postillion-{mode}.log"));
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        // Probe the CURRENT inode for a live writer before touching it.
        let preexisting = path.exists();
        let existing = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .ok()?;
        let rc = unsafe { libc::flock(existing.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            // A live process owns the canonical log — leave it alone.
            return std::fs::File::create(
                dir.join(format!("postillion-{mode}.{}.log", std::process::id())),
            )
            .ok();
        }
        // No live writer: rotate, create fresh, and lock it as ours. (The
        // probe's flock dies with `existing`; a first-ever launch has nothing
        // to rotate — the probe itself created the empty file.)
        drop(existing);
        if preexisting {
            let _ = std::fs::rename(&path, dir.join(format!("postillion-{mode}.log.old")));
        }
        let file = std::fs::File::create(&path).ok()?;
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        sweep_stale_pid_logs(dir, mode);
        Some(file)
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::rename(&path, dir.join(format!("postillion-{mode}.log.old")));
        std::fs::File::create(&path).ok()
    }
}

#[cfg(all(test, unix))]
mod log_file_tests {
    use super::open_log_file_in;

    #[test]
    fn second_launch_never_rotates_a_live_processes_log() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        // First launch owns the canonical file and keeps writing.
        let first = open_log_file_in(dir, "headed").expect("first log");
        assert!(dir.join("postillion-headed.log").is_file());
        // Second launch while the first is alive: canonical file untouched,
        // pid-suffixed overflow file instead (the 2026-08-04 clobber).
        let second = open_log_file_in(dir, "headed").expect("second log");
        let pid_path = dir.join(format!("postillion-headed.{}.log", std::process::id()));
        assert!(pid_path.is_file(), "expected pid-suffixed overflow log");
        assert!(
            !dir.join("postillion-headed.log.old").exists(),
            "live canonical log must not be rotated away"
        );
        drop(second);
        // After the owner exits, a fresh launch rotates normally.
        drop(first);
        let third = open_log_file_in(dir, "headed").expect("third log");
        assert!(
            dir.join("postillion-headed.log.old").is_file(),
            "rotation resumes"
        );
        drop(third);
    }
}

/// Delete `postillion-{mode}.{pid}.log` overflow files older than a week — they
/// only exist when a second instance raced a live one for the canonical log.
#[cfg(unix)]
fn sweep_stale_pid_logs(dir: &std::path::Path, mode: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("postillion-{mode}.");
    let week = std::time::Duration::from_secs(7 * 24 * 60 * 60);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(middle) = name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".log"))
        else {
            continue;
        };
        if !middle.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > week);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
