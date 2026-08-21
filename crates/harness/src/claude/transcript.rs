//! Yerel Claude Code transcript'leri: tarama, özet çıkarma, olay okuma.
//!
//! Claude Code her oturumu `~/.claude/projects/<cwd-slug>/<sessionId>.jsonl`
//! altında append-only JSONL olarak tutuyor. Postillion 1 bu dosyaları
//! doğrudan oturum listesi olarak gösteriyordu; v2'de sohbetler Loro
//! dokümanı olduğu için burası yalnızca **okuma** tarafını veriyor:
//!
//!   - [`transcript_paths`] hangi dosyaların oturum sayıldığını bilir,
//!   - [`parse_summary`] listeyi çizecek kadarını (başlık, cwd, dal, model)
//!     dosyanın tamamını okumadan çıkarır,
//!   - [`read_events`] konuşmayı motorun zaten kullandığı [`AgentEvent`]
//!     diline çevirir; böylece içe aktarılan geçmiş, canlı bir turun ürettiği
//!     mesajlarla birebir aynı parçalardan oluşur.
//!
//! Önbellek ve "hangisi zaten içe aktarıldı" bilgisi burada değil, motorda:
//! bu modül durumsuz ve dosya sisteminden başka hiçbir şeye bakmıyor.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use postillion_proto::AgentEvent;

use super::normalize::decode_tool_use;

/// Bu boyutun altındaki dosyalar özet için tamamen okunur.
const FULL_READ_LIMIT: u64 = 1 << 20; // 1 MiB
/// Büyük dosyalarda baştan okunan miktar (sessionId, cwd, ilk mesaj burada).
const HEAD_BYTES: u64 = 128 << 10;
/// Büyük dosyalarda sondan okunan miktar (güncel başlık ve zaman burada).
const TAIL_BYTES: u64 = 256 << 10;
/// Başlık kırpma sınırı — liste satırı bundan uzununu zaten göstermiyor.
const TITLE_CHARS: usize = 120;

/// Listede gösterilecek kadarıyla bir yerel oturum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTranscript {
    /// Claude'un oturum kimliği — `--resume` bunu istiyor.
    pub session_id: String,
    pub path: PathBuf,
    /// Oturumun açıldığı çalışma dizini. `--resume` cwd'ye bağlı olduğu için
    /// içe aktarılan sohbetin cwd'si de bu olmak zorunda.
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// custom-title > ai-title > last-prompt > ilk kullanıcı mesajı
    pub title: Option<String>,
    pub model: Option<String>,
    /// Gerçek bir konuşma içeriyor mu.
    ///
    /// Yalnızca yerel komut kaydı taşıyan transcript'ler var: `claude -p
    /// "/usage"` gibi her çağrı bir tane bırakıyor. Bunlar oturum değil,
    /// listede yer kaplamamalılar.
    pub has_conversation: bool,
    pub size_bytes: u64,
    /// Dosyanın mtime'ı (unix ms) — sıralama buna göre.
    pub modified_ms: u64,
}

/// Claude'un yapılandırma dizini (`$CLAUDE_CONFIG_DIR` ya da `~/.claude`).
fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    super::mcp::dirs_home().map(|home| home.join(".claude"))
}

/// Transcript kökü. Hesap değiştirmek bu dizini taşımıyor — oturumlar
/// hesaplar arasında ortak (v1'in `shared_projects_dir`'ı).
pub fn projects_dir() -> Option<PathBuf> {
    Some(config_dir()?.join("projects"))
}

/// Bir çalışma dizininin transcript klasörü adı.
///
/// Claude yolu düzleştiriyor: `/` ve `.` tire oluyor, yani
/// `/home/ali/Projects/x` → `-home-ali-Projects-x`.
///
/// Windows'ta ters bölü ve sürücü iki noktası da düzleşiyor:
/// `C:\Users\ali\x` → `C--Users-ali-x`. Bu ikisi yalnızca Windows'ta
/// eşleniyor — `:` unix'te geçerli bir dizin adı karakteri ve orada Claude
/// onu olduğu gibi bırakıyor, körlemesine eşlemek klasörü ıskalatırdı.
pub fn project_slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| match c {
            '/' | '.' => '-',
            #[cfg(windows)]
            '\\' | ':' => '-',
            other => other,
        })
        .collect()
}

