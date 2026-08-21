//! Bu makinedeki Claude Code oturumlarını algılama ve içe aktarma.
//!
//! Postillion 1 oturum listesini doğrudan `~/.claude/projects/**.jsonl`
//! dosyalarından çiziyordu. v2'de bir sohbet Loro dokümanı olduğu için
//! algılama iki adım:
//!
//!  1. **Tarama** ([`LocalChats::list`]) — transcript'lerin başlık/klasör/dal
//!     özetini çıkarır ve hangilerinin zaten bir Postillion sohbetine
//!     bağlandığını (`harness_session_id`) işaretler. Tarama diskte yüzlerce
//!     megabayt okuyabildiği için (yol, mtime, boyut) anahtarlı kalıcı bir
//!     önbellek var: append-only dosyalarda bu üçlü değişmediyse içerik de
//!     değişmemiştir.
//!  2. **Benimseme** ([`LocalChats::adopt`]) — seçilen transcript'i bir
//!     sohbete çevirir: konuşma, canlı bir turun ürettiğiyle aynı parçalara
//!     katlanıp doküman anlık görüntüsü olarak yazılır (local_import'un
//!     "born chat2" şekli: imleç 0, epoch 2), ardından kayıt satırı düşer.
//!     Satırdaki `harness_session_id` + `harness_session_cwd` sürekliliği
//!     kurar: sohbete yazılan ilk mesaj `claude --resume=<sessionId>` ile
//!     aynı konuşmayı devam ettirir.
//!
//! Transcript'in kendisi kopyalanmıyor, taşınmıyor, silinmiyor — Claude Code
//! aynı dosyaya yazmaya devam ediyor. Aynı oturum iki kez benimsenmiyor
//! (satır zaten varsa liste onu `imported` işaretler ve `adopt` mevcut
//! sohbetin kimliğini döndürür).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

use postillion_doc::{
    MessagePart, MessageRole, MessageStatus, SessionDoc, SessionMessageEntry,
    fold_event_into_parts,
};
use postillion_harness::claude::transcript::{self, LocalTranscript};
use postillion_proto::{AgentEvent, Chat, ChatConfig, HarnessId, SandboxLevel, Space};
use postillion_sync::DocsStore;

use crate::chat2_host::CHAT2_DOC_EPOCH;
use crate::workspace_host::WorkspaceHost;
use crate::{EngineError, new_id};

/// Dokümana yazılan en yeni kayıt sayısı. Uzun oturumların tamamını taşımanın
/// anlamı yok: model tarafındaki bağlam `--resume` ile zaten geri geliyor,
/// bu doküman yalnızca insanın okuduğu geçmiş.
const MAX_IMPORTED_RECORDS: usize = 2_000;

/// Önbellek dosyası, `{data_dir}/local-chat-scan.json`.
const CACHE_FILE: &str = "local-chat-scan.json";

/// Listede bir satır: transcript özeti + bu profildeki karşılığı.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalChat {
    #[serde(flatten)]
    pub transcript: LocalTranscript,
    /// Bu oturumu daha önce benimsemiş sohbetin kimliği. Doluysa satır
    /// "içe aktarılmış" görünür ve tıklanınca o sohbete gider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
}

/// [`LocalChats::adopt`] sonucu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptedChat {
    pub chat_id: String,
    /// Sohbetin bağlandığı alan (yoksa cwd için yeni bir tane açılır).
    pub space_id: String,
    /// Dokümana yazılan mesaj sayısı.
    pub messages: usize,
    /// Bu oturum zaten benimsenmişti; yeni bir şey yazılmadı.
    pub already_imported: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    mtime_ms: u64,
    size_bytes: u64,
    summary: LocalTranscript,
}

/// Algılama servisi. Motor bir tane taşıyor; RPC katmanı onu klonlayıp
/// bloklayan işi çalıştırıcıya atıyor.
#[derive(Clone)]
pub struct LocalChats {
    inner: Arc<Inner>,
}

struct Inner {
    /// `~/.claude/projects` — testlerde geçersiz kılınabiliyor.
    projects_root: Option<PathBuf>,
    data_dir: PathBuf,
    device_id: String,
    store: Arc<DocsStore>,
    workspace: WorkspaceHost,
    cache: Mutex<HashMap<PathBuf, CacheEntry>>,
}

impl LocalChats {
    pub fn new(
        data_dir: &Path,
        device_id: &str,
        store: Arc<DocsStore>,
        workspace: WorkspaceHost,
    ) -> Self {
        Self::with_root(transcript::projects_dir(), data_dir, device_id, store, workspace)
    }

