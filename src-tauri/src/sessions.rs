//! Transcript tarama.
//!
//! Transcript'ler `~/.claude/projects/<cwd-slug>/<sessionId>.jsonl` altında
//! append-only JSONL olarak duruyor. Bu makinede 129 dosya / 412 MB var, yani
//! naif "hepsini oku" yaklaşımı UI'ı kilitler.
//!
//! İki optimizasyon:
//!   1. Büyük dosyalarda baş + son parçayı okuruz (aşağıda gerekçesi).
//!   2. (yol, mtime, boyut) anahtarlı cache — değişmemiş dosya yeniden okunmaz.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::paths;

/// Bu boyutun altındaki dosyalar tamamen okunur.
const FULL_READ_LIMIT: u64 = 1 << 20; // 1 MiB
/// Büyük dosyalarda baştan okunan miktar (sessionId, cwd, ilk mesaj burada).
const HEAD_BYTES: u64 = 128 << 10;
/// Büyük dosyalarda sondan okunan miktar (güncel başlık ve zaman burada).
const TAIL_BYTES: u64 = 256 << 10;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    pub path: PathBuf,
    /// Oturumun açıldığı çalışma dizini — `claude` bu dizinden başlatılmalı.
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// custom-title > ai-title > last-prompt > ilk kullanıcı mesajı
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub model: Option<String>,
    pub size_bytes: u64,
    /// Dosyanın mtime'ı (unix ms) — sıralama buna göre.
    pub modified_ms: u64,
}

#[derive(Default)]
pub struct Cache {
    inner: Mutex<HashMap<PathBuf, (u64, u64, Session)>>,
}

impl Cache {
    /// Paylaşılan `projects/` dizinindeki tüm oturumlar, en yeniden eskiye.
    pub fn scan(&self, project_filter: Option<&str>) -> Result<Vec<Session>> {
        let root = paths::shared_projects_dir()?;
        let mut out = Vec::new();

        if !root.is_dir() {
            return Ok(out);
        }

        for project in fs::read_dir(&root)?.filter_map(|e| e.ok()) {
            let project_path = project.path();
            if !project_path.is_dir() {
                continue;
            }
            if let Some(filter) = project_filter {
                let name = project.file_name().to_string_lossy().into_owned();
                if name != filter {
                    continue;
                }
            }

            // Kasıtlı olarak tek seviye derine iniyoruz.
            //
            // `<proje>/<sessionId>/subagents/agent-*.jsonl` altında alt-ajan
            // transcript'leri var (bu makinede 21 tane). Bunlar bağımsız oturum
            // değil, üst oturumun yan dalları — `--resume` ile açılamazlar ve
            // listede görünmemeleri gerekir. Özyinelemeli tarama onları
            // yanlışlıkla oturum sanardı.
            for file in fs::read_dir(&project_path)?.filter_map(|e| e.ok()) {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                match self.load(&path) {
                    Ok(session) => out.push(session),
                    // Tek bozuk dosya tüm listeyi düşürmesin.
                    Err(_) => continue,
                }
            }
        }

        out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        Ok(out)
    }

    fn load(&self, path: &Path) -> Result<Session> {
        let meta = fs::metadata(path)?;
        let len = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Ok(cache) = self.inner.lock() {
            if let Some((c_mtime, c_len, session)) = cache.get(path) {
                // Append-only dosyalar; mtime+boyut aynıysa içerik de aynıdır.
                if *c_mtime == mtime && *c_len == len {
                    return Ok(session.clone());
                }
            }
        }

        let session = parse(path, len, mtime)?;

        if let Ok(mut cache) = self.inner.lock() {
            cache.insert(path.to_path_buf(), (mtime, len, session.clone()));
        }
        Ok(session)
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.clear();
        }
    }
}

/// Transcript'in sohbet kayıtlarını sırayla döndürür.
///
/// Neden gerekli: `claude --resume` oturumu yalnızca *model tarafında* geri
/// yüklüyor; geçmiş mesajları stdout'a tekrar basmıyor (ölçüldü: boş stdin ile
/// sıfır satır çıktı). Dolayısıyla arayüzdeki geçmişi diskten biz doldurmak
/// zorundayız.
///
/// Kayıtlar canlı stream-json ile aynı şekle sahip (`type` + `message`), bu
/// yüzden arayüz tarafında aynı reducer'dan geçirilebiliyorlar.
pub fn read_transcript(path: &Path, max_records: usize) -> Result<Vec<Value>> {
    let file = File::open(path)?;
    // Uzun oturumlarda tamamını IPC'den geçirmek anlamsız; son N kayıt yeterli.
    let mut kept: VecDeque<Value> = VecDeque::with_capacity(max_records.min(1024));

    for line in BufReader::new(file).lines().map_while(|l| l.ok()) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if !is_conversation_record(&value) {
            continue;
        }

        if kept.len() == max_records {
            kept.pop_front();
        }
        kept.push_back(value);
    }

    Ok(kept.into())
}

/// Yalnızca sohbetin kendisi: kullanıcı ve asistan mesajları.
///
/// Elenenler: başlık/kuyruk/ek kayıtları, `system` girdileri ve **alt-ajan
/// yan dalları** (`isSidechain`). Sonuncusu önemli — alt-ajan konuşmaları
/// ana sohbete karışırsa geçmiş okunamaz hale gelir.
fn is_conversation_record(value: &Value) -> bool {
    if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    // Sisteme enjekte edilmiş kayıtlar (hatırlatmalar, bağlam notları) sohbetin
    // parçası değil; kullanıcı onları hiç yazmadı.
    if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return false;
    }

    match value.get("type").and_then(Value::as_str) {
        Some("user") | Some("assistant") => value.get("message").is_some(),
        _ => false,
    }
}