/// `root` altındaki oturum transcript'lerinin yolları.
///
/// Kasıtlı olarak tek seviye derine iniyoruz.
/// `<proje>/<sessionId>/subagents/agent-*.jsonl` altında alt-ajan
/// transcript'leri var; bunlar bağımsız oturum değil, üst oturumun yan
/// dalları — `--resume` ile açılamazlar ve listede görünmemeleri gerekir.
pub fn transcript_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return out;
    };
    for project in projects.filter_map(|e| e.ok()) {
        let project_path = project.path();
        if !project_path.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for file in files.filter_map(|e| e.ok()) {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if path.is_file() {
                out.push(path);
            }
        }
    }
    out
}

/// Listeyi çizecek kadarını çıkar.
///
/// Büyük dosyalarda baş + son parçayı okuruz: kimlik ve cwd baştaki
/// kayıtlarda, güncel başlık ve son istem sondakilerde. Bu makinelerde
/// transcript kökü yüz megabaytları buluyor; naif "hepsini oku" taraması
/// arayüzü kilitler.
pub fn parse_summary(path: &Path, len: u64, modified_ms: u64) -> std::io::Result<LocalTranscript> {
    let session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut s = LocalTranscript {
        session_id,
        path: path.to_path_buf(),
        cwd: None,
        git_branch: None,
        title: None,
        model: None,
        has_conversation: false,
        size_bytes: len,
        modified_ms,
    };

    let mut first_user_message: Option<String> = None;
    let mut custom_title: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut last_prompt: Option<String> = None;

    let mut apply = |line: &str| {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let str_at = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);

        // Başlık kayıtları dosya boyunca tekrar eder; son görülen kazanır.
        match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => custom_title = str_at("customTitle").or(custom_title.take()),
            Some("ai-title") => ai_title = str_at("aiTitle").or(ai_title.take()),
            Some("last-prompt") => last_prompt = str_at("lastPrompt").or(last_prompt.take()),
            Some("user") => {
                // `isMeta` kayıtları arayüzün ürettiği sarmalayıcılar; başlık
                // olarak kullanılırlarsa liste "<local-command-caveat>Caveat:
                // The messages below were generated…" diye doluyor.
                if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
                    return;
                }
                match v.get("message").and_then(|m| m.get("content")) {
                    Some(Value::String(text)) => {
                        if is_wrapper_text(text) {
                            return;
                        }
                        s.has_conversation = true;
                        if first_user_message.is_none() {
                            first_user_message = Some(text.clone());
                        }
                    }
                    // Görüntü ya da araç sonucu taşıyan dizi biçimi. Yalnızca
                    // araç sonucu taşıyan kayıtlar konuşma sayılmaz: onlar
                    // asistanın turunun parçası.
                    Some(Value::Array(blocks)) => {
                        let text = user_text_blocks(blocks);
                        if blocks.iter().any(|b| block_kind(b) != "tool_result") {
                            s.has_conversation = true;
                        }
                        if let Some(text) = text.filter(|t| !is_wrapper_text(t))
                            && first_user_message.is_none()
                        {
                            first_user_message = Some(text);
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                if let Some(model) = v
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                {
                    // Claude yerel ürettiği kayıtlara `<synthetic>` yazıyor;
                    // rozette model adı olarak görünüyordu.
                    if !model.starts_with('<') {
                        s.model = Some(model.to_string());
                    }
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
        for line in BufReader::new(File::open(path)?).lines().map_while(Result::ok) {
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
        .or(last_prompt)
        .or(first_user_message)
        .map(|t| truncate(&t, TITLE_CHARS));

    Ok(s)
}

/// Kaydın kendi zaman damgasıyla birlikte tek bir olay.
///
/// Damga transcript'teki ISO-8601 dizesi (`timestamp`) — ayrıştırmak
/// çağıranın işi; bu crate'in takvim bağımlılığı yok.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptEvent {
    pub timestamp: Option<String>,
    pub event: AgentEvent,
}

/// Transcript'i motorun dilinde okur: sohbetin kendisi, sırayla.
///
/// Kullanıcı mesajları [`AgentEvent::UserMessage`], asistan metni
/// [`AgentEvent::TextDelta`], araçlar [`AgentEvent::ToolCall`] +
/// [`AgentEvent::ToolResult`] olarak çıkıyor — yani motorun `fold_event_into_parts`
/// katlaması bu olayları canlı bir turdan gelmiş gibi işleyebiliyor.
///
/// Elenenler: alt-ajan yan dalları (`isSidechain`), sisteme enjekte edilmiş
/// kayıtlar (`isMeta`) ve arayüzün ürettiği `<command-…>` sarmalayıcıları.
/// `max_records` en yeni N kaydı tutar: çok uzun oturumların tamamını
/// dokümana yazmanın anlamı yok.
pub fn read_events(path: &Path, max_records: usize) -> std::io::Result<Vec<TranscriptEvent>> {
    let file = File::open(path)?;
    let mut kept: std::collections::VecDeque<Value> =
        std::collections::VecDeque::with_capacity(max_records.min(1024));

    for line in BufReader::new(file).lines().map_while(Result::ok) {
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

    let mut events = Vec::new();
    for record in kept {
        let timestamp = record
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut of_record = Vec::new();
        record_events(&record, &mut of_record);
        events.extend(of_record.into_iter().map(|event| TranscriptEvent {
            timestamp: timestamp.clone(),
            event,
        }));
    }
    Ok(events)
}

/// Yalnızca sohbetin kendisi: kullanıcı ve asistan mesajları.
///
/// Alt-ajan konuşmaları ana sohbete karışırsa geçmiş okunamaz hale gelir;
/// `isSidechain` onları ayırt eden tek işaret.
fn is_conversation_record(value: &Value) -> bool {
    if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    match value.get("type").and_then(Value::as_str) {
        Some("user") | Some("assistant") => value.get("message").is_some(),
        _ => false,
    }
}

fn block_kind(block: &Value) -> &str {
    block.get("type").and_then(Value::as_str).unwrap_or("")
}

/// Arayüzün ürettiği sarmalayıcılar: `<command-name>`,
/// `<local-command-stdout>` ve benzerleri. Kullanıcının yazdığı bir şey
/// değiller.
fn is_wrapper_text(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("<command-") || head.starts_with("<local-command-")
}

/// Bir dizi biçimli kullanıcı mesajındaki düz metin blokları.
fn user_text_blocks(blocks: &[Value]) -> Option<String> {
    let joined: Vec<&str> = blocks
        .iter()
        .filter(|b| block_kind(b) == "text")
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    (!joined.is_empty()).then(|| joined.join("\n"))
}

/// Tek transcript kaydını olaylara çevirir.
fn record_events(record: &Value, out: &mut Vec<AgentEvent>) {
    let Some(content) = record.get("message").and_then(|m| m.get("content")) else {
        return;
    };
    let is_user = record.get("type").and_then(Value::as_str) == Some("user");

    match content {
        Value::String(text) => {
            if is_user && !is_wrapper_text(text) && !text.trim().is_empty() {
                out.push(AgentEvent::UserMessage { text: text.clone() });
            } else if !is_user && !text.trim().is_empty() {
                out.push(AgentEvent::TextDelta { text: text.clone() });
            }
        }
        Value::Array(blocks) => {
            for block in blocks {
                match block_kind(block) {
                    "text" => {
                        let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                        if text.trim().is_empty() {
                            continue;
                        }
                        if is_user {
                            if !is_wrapper_text(text) {
                                out.push(AgentEvent::UserMessage { text: text.into() });
                            }
                        } else {
                            out.push(AgentEvent::TextDelta { text: text.into() });
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(Value::as_str).unwrap_or_default();
                        let name = block.get("name").and_then(Value::as_str).unwrap_or_default();
                        let input = block.get("input").cloned().unwrap_or(Value::Null);
                        out.push(AgentEvent::ToolCall {
                            id: id.to_string(),
                            call: decode_tool_use(name, &input),
                        });
                    }
                    "tool_result" => {
                        let id = block
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        out.push(AgentEvent::ToolResult {
                            id: id.to_string(),
                            is_error: block
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            // Araç çıktıları zaten dokümana girmiyor (chip'ler
                            // tek satır); çalıştıran cihazın günlüğünde
                            // yaşıyorlar ve içe aktarılan oturumun günlüğü yok.
                            output: None,
                            diff: None,
                        });
                    }
                    // `thinking` dokümana yazılmıyor (canlı turda da öyle),
                    // `image` blokları eklerin yolunu taşıyor — ekler kopyalanmadığı
                    // için onları da atlıyoruz.
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn read_head(path: &Path, bytes: u64) -> std::io::Result<Vec<String>> {
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

fn read_tail(path: &Path, len: u64, bytes: u64) -> std::io::Result<Vec<String>> {
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
    use postillion_proto::ToolCall;
    use std::io::Write;

    fn tmp_jsonl(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("po-transcript-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
{"type":"assistant","message":{"model":"claude-opus-5","content":[]}}
"#,
        );

        let s = parse_summary(&path, 100, 0).unwrap();
        assert_eq!(s.session_id, "abc-123");
        assert_eq!(s.title.as_deref(), Some("guncel baslik"));
        assert_eq!(s.cwd.as_deref(), Some("/tmp/proje"));
        assert_eq!(s.git_branch.as_deref(), Some("main"));
        assert_eq!(s.model.as_deref(), Some("claude-opus-5"));
        assert!(s.has_conversation);
    }

    /// `claude -p "/usage"` çağrısının bıraktığı transcript'in şekli.
    /// Kullanıcının açtığı bir oturum değil.
    #[test]
    fn yalnizca_yerel_komut_iceren_transcript_konusma_sayilmaz() {
        let path = tmp_jsonl(
            "usage-probe.jsonl",
            concat!(
                r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>Caveat…</local-command-caveat>"}}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":"<command-name>/usage</command-name>"}}"#,
                "\n",
                r#"{"type":"system","subtype":"local_command"}"#,
                "\n",
            ),
        );

        let s = parse_summary(&path, 300, 0).unwrap();
        assert!(!s.has_conversation, "yoklama oturumu listeye girmemeli");
        assert_eq!(s.title, None, "başlık: {:?}", s.title);
    }

    #[test]
    fn olaylar_konusma_sirasini_korur() {
        let path = tmp_jsonl(
            "olaylar.jsonl",
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"dosyayı oku"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Tamam."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/a.rs"}}]}}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false}]}}"#,
                "\n",
                r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"gizli"},{"type":"text","text":"Okudum."}]}}"#,
                "\n",
                // Alt-ajan yan dalı ana sohbete karışmamalı.
                r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"yan dal"}]}}"#,
                "\n",
            ),
        );

        let events: Vec<AgentEvent> = read_events(&path, 100)
            .unwrap()
            .into_iter()
            .map(|e| e.event)
            .collect();
        assert_eq!(events.len(), 5, "{events:?}");
        assert!(matches!(&events[0], AgentEvent::UserMessage { text } if text == "dosyayı oku"));
        assert!(matches!(&events[1], AgentEvent::TextDelta { text } if text == "Tamam."));
        assert!(matches!(
            &events[2],
            AgentEvent::ToolCall { id, call: ToolCall::ReadFile { path } }
                if id == "t1" && path == "/tmp/a.rs"
        ));
        assert!(matches!(&events[3], AgentEvent::ToolResult { id, is_error, .. }
            if id == "t1" && !*is_error));
        assert!(matches!(&events[4], AgentEvent::TextDelta { text } if text == "Okudum."));
    }

    #[test]
    fn proje_slug_yolu_duzlestirir() {
        assert_eq!(project_slug(Path::new("/home/ali")), "-home-ali");
        assert_eq!(
            project_slug(Path::new("/home/ali/p/.claude/worktrees/a")),
            "-home-ali-p--claude-worktrees-a"
        );
    }

    #[cfg(windows)]
    #[test]
    fn proje_slug_windows_yolunu_duzlestirir() {
        assert_eq!(
            project_slug(Path::new(r"C:\Users\ali\Projects\x")),
            "C--Users-ali-Projects-x"
        );
    }

    #[test]
    fn alt_ajan_dosyalari_oturum_sayilmaz() {
        let root = std::env::temp_dir().join(format!("po-scan-{}", std::process::id()));
        let project = root.join("-tmp-proje");
        std::fs::create_dir_all(project.join("sess-1").join("subagents")).unwrap();
        std::fs::write(project.join("sess-1.jsonl"), "{}\n").unwrap();
        std::fs::write(
            project.join("sess-1").join("subagents").join("agent-a.jsonl"),
            "{}\n",
        )
        .unwrap();

        let found = transcript_paths(&root);
        assert_eq!(found, vec![project.join("sess-1.jsonl")]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
