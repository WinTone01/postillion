//! Uygulama içi giriş akışı.
//!
//! `claude auth login` tarayıcıyı açıp URL'yi basıyor, sonra stdin'den kod
//! bekliyor:
//!
//! ```text
//! Opening browser to sign in…
//! If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?...
//! Paste code here if prompted >
//! ```
//!
//! Bu yüzden süreç ayrı bir terminale gerek kalmadan sürülebiliyor: URL'yi
//! yakalayıp arayüze yolluyoruz, kullanıcının yapıştırdığı kodu stdin'e
//! yazıyoruz.
//!
//! Giriş **geçici** bir yapılandırma dizininde yapılıyor. Aktif hesabı
//! bozmamak için: kullanıcı yeni hesap eklerken mevcut oturumu kaybetmemeli.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::accounts::Account;
use crate::error::{Error, Result};
use crate::paths;

pub const EVENT_URL: &str = "auth://url";
pub const EVENT_OUTPUT: &str = "auth://output";
pub const EVENT_DONE: &str = "auth://done";

#[derive(Clone, Serialize)]
struct UrlEvent {
    url: String,
}

#[derive(Clone, Serialize)]
struct OutputEvent {
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    ok: bool,
    account: Option<Account>,
    error: Option<String>,
}

struct Session {
    stdin: ChildStdin,
    child: Child,
    temp_dir: PathBuf,
}

#[derive(Default)]
pub struct Manager {
    session: Mutex<Option<Session>>,
}

impl Manager {
    /// Giriş akışını başlatır. URL `auth://url` ile geliyor.
    pub fn start(&self, app: AppHandle, email: Option<&str>) -> Result<()> {
        self.cancel()?;

        // Geçici dizin: giriş yarıda kalırsa aktif hesap etkilenmesin.
        let temp_dir = std::env::temp_dir().join(format!(
            "postillion-login-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp_dir)?;

        let mut cmd = Command::new(paths::claude_bin());
        cmd.arg("auth").arg("login");
        if let Some(email) = email {
            let trimmed = email.trim();
            if !trimmed.is_empty() {
                cmd.arg("--email").arg(trimmed);
            }
        }

        cmd.env("CLAUDE_CONFIG_DIR", &temp_dir)
            .env("PATH", paths::augmented_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Other(format!("giriş başlatılamadı: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Other("stdin alınamadı".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Other("stdout alınamadı".into()))?;

        let reader_app = app.clone();
        let reader_dir = temp_dir.clone();

        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buffer = Vec::new();

            // Satır satır okumak yetmiyor: son satır ("Paste code here >")
            // newline ile bitmiyor, bu yüzden byte byte okuyup hem satırları
            // hem yarım kalan istemi yakalıyoruz.
            let mut line = String::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) => break,
                    Err(_) => break,
                    Ok(_) => {}
                }
                line = String::from_utf8_lossy(&buffer).to_string();
                let trimmed = line.trim_end().to_string();

                if let Some(url) = extract_url(&trimmed) {
                    let _ = reader_app.emit(EVENT_URL, UrlEvent { url });
                }
                if !trimmed.is_empty() {
                    let _ = reader_app.emit(EVENT_OUTPUT, OutputEvent { line: trimmed });
                }
            }
            drop(line);

            // Süreç bitti; jeton oluştuysa profil olarak sakla.
            let result = crate::accounts::adopt(&reader_dir);
            let payload = match result {
                Ok(account) => DoneEvent {
                    ok: true,
                    account: Some(account),
                    error: None,
                },
                Err(e) => DoneEvent {
                    ok: false,
                    account: None,
                    error: Some(e.to_string()),
                },
            };
            let _ = reader_app.emit(EVENT_DONE, payload);
            let _ = std::fs::remove_dir_all(&reader_dir);
        });

        // stderr yalnızca teşhis için.
        if let Some(stderr) = child.stderr.take() {
            let app = app.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                    if !line.trim().is_empty() {
                        let _ = app.emit(EVENT_OUTPUT, OutputEvent { line });
                    }
                }
            });
        }

        *self.session.lock().unwrap() = Some(Session {
            stdin,
            child,
            temp_dir,
        });

        Ok(())
    }

    /// Kullanıcının yapıştırdığı kodu sürece iletir.
    pub fn submit_code(&self, code: &str) -> Result<()> {
        let mut guard = self.session.lock().unwrap();
        let session = guard
            .as_mut()
            .ok_or_else(|| Error::Other("etkin giriş akışı yok".into()))?;

        writeln!(session.stdin, "{}", code.trim())?;
        session.stdin.flush()?;
        Ok(())
    }

    pub fn cancel(&self) -> Result<()> {
        if let Some(mut session) = self.session.lock().unwrap().take() {
            let _ = session.child.kill();
            let _ = session.child.wait();
            let _ = std::fs::remove_dir_all(&session.temp_dir);
        }
        Ok(())
    }
}

/// Satırdan OAuth adresini çıkarır.
fn extract_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    // URL satırın sonuna kadar sürüyor; olası noktalama temizleniyor.
    let end = rest
        .find(char::is_whitespace)
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches(['.', ',', ')']).to_string();
    if url.len() > "https://".len() {
        Some(url)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_satirdan_cikarilir() {
        let line = "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=abc&state=xyz";
        let url = extract_url(line).expect("url bulunmalı");
        assert!(url.starts_with("https://claude.com/cai/oauth/authorize"));
        assert!(url.contains("client_id=abc"));
        // Sorgu dizesi kırpılmamalı.
        assert!(url.ends_with("state=xyz"));
    }

    #[test]
    fn url_olmayan_satir_none_doner() {
        assert!(extract_url("Opening browser to sign in…").is_none());
        assert!(extract_url("Paste code here if prompted >").is_none());
    }

    #[test]
    fn sondaki_noktalama_atilir() {
        assert_eq!(
            extract_url("visit https://ornek.com/x.").unwrap(),
            "https://ornek.com/x"
        );
    }
}