fn parse(path: &Path, len: u64, modified_ms: u64) -> Result<Session> {
    let session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut s = Session {
        session_id,
        path: path.to_path_buf(),
        cwd: None,
        git_branch: None,
        title: None,
        last_prompt: None,
        model: None,
        size_bytes: len,
        modified_ms,
    };

    let mut first_user_message: Option<String> = None;
    let mut custom_title: Option<String> = None;
    let mut ai_title: Option<String> = None;

    let mut apply = |line: &str| {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let str_at = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);

        // Başlık kayıtları dosya boyunca tekrar eder; son görülen kazanır.
        match v.get("type").and_then(|t| t.as_str()) {
            Some("custom-title") => custom_title = str_at("customTitle").or(custom_title.take()),
            Some("ai-title") => ai_title = str_at("aiTitle").or(ai_title.take()),
            Some("last-prompt") => s.last_prompt = str_at("lastPrompt").or(s.last_prompt.take()),
            Some("user") => {
                if first_user_message.is_none() {
                    if let Some(text) = v
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        first_user_message = Some(text.to_string());
                    }
                }
            }
            Some("assistant") => {
                if let Some(model) = v
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .and_then(|m| m.as_str())
                {
                    s.model = Some(model.to_string());
                }
            }
            _ => {}
        }

        // cwd ve gitBranch birden çok kayıt tipinde bulunur.
        if s.cwd.is_none() {
            s.cwd = str_at("cwd");
        }
        if s.git_branch.is_none() {
            s.git_branch = str_at("gitBranch");
        }
    };

    if len <= FULL_READ_LIMIT {
        for line in BufReader::new(File::open(path)?).lines().map_while(|l| l.ok()) {
            apply(&line);
        }
    } else {
        for line in read_head(path, HEAD_BYTES)? {
            apply(&line);
        }
        for line in read_tail(path, len, TAIL_BYTES)? {
            apply(&line);
        }
    }

    s.title = custom_title
        .or(ai_title)
        .or_else(|| s.last_prompt.clone())
        .or(first_user_message)
        .map(|t| truncate(&t, 120));

    Ok(s)
}

fn read_head(path: &Path, bytes: u64) -> Result<Vec<String>> {
    let mut buf = vec![0u8; bytes as usize];
    let mut f = File::open(path)?;
    let n = f.read(&mut buf)?;
    buf.truncate(n);

    let text = String::from_utf8_lossy(&buf);
    // Son satır kesilmiş olabilir; onu atıyoruz.
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines.pop();
    Ok(lines)
}

fn read_tail(path: &Path, len: u64, bytes: u64) -> Result<Vec<String>> {
    let start = len.saturating_sub(bytes);
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(start))?;

    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;

    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    // İlk satır neredeyse kesin yarım kalmıştır (rastgele offset'ten okuduk).
    if start > 0 {
        lines.next();
    }
    Ok(lines.map(str::to_string).collect())
}

fn truncate(s: &str, max: usize) -> String {
    let clean = s.replace(['\n', '\r'], " ");
    let trimmed = clean.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    // char sınırında kes — Türkçe karakterler çok baytlı.
    let cut: String = trimmed.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_jsonl(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("po-sess-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn son_baslik_kazanir() {
        // Gerçek transcript'lerde başlık kayıtları tekrar eder.
        let path = tmp_jsonl(
            "abc-123.jsonl",
            r#"{"type":"user","message":{"role":"user","content":"ilk mesaj"},"cwd":"/tmp/proje","gitBranch":"main"}
{"type":"ai-title","aiTitle":"eski baslik"}
{"type":"custom-title","customTitle":"guncel baslik"}
{"type":"assistant","message":{"model":"claude-opus-5"}}
"#,
        );

        let s = parse(&path, 100, 0).unwrap();
        assert_eq!(s.session_id, "abc-123");
        assert_eq!(s.title.as_deref(), Some("guncel baslik"));
        assert_eq!(s.cwd.as_deref(), Some("/tmp/proje"));
        assert_eq!(s.git_branch.as_deref(), Some("main"));
        assert_eq!(s.model.as_deref(), Some("claude-opus-5"));
    }

    #[test]
    fn baslik_yoksa_ilk_mesaja_duser() {
        let path = tmp_jsonl(
            "def-456.jsonl",
            r#"{"type":"user","message":{"role":"user","content":"başlıksız oturum"}}
"#,
        );
        let s = parse(&path, 60, 0).unwrap();
        assert_eq!(s.title.as_deref(), Some("başlıksız oturum"));
    }

    #[test]
    fn bozuk_satir_atlanir() {
        let path = tmp_jsonl(
            "ghi-789.jsonl",
            "{bu json degil\n{\"type\":\"custom-title\",\"customTitle\":\"saglam\"}\n",
        );
        let s = parse(&path, 60, 0).unwrap();
        assert_eq!(s.title.as_deref(), Some("saglam"));
    }

    #[test]
    fn turkce_kesme_char_sinirinda() {
        let uzun = "ş".repeat(200);
        let out = truncate(&uzun, 120);
        assert_eq!(out.chars().count(), 121); // 120 + elips
        assert!(out.ends_with('…'));
    }
}
