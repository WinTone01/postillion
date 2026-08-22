//! Bir oturumun arka planda çalıştırdığı süreçler.
//!
//! Claude'un `Bash` aracı komutları kendi alt süreci olarak açıyor; uzun süren
//! ya da takılan bir komut arayüzde yalnızca "çalışıyor" olarak görünüyordu.
//! Burada spawn ettiğimiz `claude` sürecinin **soyundan gelen** her şey `/proc`
//! üzerinden çıkarılıyor, böylece ne çalıştığı görülebiliyor ve tek tek
//! durdurulabiliyor.
//!
//! Linux'a özgü — uygulama zaten Linux masaüstü için.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// `times(2)` saat tiki; `starttime` bu birimde. Linux'ta pratikte hep 100.
const USER_HZ: u64 = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Tam komut satırı; boşsa çekirdek işçisi ya da okunamayan süreç.
    pub command: String,
    /// `stat` alanındaki tek harf: R çalışıyor, S uyuyor, Z zombi…
    pub state: String,
    pub elapsed_secs: u64,
}

/// `/proc/<pid>/stat`'ten ppid, durum ve başlangıç zamanı.
///
/// İkinci alan (`comm`) parantez içinde ve **boşluk da parantez de**
/// içerebiliyor; bu yüzden alanlara son `)` işaretinden sonra bakılıyor.
fn read_stat(pid: u32) -> Option<(u32, String, u64)> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = &raw[raw.rfind(')')? + 1..];
    let fields: Vec<&str> = tail.split_whitespace().collect();

    // `tail` 3. alandan başlıyor: state, ppid, … starttime 22. alan.
    let state = fields.first()?.to_string();
    let ppid = fields.get(1)?.parse().ok()?;
    let starttime: u64 = fields.get(19)?.parse().ok()?;

    Some((ppid, state, starttime))
}

fn read_cmdline(pid: u32) -> String {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return String::new();
    };

    // Argümanlar NUL ile ayrılmış ve sonda bir NUL var.
    String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn uptime_secs() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|raw| raw.split_whitespace().next()?.parse::<f64>().ok())
        .map(|secs| secs as u64)
        .unwrap_or(0)
}

/// Tüm süreçlerin pid → ppid eşlemesi.
fn parent_map() -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return map;
    };

    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue; // /proc'ta sayı olmayan girdiler de var.
        };
        if let Some((ppid, _, _)) = read_stat(pid) {
            map.insert(pid, ppid);
        }
    }

    map
}

/// `root`'un altındaki tüm pid'ler (kendisi hariç).
fn descendant_pids(root: u32) -> Vec<u32> {
    let parents = parent_map();

    // Ağacı tepeden kurmak yerine her süreçten köke yürüyoruz: /proc listesi
    // zaten tek geçişte okundu ve zincirler kısa.
    let mut out = Vec::new();
    for &pid in parents.keys() {
        if pid == root {
            continue;
        }

        let mut cursor = pid;
        // Zincir bir yerde kopabilir (süreç bu arada ölmüş olabilir) ya da
        // 1'e/0'a varır. Döngüye karşı adım sayısı sınırlı.
        for _ in 0..64 {
            let Some(&parent) = parents.get(&cursor) else {
                break;
            };
            if parent == root {
                out.push(pid);
                break;
            }
            if parent <= 1 {
                break;
            }
            cursor = parent;
        }
    }

    out.sort_unstable();
    out
}

/// Bir oturumun süreç ağacı, en yenisi sonda.
pub fn descendants(root: u32) -> Vec<Proc> {
    let now = uptime_secs();

    let mut out: Vec<Proc> = descendant_pids(root)
        .into_iter()
        .filter_map(|pid| {
            let (ppid, state, starttime) = read_stat(pid)?;
            let started = starttime / USER_HZ;

            Some(Proc {
                pid,
                ppid,
                command: read_cmdline(pid),
                state,
                elapsed_secs: now.saturating_sub(started),
            })
        })
        // Komut satırı okunamayan süreçler için gösterilecek bir şey yok.
        .filter(|p| !p.command.is_empty())
        .collect();

    out.sort_by_key(|p| std::cmp::Reverse(p.elapsed_secs));
    out
}

/// Bir alt süreci durdurur.
///
/// `root`'un soyundan olduğu **sinyal göndermeden önce** doğrulanıyor: pid
/// arayüzden geliyor ve arada süreç ölüp numarası başka bir sürece verilmiş
/// olabilir. Bu kontrol olmadan uygulama rastgele bir süreci öldürebilirdi.
pub fn kill(root: u32, pid: u32, force: bool) -> Result<(), EngineError> {
    if !descendant_pids(root).contains(&pid) {
        return Err(EngineError::Other(format!(
            "{pid} bu oturumun alt süreci değil"
        )));
    }

    let signal = if force { "-KILL" } else { "-TERM" };
    let program = ["/usr/bin/kill", "/bin/kill"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .ok_or_else(|| EngineError::Other("kill komutu bulunamadı".into()))?;

    let status = std::process::Command::new(program)
        .arg(signal)
        .arg(pid.to_string())
        .status()?;

    if !status.success() {
        return Err(EngineError::Other(format!("{pid} durdurulamadı")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kendi sürecimizin soyu okunabiliyor mu — /proc ayrıştırması burada
    /// kırılırsa panel hep boş görünürdü.
    ///
    /// Linux'a kapalı: `/bin/sh` ve `/proc` başka bir yerde yok. Modülün
    /// tamamı zaten `/proc` üzerine kurulu ve diğer platformlarda boş liste
    /// dönüyor — panel orada boş görünüyor, çökmüyor.
    #[cfg(target_os = "linux")]
    #[test]
    fn kendi_alt_surecimizi_gorur() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 5"])
            .spawn()
            .expect("alt süreç açılmalı");

        let me = std::process::id();

        // `spawn` döndüğünde çocuk /proc'ta görünmüş olmalı ama testler
        // paralel koşarken bu birkaç milisaniye gecikebiliyor; tek atışlık
        // kontrol ara sıra boşa düşüyordu.
        let mut found = Vec::new();
        for _ in 0..50 {
            found = descendants(me);
            if found.iter().any(|p| p.pid == child.id()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let entry = found.iter().find(|p| p.pid == child.id());
        assert!(
            entry.is_some(),
            "açılan alt süreç bulunamadı; görülenler: {found:#?}"
        );

        let entry = entry.unwrap();
        assert_eq!(entry.ppid, me);
        assert!(entry.command.contains("sleep 5"), "komut: {}", entry.command);

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Soydan olmayan bir pid'e sinyal gönderilmemeli. Bu test bilerek her
    /// platformda koşuyor: soy listesi boş dönen bir sistemde bile reddin
    /// tutması gerekiyor, aksi halde boş liste "her pid serbest" demek olurdu.
    #[test]
    fn yabanci_pid_reddedilir() {
        // 1 (init) hiçbir zaman bizim alt sürecimiz olamaz.
        let err = kill(std::process::id(), 1, false).unwrap_err();
        assert!(err.to_string().contains("alt süreci değil"));
    }

    /// `comm` alanı boşluk ve parantez içerebiliyor; ayrıştırma son `)`'e
    /// bakmazsa alanlar kayar.
    #[cfg(target_os = "linux")]
    #[test]
    fn stat_ayristirmasi_kendi_surecimizde_calisir() {
        let me = std::process::id();
        let (ppid, state, starttime) = read_stat(me).expect("kendi stat'ımız okunmalı");

        assert!(ppid > 0);
        assert!(!state.is_empty() && state.len() == 1, "durum: {state}");
        assert!(starttime > 0);
    }
}
