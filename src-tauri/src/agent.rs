//! Claude Code'u headless ajan olarak sürer.
//!
//! Terminal yansıtmak yerine `claude`'u stream-json modunda çalıştırıp
//! yapılandırılmış event akışını arayüze taşıyoruz:
//!
//! ```text
//!   claude -p --input-format stream-json --output-format stream-json
//!          --include-partial-messages --verbose
//!          --permission-prompt-tool stdio --permission-mode manual
//! ```
//!
//! `--permission-prompt-tool stdio` `--help`'te listelenmiyor ama Agent SDK'nın
//! `canUseTool` geri çağrımını da bu bayrak besliyor. Onsuz izin istekleri
//! sessizce reddediliyor (deneyle doğrulandı). Bununla birlikte CLI bize
//! `control_request/can_use_tool` gönderiyor ve cevabımızı bekliyor — izin
//! diyaloğunu mümkün kılan tek şey bu.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::error::{Error, Result};

pub const EVENT_MESSAGE: &str = "agent://event";
pub const EVENT_EXIT: &str = "agent://exit";
pub const EVENT_STDERR: &str = "agent://stderr";

/// Handshake isteğinin sabit kimliği; arayüz cevabı bununla tanıyor.
pub const INIT_REQUEST_ID: &str = "cs-initialize";

#[derive(Clone, Serialize)]
struct AgentEvent {
    id: String,
    payload: Value,
}

#[derive(Clone, Serialize)]
struct ExitEvent {
    id: String,
    code: Option<i32>,
}

#[derive(Clone, Serialize)]
struct StderrEvent {
    id: String,
    line: String,
}

struct Handle {
    stdin: ChildStdin,
    child: Child,
    /// Oturuma özel MCP yapılandırması; kapanışta siliniyor.
    mcp_config: Option<std::path::PathBuf>,
}

/// Kullanıcı mesajına iliştirilen görüntü.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    /// `image/png`, `image/jpeg`, `image/gif`, `image/webp`.
    pub media_type: String,
    /// Base64; ham ikili veri IPC'den geçemiyor.
    pub data: String,
}

/// Kullanıcı mesajının içerik alanını kurar.
///
/// Görüntü yoksa düz metin — Claude Code'un beklediği en sade biçim. Görüntü
/// varsa blok dizisi; görüntüler metnin önünde, çünkü soru genelde görüntüye
/// atıfta bulunuyor. (Ölçüldü: base64 `image` blokları stream-json girdisinde
/// çalışıyor.)
fn user_content(text: &str, images: &[Image]) -> Value {
    if images.is_empty() {
        return json!(text);
    }

    let mut blocks: Vec<Value> = images
        .iter()
        .map(|image| {
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": image.media_type,
                    "data": image.data,
                }
            })
        })
        .collect();

    // Yalnızca görüntüden oluşan bir mesaj da geçerli; boş metin bloğu
    // eklemek API tarafında hata veriyor.
    if !text.is_empty() {
        blocks.push(json!({ "type": "text", "text": text }));
    }

    json!(blocks)
}