    /// Açık bir transcript köküyle (testler).
    pub fn with_root(
        projects_root: Option<PathBuf>,
        data_dir: &Path,
        device_id: &str,
        store: Arc<DocsStore>,
        workspace: WorkspaceHost,
    ) -> Self {
        let this = Self {
            inner: Arc::new(Inner {
                projects_root,
                data_dir: data_dir.to_path_buf(),
                device_id: device_id.to_string(),
                store,
                workspace,
                cache: Mutex::new(HashMap::new()),
            }),
        };
        this.warm_cache();
        this
    }

    fn cache_path(&self) -> PathBuf {
        self.inner.data_dir.join(CACHE_FILE)
    }

    /// Önbelleği diskten yükler. Bozuk dosya sessizce yok sayılıyor — en
    /// kötü ihtimalle bir tarama daha pahalıya mal olur.
    fn warm_cache(&self) {
        let Ok(bytes) = std::fs::read(self.cache_path()) else {
            return;
        };
        let Ok(entries) = serde_json::from_slice::<HashMap<PathBuf, CacheEntry>>(&bytes) else {
            return;
        };
        *lock(&self.inner.cache) = entries;
    }

    /// Değişiklik varsa önbelleği diske yazar (yarıda kalan yazma bir sonraki
    /// açılışta bozuk JSON okutmasın diye temp + rename).
    fn persist_cache(&self) {
        let bytes = {
            let cache = lock(&self.inner.cache);
            match serde_json::to_vec(&*cache) {
                Ok(bytes) => bytes,
                Err(err) => {
                    tracing::warn!(error = %err, "local-chat scan cache serialize failed");
                    return;
                }
            }
        };
        let path = self.cache_path();
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// Bu makinedeki oturumlar, en yeniden eskiye.
    ///
    /// Bloklayan (dosya sistemi) — çağıran çalıştırıcıya atmalı.
    pub fn list(&self) -> Result<Vec<LocalChat>, EngineError> {
        let Some(root) = self.inner.projects_root.clone() else {
            return Ok(Vec::new());
        };
        if !root.is_dir() {
            return Ok(Vec::new());
        }

        // Oturum kimliği → onu benimsemiş sohbet. Tombstone (boş kimlik)
        // eşleşmiyor: o satır "devam ettirme" demek, "bu transcript'e bağlı"
        // demek değil.
        let mut adopted: HashMap<String, String> = HashMap::new();
        for chat in self.inner.workspace.read_chats()? {
            if let Some(session_id) = chat.harness_session_id.filter(|s| !s.is_empty()) {
                adopted.insert(session_id, chat.id);
            }
        }

        let mut dirty = false;
        let mut out = Vec::new();
        // Bu taramada diskte görülen her transcript — silinenleri önbellekten
        // düşürmenin ölçüsü bu (listeye girmeyen yoklama dosyaları da
        // önbellekte kalmalı: yeniden ayrıştırmanın anlamı yok).
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for path in transcript::transcript_paths(&root) {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let len = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            seen.insert(path.clone());
            let cached = lock(&self.inner.cache).get(&path).and_then(|entry| {
                (entry.mtime_ms == mtime && entry.size_bytes == len).then(|| entry.summary.clone())
            });
            let summary = match cached {
                Some(summary) => summary,
                None => {
                    let Ok(summary) = transcript::parse_summary(&path, len, mtime) else {
                        continue; // tek bozuk dosya tüm listeyi düşürmesin
                    };
                    dirty = true;
                    lock(&self.inner.cache).insert(
                        path.clone(),
                        CacheEntry {
                            mtime_ms: mtime,
                            size_bytes: len,
                            summary: summary.clone(),
                        },
                    );
                    summary
                }
            };

            if !summary.has_conversation {
                continue;
            }
            let chat_id = adopted.get(&summary.session_id).cloned();
            out.push(LocalChat {
                transcript: summary,
                chat_id,
            });
        }

        out.sort_by(|a, b| b.transcript.modified_ms.cmp(&a.transcript.modified_ms));

        // Silinen oturumlar önbellekte kalmasın; dosya büyümesin.
        {
            let mut cache = lock(&self.inner.cache);
            let before = cache.len();
            cache.retain(|path, _| seen.contains(path));
            dirty |= cache.len() != before;
        }
        if dirty {
            self.persist_cache();
        }
        Ok(out)
    }

    /// Bir oturumu sohbete çevir. Zaten benimsenmişse mevcut sohbeti döndürür.
    ///
    /// Bloklayan (dosya sistemi + sqlite) — çağıran çalıştırıcıya atmalı.
    pub fn adopt(&self, session_id: &str) -> Result<AdoptedChat, EngineError> {
        let found = self
            .list()?
            .into_iter()
            .find(|c| c.transcript.session_id == session_id)
            .ok_or_else(|| EngineError::Other(format!("no local session: {session_id}")))?;

        if let Some(chat_id) = found.chat_id {
            let space_id = self
                .inner
                .workspace
                .chat(&chat_id)?
                .and_then(|c| c.space_id)
                .unwrap_or_default();
            return Ok(AdoptedChat {
                chat_id,
                space_id,
                messages: 0,
                already_imported: true,
            });
        }

        let summary = found.transcript;
        // Resume cwd'ye bağlı: kaydı olmayan bir oturum benimsenirse sonraki
        // tur yanlış dizinde açılır ve claude konuşmayı bulamaz.
        let cwd = summary
            .cwd
            .clone()
            .ok_or_else(|| EngineError::Other("transcript has no cwd".into()))?;

        let events = transcript::read_events(&summary.path, MAX_IMPORTED_RECORDS)
            .map_err(|err| EngineError::Other(format!("transcript read: {err}")))?;
        let entries = entries_from_events(&events, &self.inner.device_id, summary.modified_ms);

        let space_id = self.space_for(&cwd)?;
        let chat_id = new_id();

        // Doküman önce: satır göründüğü anda sohbet tıklanabilir oluyor.
        let doc = SessionDoc::init(&chat_id)?;
        for entry in &entries {
            doc.push_message(entry)?;
        }
        let bytes = doc.export_snapshot()?;
        self.inner
            .store
            .save_snapshot_with_cursor(&chat_id, &bytes, 0, CHAT2_DOC_EPOCH)?;

        let last_at = entries
            .last()
            .map(|e| e.created_at)
            .unwrap_or(summary.modified_ms as i64);
        let row = Chat {
            id: chat_id.clone(),
            device_id: self.inner.device_id.clone(),
            title: summary.title.clone(),
            archived: false,
            cwd: Some(cwd.clone()),
            branch: summary.git_branch.clone(),
            checkout_id: None,
            config: Some(ChatConfig {
                harness: HarnessId::ClaudeCode,
                model: summary.model.clone(),
                reasoning: None,
                model_options: serde_json::Map::new(),
                sandbox: SandboxLevel::WorkspaceWrite,
                mcp_servers: None,
            }),
            last_message_preview: entries.last().and_then(preview_of),
            last_message_at: Utc.timestamp_millis_opt(last_at).single(),
            created_at: entries
                .first()
                .and_then(|e| Utc.timestamp_millis_opt(e.created_at).single())
                .unwrap_or_else(Utc::now),
            // Devamlılık: sohbete yazılan ilk mesaj `--resume` ile bu
            // konuşmayı sürdürüyor.
            harness_session_id: Some(summary.session_id.clone()),
            harness_session_cwd: Some(cwd),
            // local_import ile aynı gerekçe: chat2 soyağacı açıkça yazılmazsa
            // `DocHost::open` transcript'i düşüren s2 devralma dalına giriyor.
            room_gen: Some(CHAT2_DOC_EPOCH),
            space_id: Some(space_id.clone()),
            last_seen_at: None,
        };
        self.inner.workspace.import_chat_row(&row)?;

        Ok(AdoptedChat {
            chat_id,
            space_id,
            messages: entries.len(),
            already_imported: false,
        })
    }

    /// `cwd` için bu cihazdaki alan; yoksa açar.
    fn space_for(&self, cwd: &str) -> Result<String, EngineError> {
        if let Some(space) = self
            .inner
            .workspace
            .read_spaces()?
            .into_iter()
            .find(|s: &Space| s.device_id == self.inner.device_id && s.path == cwd)
        {
            return Ok(space.id);
        }
        let space_id = new_id();
        let name = Path::new(cwd)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let git_detected = Path::new(cwd).join(".git").exists();
        self.inner
            .workspace
            .create_space(&space_id, &self.inner.device_id, cwd, name, git_detected)?;
        Ok(space_id)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// İlk 120 karakterlik kenar çubuğu önizlemesi — canlı yazma yolundaki
/// `note_message` ile aynı kural.
fn preview_of(entry: &SessionMessageEntry) -> Option<String> {
    entry.parts.iter().find_map(|part| match part {
        MessagePart::Text { text, .. } if !text.trim().is_empty() => {
            Some(text.chars().take(120).collect())
        }
        _ => None,
    })
}

/// Olay akışını doküman girdilerine katlar.
///
/// Kullanıcı mesajı akışı bölüyor: o ana kadar biriken asistan parçaları bir
/// girdi olarak kapanıyor, kullanıcı kendi girdisini alıyor. Aradaki araç
/// sonuçları (transcript'te kullanıcı rolünde geliyorlar) çağrı yaptıkları
/// parçaya kapanıyor — yani bir tur, canlı çalışmış gibi tek asistan girdisi.
///
/// Zaman damgası kayıttan geliyor; okunamayan damgalar dosyanın mtime'ına
/// düşüyor ki sıralama hiçbir zaman 1970'e kaymasın.
fn entries_from_events(
    events: &[transcript::TranscriptEvent],
    device_id: &str,
    fallback_ms: u64,
) -> Vec<SessionMessageEntry> {
    let fallback = fallback_ms as i64;
    let mut out: Vec<SessionMessageEntry> = Vec::new();
    let mut parts: Vec<MessagePart> = Vec::new();
    let mut started_at: Option<i64> = None;

    let entry = |role: MessageRole, parts: Vec<MessagePart>, at: i64| SessionMessageEntry {
        id: new_id(),
        role,
        parts,
        created_at: at,
        device_id: device_id.to_owned(),
        status: Some(MessageStatus::Complete),
        continuation_of: None,
    };

    for item in events {
        let at = item
            .timestamp
            .as_deref()
            .and_then(parse_timestamp_ms)
            .unwrap_or(fallback);
        match &item.event {
            AgentEvent::UserMessage { text } => {
                if !parts.is_empty() {
                    out.push(entry(
                        MessageRole::Assistant,
                        std::mem::take(&mut parts),
                        started_at.take().unwrap_or(at),
                    ));
                }
                out.push(entry(
                    MessageRole::User,
                    vec![MessagePart::Text {
                        id: "t0".into(),
                        text: text.clone(),
                    }],
                    at,
                ));
            }
            event => {
                started_at.get_or_insert(at);
                fold_event_into_parts(&mut parts, event);
            }
        }
    }
    if !parts.is_empty() {
        out.push(entry(
            MessageRole::Assistant,
            parts,
            started_at.unwrap_or(fallback),
        ));
    }
    out
}

/// ISO-8601 damgayı epoch ms'e çevirir.
fn parse_timestamp_ms(raw: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postillion_proto::ToolCall;

    fn ev(timestamp: Option<&str>, event: AgentEvent) -> transcript::TranscriptEvent {
        transcript::TranscriptEvent {
            timestamp: timestamp.map(str::to_string),
            event,
        }
    }

    #[test]
    fn tur_basina_tek_asistan_girdisi() {
        let events = vec![
            ev(
                Some("2026-08-20T10:00:00Z"),
                AgentEvent::UserMessage {
                    text: "dosyayı oku".into(),
                },
            ),
            ev(
                Some("2026-08-20T10:00:01Z"),
                AgentEvent::TextDelta {
                    text: "Tamam.".into(),
                },
            ),
            ev(
                Some("2026-08-20T10:00:02Z"),
                AgentEvent::ToolCall {
                    id: "t1".into(),
                    call: ToolCall::ReadFile {
                        path: "/tmp/a.rs".into(),
                    },
                },
            ),
            // Araç sonucu transcript'te kullanıcı rolünde geliyor ama sohbeti
            // bölmüyor: aynı turun parçası.
            ev(
                Some("2026-08-20T10:00:03Z"),
                AgentEvent::ToolResult {
                    id: "t1".into(),
                    is_error: false,
                    output: None,
                    diff: None,
                },
            ),
            ev(
                Some("2026-08-20T10:00:04Z"),
                AgentEvent::TextDelta {
                    text: " Okudum.".into(),
                },
            ),
        ];

        let entries = entries_from_events(&events, "dev-1", 0);
        assert_eq!(entries.len(), 2, "{entries:#?}");
        assert_eq!(entries[0].role, MessageRole::User);
        assert_eq!(entries[1].role, MessageRole::Assistant);
        // Metin, araç, metin: araçtan sonraki metin yeni bir parça açıyor —
        // canlı turun katlaması da aynı şeyi yapıyor.
        assert_eq!(entries[1].parts.len(), 3);
        match &entries[1].parts[1] {
            MessagePart::Tool { resolved, id, .. } => {
                assert!(resolved, "araç sonucu parçaya kapanmalı");
                assert_eq!(id, "t1");
            }
            other => panic!("araç parçası bekleniyordu: {other:?}"),
        }
        assert!(entries[0].created_at < entries[1].created_at);
    }

    #[test]
    fn damgasiz_kayitlar_dosyanin_zamanina_duser() {
        let events = vec![ev(
            None,
            AgentEvent::UserMessage {
                text: "merhaba".into(),
            },
        )];
        let entries = entries_from_events(&events, "dev-1", 1_700_000_000_000);
        assert_eq!(entries[0].created_at, 1_700_000_000_000);
    }
}
