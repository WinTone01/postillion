//! Single-instance lock — an exclusive advisory `flock` on `{data_dir}/engine.lock`
//! held for the engine's lifetime. Two engines sharing one data dir would race the
//! SQLite snapshots DB and the append-only run journals (WAL + `busy_timeout` guard
//! individual statements, not whole-file ownership), so the second instance must
//! fail fast with a clear error instead of corrupting state.
//!
//! The lock is taken in `EngineCore::assemble_with_identity` BEFORE any store is opened
//! and before the IPC port binds, which also closes the race where a headed app's
//! TCP probe sees no daemon during another instance's startup window.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::EngineError;

/// Held lock on the data dir. Dropping it (engine shutdown / process exit)
/// releases the advisory lock; a crash releases it too (kernel-owned).
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Acquire the exclusive lock, non-blocking. Errors with a descriptive
    /// message (including the holder's pid when readable) if another engine
    /// already owns this data dir.
    pub fn acquire(data_dir: &Path) -> Result<Self, EngineError> {
        let path = data_dir.join("engine.lock");
        #[cfg(windows)]
        let mut file = {
            // Windows'ta `flock` yok; paylaşım kipini sıfırlamak (dosyayı
            // münhasır açmak) aynı işi görüyor — ikinci motor açarken
            // PermissionDenied alır. Pid damgasını okuyabilmek için ikinci
            // motor önce paylaşımlı bir okuma denemesi yapıyor (aşağıda).
            match open_exclusive(&path) {
                Ok(file) => file,
                Err(err) if is_lock_contention(&err) => {
                    // Kilit dosyası münhasır açıldığı için içeriği okunamaz;
                    // pid damgası yanındaki `engine.pid`'de duruyor.
                    let holder = std::fs::read_to_string(pid_stamp_path(data_dir)).unwrap_or_default();
                    let holder = holder.trim();
                    return Err(EngineError::Other(format!(
                        "another postillion engine is already running on {} (pid {}); \
                         stop it or use a different data dir (POSTILLION_DATA_DIR)",
                        data_dir.display(),
                        if holder.is_empty() { "unknown" } else { holder },
                    )));
                }
                Err(err) => return Err(EngineError::Io(err)),
            }
        };
        #[cfg(not(windows))]
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // Bounded EWOULDBLOCK retries: a fork→exec window in ANY process
            // that inherited the previous holder's fd (git scans, harness
            // spawns — fds are duplicated between fork and CLOEXEC-at-exec)
            // keeps the flock alive for a few milliseconds after release. A
            // real second engine holds it forever; transient artifacts clear
            // well within the budget.
            let mut retries = 40u32; // × 25ms = 1s budget
            loop {
                let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
                if rc == 0 {
                    break;
                }
                let errno = std::io::Error::last_os_error();
                match errno.raw_os_error() {
                    Some(libc::EINTR) => continue, // signal-interrupted: retry
                    Some(libc::EWOULDBLOCK) if retries > 0 => {
                        retries -= 1;
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Some(libc::EWOULDBLOCK) => {
                        let holder = std::fs::read_to_string(&path).unwrap_or_default();
                        let holder = holder.trim();
                        return Err(EngineError::Other(format!(
                            "another postillion engine is already running on {} (pid {}); \
                             stop it or use a different data dir (POSTILLION_DATA_DIR)",
                            data_dir.display(),
                            if holder.is_empty() { "unknown" } else { holder },
                        )));
                    }
                    // Anything else (ENOLCK, filesystem without flock, …) is an
                    // environment problem, not a second engine — surface it as-is.
                    _ => return Err(EngineError::Io(errno)),
                }
            }
        }

        // Best-effort pid stamp for the contention error message above.
        let _ = file.set_len(0);
        let _ = write!(file, "{}", std::process::id());
        let _ = file.flush();
        #[cfg(windows)]
        let _ = std::fs::write(pid_stamp_path(data_dir), std::process::id().to_string());
        Ok(Self { _file: file })
    }

    /// Best-effort liveness probe: the pid stamped by the engine currently holding
    /// this data dir's lock, `None` when no engine is running (or the platform
    /// cannot test a lock without taking it). Used by `postillion status` and the
    /// login/logout guards; a single non-blocking try — no retry budget — so a
    /// starting engine's transient fork-window artifacts read as "running", which
    /// is the safe direction for those callers.
    pub fn holder(data_dir: &Path) -> Option<String> {
        let path = data_dir.join("engine.lock");
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .ok()?;
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc == 0 {
                // We took it: nothing is running. Closing the fd releases it, but
                // unlock explicitly so the window is as small as possible.
                unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
                return None;
            }
            let pid = std::fs::read_to_string(&path).unwrap_or_default();
            let pid = pid.trim();
            Some(if pid.is_empty() {
                "unknown".to_string()
            } else {
                pid.to_string()
            })
        }
        #[cfg(windows)]
        {
            // Kilidi almayı dene: başarılıysa kimse çalışmıyor, elimizdekini
            // hemen bırakıyoruz. `open_exclusive` yerine tek denemelik açış —
            // bu sonda çağıranlar (status, login/logout guard) beklememeli.
            use std::os::windows::fs::OpenOptionsExt;
            let probe = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(&path);
            match probe {
                Ok(file) => {
                    drop(file);
                    None
                }
                // Yalnızca paylaşım ihlali "başkası tutuyor" demek. Veri dizini
                // henüz yokken (ilk çalıştırma) açış NotFound veriyor; bunu
                // sahiplik saymak `postillion status`'a hayali bir motor
                // gösteriyordu.
                Err(err) if !is_lock_contention(&err) => None,
                Err(_) => {
                    let pid = std::fs::read_to_string(pid_stamp_path(data_dir)).unwrap_or_default();
                    let pid = pid.trim();
                    Some(if pid.is_empty() {
                        "unknown".to_string()
                    } else {
                        pid.to_string()
                    })
                }
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            None
        }
    }
}