/// Seçilen MCP sunucularını geçici bir dosyaya yazar.
///
/// Adlar arayüzden geliyor ama tanımlar kullanıcının kendi yapılandırmasından
/// okunuyor; bilinmeyen bir ad sessizce atlanıyor (silinmiş olabilir).
fn write_mcp_config(session_id: &str, names: &[String]) -> Result<std::path::PathBuf> {
    let defined = crate::catalog::mcp_definitions(None)?;

    let mut servers = serde_json::Map::new();
    for name in names {
        if let Some(def) = defined.get(name) {
            servers.insert(name.clone(), def.clone());
        }
    }

    // Dosya adı oturum kimliğinden; kimlik zaten benzersiz ve dosya sistemi
    // için güvenli hale getiriliyor.
    let safe: String = session_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let path = std::env::temp_dir().join(format!("postillion-mcp-{safe}.json"));

    // Tanımlar API anahtarı taşıyabiliyor ve /tmp paylaşılan bir dizin.
    // `create_new` + 0600: dosya yalnızca bu kullanıcı tarafından okunabilsin
    // ve önceden var olan (belki başkasına ait) bir dosyaya yazılmasın.
    let _ = std::fs::remove_file(&path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;

    file.write_all(&serde_json::to_vec(&json!({ "mcpServers": servers }))?)?;
    Ok(path)
}

#[derive(Default)]
pub struct Manager {
    sessions: Mutex<HashMap<String, Handle>>,
    /// İstemci kaynaklı kontrol isteklerinin request_id sayacı.
    next_request: AtomicU64,
}

pub struct StartOptions<'a> {
    pub id: String,
    /// Claude'un başlatılacağı dizin. `resume` verildiyse oturumun kendi
    /// cwd'si olmak zorunda: Claude transcript'leri cwd'ye göre dizinliyor,
    /// yanlış dizinden başlatılırsa oturumu bulamaz.
    pub cwd: Option<&'a Path>,
    pub resume: Option<&'a str>,
    pub model: Option<&'a str>,
    /// low | medium | high | xhigh | max.
    ///
    /// Modelin aksine çalışma anında değiştirilemiyor (`set_effort` diye bir
    /// kontrol isteği yok), yalnızca başlatırken veriliyor.
    pub effort: Option<&'a str>,
    /// Bu oturumun kullanacağı MCP sunucularının adları.
    ///
    /// `None` genel yapılandırma demek. Liste verildiğinde geçici bir dosyaya
    /// yazılıp `--mcp-config` ile veriliyor ve `--strict-mcp-config` diğer
    /// kaynakları kapatıyor.
    pub mcp_servers: Option<&'a [String]>,
}

impl Manager {
    pub fn start(&self, app: AppHandle, opts: StartOptions<'_>) -> Result<String> {
        if self.sessions.lock().unwrap().contains_key(&opts.id) {
            return Err(Error::Other(format!("oturum zaten açık: {}", opts.id)));
        }

        // Mutlak yol: menüden başlatıldığında PATH `~/.local/bin` içermiyor.
        let mut cmd = Command::new(crate::paths::claude_bin());
        cmd.arg("-p")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--output-format")
            .arg("stream-json")
            // Token-token akış; olmadan yalnızca tamamlanmış mesajlar gelir.
            .arg("--include-partial-messages")
            // stream-json çıktısı için gerekli.
            .arg("--verbose")
            // İzin isteklerini kontrol kanalına yönlendiren gizli bayrak.
            .arg("--permission-prompt-tool")
            .arg("stdio")
            // Hiçbir şeyi kendiliğinden onaylama; her şeyi bize sor.
            .arg("--permission-mode")
            .arg("manual");

        if let Some(session) = opts.resume {
            cmd.arg("--resume").arg(session);
        }
        if let Some(model) = opts.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = opts.effort {
            cmd.arg("--effort").arg(effort);
        }

        // Sunucu tanımları burada, arayüzde değil kuruluyor: tanımlar API
        // anahtarı taşıyabiliyor ve gizli değerlerin frontend'e geçmemesi
        // katalogda korunan bir kural.
        let mcp_config = match opts.mcp_servers {
            Some(names) => Some(write_mcp_config(&opts.id, names)?),
            None => None,
        };
        if let Some(path) = &mcp_config {
            cmd.arg("--mcp-config").arg(path);
            cmd.arg("--strict-mcp-config");
        }

        // Claude kendi alt süreçlerini (node, git, ripgrep) PATH'ten arıyor;
        // onu mutlak yolla başlatmak kendi bulunmasını çözer, komşularını değil.
        cmd.env("PATH", crate::paths::augmented_path());

        if let Some(cwd) = opts.cwd {
            if cwd.is_dir() {
                cmd.current_dir(cwd);
            }
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Other(format!("claude başlatılamadı: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Other("stdin alınamadı".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Other("stdout alınamadı".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Other("stderr alınamadı".into()))?;

        // stdout: satır başına bir JSON event.
        {
            let id = opts.id.clone();
            let app = app.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(&line) {
                        Ok(payload) => {
                            let _ = app.emit(
                                EVENT_MESSAGE,
                                AgentEvent {
                                    id: id.clone(),
                                    payload,
                                },
                            );
                        }
                        // Bozuk satır akışı durdurmasın; teşhis için stderr
                        // kanalından bildir.
                        Err(_) => {
                            let _ = app.emit(
                                EVENT_STDERR,
                                StderrEvent {
                                    id: id.clone(),
                                    line: format!("ayrıştırılamayan satır: {line}"),
                                },
                            );
                        }
                    }
                }

                let _ = app.emit(EVENT_EXIT, ExitEvent { id, code: None });
            });
        }

        // stderr: teşhis. Sessizce yutmak hata ayıklamayı imkansızlaştırır.
        {
            let id = opts.id.clone();
            let app = app.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                    let _ = app.emit(
                        EVENT_STDERR,
                        StderrEvent {
                            id: id.clone(),
                            line,
                        },
                    );
                }
            });
        }

        self.sessions
            .lock()
            .unwrap()
            .insert(
                opts.id.clone(),
                Handle {
                    stdin,
                    child,
                    mcp_config,
                },
            );

        // Handshake: cevabı slash komut listesini, model listesini ve
        // alt-ajanları taşıyor. Bunları başka hiçbir yerden öğrenemiyoruz —
        // `claude` bu bilgiyi CLI'dan sunmuyor.
        self.write_line(
            &opts.id,
            &json!({
                "type": "control_request",
                "request_id": INIT_REQUEST_ID,
                "request": { "subtype": "initialize", "hooks": {} }
            }),
        )?;

        Ok(opts.id)
    }

    fn write_line(&self, id: &str, value: &Value) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        let handle = sessions
            .get_mut(id)
            .ok_or_else(|| Error::SessionNotFound(id.to_string()))?;

        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        handle.stdin.write_all(line.as_bytes())?;
        handle.stdin.flush()?;
        Ok(())
    }

    /// Kullanıcı mesajı gönderir.
    pub fn send_user_message(&self, id: &str, text: &str, images: &[Image]) -> Result<()> {
        self.write_line(
            id,
            &json!({
                "type": "user",
                "message": { "role": "user", "content": user_content(text, images) }
            }),
        )
    }

    /// `can_use_tool` kontrol isteğine cevap verir.
    ///
    /// `updated_input` verilirse Claude aracı o girdiyle çalıştırır — kullanıcı
    /// izin diyaloğunda parametreyi düzenlerse bu kullanılır.
    pub fn respond_permission(
        &self,
        id: &str,
        request_id: &str,
        allow: bool,
        updated_input: Option<Value>,
        message: Option<&str>,
    ) -> Result<()> {
        let response = if allow {
            let mut body = json!({ "behavior": "allow" });
            if let Some(input) = updated_input {
                body["updatedInput"] = input;
            }
            body
        } else {
            json!({
                "behavior": "deny",
                "message": message.unwrap_or("Kullanıcı reddetti"),
            })
        };

        self.write_line(
            id,
            &json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": response,
                }
            }),
        )
    }

    /// İzin modunu değiştirir — `permission_suggestions` içindeki
    /// `{"type":"setMode","mode":"acceptEdits"}` önerisini uygular.
    pub fn set_permission_mode(&self, id: &str, mode: &str) -> Result<()> {
        let request_id = format!("cs-{}", self.next_request.fetch_add(1, Ordering::Relaxed));
        self.write_line(
            id,
            &json!({
                "type": "control_request",
                "request_id": request_id,
                "request": { "subtype": "set_permission_mode", "mode": mode }
            }),
        )
    }

    /// Modeli çalışma anında değiştirir — süreç yeniden başlatılmıyor,
    /// dolayısıyla sohbet bağlamı korunuyor.
    pub fn set_model(&self, id: &str, model: &str) -> Result<()> {
        let request_id = format!("cs-{}", self.next_request.fetch_add(1, Ordering::Relaxed));
        self.write_line(
            id,
            &json!({
                "type": "control_request",
                "request_id": request_id,
                "request": { "subtype": "set_model", "model": model }
            }),
        )
    }

    /// Süren turu kesintiye uğratır.
    pub fn interrupt(&self, id: &str) -> Result<()> {
        let request_id = format!("cs-{}", self.next_request.fetch_add(1, Ordering::Relaxed));
        self.write_line(
            id,
            &json!({
                "type": "control_request",
                "request_id": request_id,
                "request": { "subtype": "interrupt" }
            }),
        )
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(mut handle) = sessions.remove(id) {
            // stdin'i kapatmak Claude'a düzgün çıkış şansı verir.
            drop(handle.stdin);
            let _ = handle.child.kill();
            let _ = handle.child.wait();
            if let Some(path) = handle.mcp_config {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }

    /// Oturumun `claude` süreç kimliği; süreç ağacı buradan çıkarılıyor.
    pub fn session_pid(&self, id: &str) -> Option<u32> {
        self.sessions.lock().unwrap().get(id).map(|h| h.child.id())
    }

    pub fn active_ids(&self) -> Vec<String> {
        self.sessions.lock().unwrap().keys().cloned().collect()
    }
}

pub type SharedManager = Arc<Manager>;

#[cfg(test)]
mod tests {
    use super::*;

    /// İzin cevabının şekli protokole birebir uymak zorunda; yanlış şekilde
    /// CLI sessizce bekler ve arayüz kilitlenir.
    #[test]
    fn izin_cevabi_protokole_uygun() {
        let allow = json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": "abc",
                "response": { "behavior": "allow", "updatedInput": {"a": 1} }
            }
        });

        assert_eq!(allow["type"], "control_response");
        assert_eq!(allow["response"]["subtype"], "success");
        assert_eq!(allow["response"]["request_id"], "abc");
        assert_eq!(allow["response"]["response"]["behavior"], "allow");
    }

    #[test]
    fn red_cevabi_mesaj_tasir() {
        let deny = json!({
            "behavior": "deny",
            "message": "Kullanıcı reddetti",
        });
        assert_eq!(deny["behavior"], "deny");
        assert!(deny["message"].as_str().unwrap().contains("reddetti"));
    }

    fn png(data: &str) -> Image {
        Image {
            media_type: "image/png".into(),
            data: data.into(),
        }
    }

    /// Görüntüsüz mesaj düz metin kalmalı: blok dizisine sarmak da geçerli ama
    /// transcript'i gereksiz yere şişiriyor ve geçmiş okuyucusu düz metni
    /// gerçek kullanıcı mesajının işareti olarak kullanıyor.
    #[test]
    fn goruntusuz_mesaj_duz_metin() {
        assert_eq!(user_content("merhaba", &[]), json!("merhaba"));
    }

    #[test]
    fn goruntuler_metnin_onunde_gonderilir() {
        let content = user_content("bu ne?", &[png("AAAA"), png("BBBB")]);
        let blocks = content.as_array().unwrap();

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert_eq!(blocks[0]["source"]["data"], "AAAA");
        assert_eq!(blocks[1]["source"]["data"], "BBBB");
        assert_eq!(blocks[2], json!({ "type": "text", "text": "bu ne?" }));
    }

    /// Yalnızca görüntü gönderilebilmeli; boş metin bloğu API tarafında hata.
    #[test]
    fn bos_metin_blok_uretmez() {
        let content = user_content("", &[png("AAAA")]);
        let blocks = content.as_array().unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
    }
}