/// "Bu dosyayı başkası münhasır tutuyor" hatası.
///
/// `share_mode(0)` ile açılmış bir dosyayı ikinci kez açmak
/// `ERROR_SHARING_VIOLATION` (32) veriyor, `ERROR_ACCESS_DENIED` değil — ve
/// Rust bunu `ErrorKind::PermissionDenied`'a eşlemiyor, sınıflandırılmamış
/// bırakıyor. Yalnızca `kind()`'a bakan bir kontrol bu yüzden çekişmeyi
/// tamamen ıskalıyor (ölçüldü: ikinci motor dostane mesaj yerine ham
/// "io: Sharing violation" ile düşüyordu). ACL nedeniyle gerçekten erişim
/// reddi de olabileceği için ikisini birden kabul ediyoruz.
#[cfg(windows)]
pub(crate) fn is_lock_contention(err: &std::io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    matches!(
        err.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    ) || err.kind() == std::io::ErrorKind::PermissionDenied
}

/// Windows'ta kilit dosyası münhasır açıldığı için pid damgası ayrı bir
/// dosyada durur; unix'te damga kilit dosyasının kendi içeriğidir.
#[cfg(windows)]
fn pid_stamp_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("engine.pid")
}

/// Münhasır açış, sınırlı yeniden denemeyle. Unix'teki EWOULDBLOCK bütçesinin
/// karşılığı: bir önceki sahibin tanıtıcısı kapanırken (ya da bir virüs
/// tarayıcı dosyaya bakarken) kısa bir paylaşım ihlali penceresi oluşabiliyor.
#[cfg(windows)]
fn open_exclusive(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut retries = 40u32; // × 25ms = 1s bütçe
    loop {
        let attempt = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path);
        match attempt {
            Ok(file) => return Ok(file),
            Err(err) if is_lock_contention(&err) && retries > 0 => {
                retries -= 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    #[test]
    fn holder_probe_reports_pid_without_disturbing_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(InstanceLock::holder(dir.path()), None, "unlocked dir");
        let lock = InstanceLock::acquire(dir.path()).expect("acquire");
        assert_eq!(
            InstanceLock::holder(dir.path()).as_deref(),
            Some(std::process::id().to_string().as_str()),
        );
        // The probe must not have stolen the lock from the holder.
        InstanceLock::acquire(dir.path()).expect_err("still held after probe");
        drop(lock);
        assert_eq!(InstanceLock::holder(dir.path()), None, "released");
    }

    #[test]
    fn second_acquire_fails_while_held_then_succeeds_after_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock = InstanceLock::acquire(dir.path()).expect("first acquire");
        let err = InstanceLock::acquire(dir.path()).expect_err("second acquire must fail");
        let msg = err.to_string();
        assert!(msg.contains("already running"), "unexpected error: {msg}");
        assert!(
            msg.contains(&std::process::id().to_string()),
            "holder pid missing from error: {msg}"
        );
        drop(lock);
        InstanceLock::acquire(dir.path()).expect("acquire after release");
    }
}
