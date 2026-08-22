//! Claude CLI stream-json wire frames (stdout JSONL + stdin lines).
//!
//! Tolerant by construction: every field defaults, unknown frame types map to
//! [`Frame::Other`], so a newer CLI never breaks parsing — we only read the
//! fields the normalizer needs (spec: docs/research/harness.md).

use serde::Deserialize;
use serde_json::{Value, json};

/// One parsed stdout line.
#[derive(Debug)]
pub(crate) enum Frame {
    System(SystemFrame),
    StreamEvent(StreamEventFrame),
    Assistant(MessageFrame),
    User(MessageFrame),
    RateLimit(RateLimitFrame),
    Result(ResultFrame),
    ControlRequest(ControlRequestFrame),
    /// control_response / control_cancel_request / anything unknown.
    Other,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SystemFrame {
    #[serde(default)]
    pub subtype: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub session_id: String,
    /// `task_notification` (a background subagent settling): the spawning
    /// Agent tool's id — the only TAGGED terminal signal the 2.1.x wire has
    /// for a background subagent (its frames otherwise just stop).
    #[serde(default, alias = "toolUseId")]
    pub tool_use_id: Option<String>,
    /// `task_notification` terminal status (`completed`/`failed`/`killed`…).
    #[serde(default)]
    pub status: Option<String>,
    /// `task_started`: the agent/task id (`SendMessage`'s `to:` address) —
    /// with `tool_use_id`, the agentId→spawn mapping steers need.
    #[serde(default, alias = "taskId")]
    pub task_id: Option<String>,
    /// `task_started`: present only for AGENT tasks (a subagent spawning),
    /// absent on subagent-owned background shell tasks.
    #[serde(default)]
    pub subagent_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct StreamEventFrame {
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub event: StreamEventBody,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct StreamEventBody {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub delta: Delta,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Delta {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub thinking: String,
}

/// An `assistant` or `user` frame (an Anthropic API message envelope).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct MessageFrame {
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub message: MessageBody,
    /// Terse assistant-level error code (`rate_limit`, `billing_error`, …).
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MessageBody {
    /// Either a plain string or an array of content blocks.
    #[serde(default)]
    pub content: Value,
}

impl MessageBody {
    pub fn blocks(&self) -> impl Iterator<Item = ContentBlock> + '_ {
        self.content
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or_default()
            .iter()
            .filter_map(|b| serde_json::from_value(b.clone()).ok())
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ContentBlock {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub tool_use_id: String,
    #[serde(default)]
    pub is_error: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RateLimitFrame {
    #[serde(default)]
    pub rate_limit_info: RateLimitInfo,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RateLimitInfo {
    #[serde(default)]
    pub status: String,
    #[serde(rename = "rateLimitType", default)]
    pub rate_limit_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ResultFrame {
    #[serde(default)]
    pub subtype: String,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub errors: Vec<Value>,
    #[serde(default)]
    pub usage: UsageBody,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UsageBody {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    /// Bu turda önbelleğe yazılan bağlam.
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    /// Bu turda önbellekten okunan bağlam.
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    /// Turdaki her API çağrısının kendi muhasebesi.
    ///
    /// Sonuç çerçevesinin ÜST DÜZEY alanları bir bağlam ölçüsü değil, tur
    /// boyunca harcananın toplamı: alt ajanların kullanımı da oraya
    /// ekleniyor. Bağlam boyutu isteniyorsa bakılacak yer burası.
    #[serde(default)]
    pub iterations: Vec<IterationUsage>,
}

/// Tek bir API çağrısının girdi muhasebesi.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct IterationUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

impl IterationUsage {
    fn context(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

impl UsageBody {
    /// Konuşmanın ŞU ANKİ bağlam boyutu.
    ///
    /// Üç kalemin toplamı alınıyor çünkü tek başına `input_tokens` yanıltıcı:
    /// bağlamın neredeyse tamamı önbellekten geliyor ve o alan hemen her turda
    /// sıfıra yakın kalıyor. Ölçüldü — 656k token'lık bir konuşmada
    /// `input_tokens` 2 iken `cache_read_input_tokens` 656.190'dı.
    ///
    /// Ama toplam SON iterasyondan alınıyor, üst düzey alanlardan değil. Üst
    /// düzey alanlar tur boyunca harcananı topluyor: bağlam her araç
    /// çağrısında yeniden önbellekten okunduğu için elli çağrılık bir tur
    /// bağlamı elli kez sayıyor, alt ajanların kendi bağlamları da üstüne
    /// biniyor. Ölçüldü — gerçek bir oturumda bu 22 ve 34 MİLYON token
    /// gösterdi; hiçbir modelin penceresi o değil.
    ///
    /// Bu yalnızca göstergeyi bozmuyordu: otomatik sıkıştırma bu sayıyı eşikle
    /// karşılaştırdığı için eşik hemen her turda aşılmış görünüyor ve
    /// `/compact` durmadan tetikleniyordu — sıkıştırmanın kendisi token
    /// yakarken bağlamın gerçekte dolup dolmadığı hiç ölçülmemiş oluyordu.
    pub fn context_tokens(&self) -> u64 {
        if let Some(last) = self.iterations.last() {
            return last.context();
        }
        // `iterations` taşımayan eski CLI sürümleri: üst düzey alanlar tek
        // çağrılık turlarda doğru, çok çağrılıklarda şişik. Yanlış ama VAR
        // olan bir ölçü, hiç ölçmemekten iyi — sıfır "bilinmiyor" demek ve
        // göstergeyi tamamen kapatırdı.
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

/// A CLI→client control request (`can_use_tool` is the one we act on).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ControlRequestFrame {
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub request: ControlRequestBody,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ControlRequestBody {
    #[serde(default)]
    pub subtype: String,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub input: Value,
}

/// Parse one stdout JSONL line. `Err` = not JSON; unknown types = `Other`.
pub(crate) fn parse_frame(line: &str) -> Result<Frame, serde_json::Error> {
    let value: Value = serde_json::from_str(line)?;
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    let frame = match kind {
        "system" => Frame::System(serde_json::from_value(value)?),
        "stream_event" => Frame::StreamEvent(serde_json::from_value(value)?),
        "assistant" => Frame::Assistant(serde_json::from_value(value)?),
        "user" => Frame::User(serde_json::from_value(value)?),
        "rate_limit_event" => Frame::RateLimit(serde_json::from_value(value)?),
        "result" => Frame::Result(serde_json::from_value(value)?),
        "control_request" => Frame::ControlRequest(serde_json::from_value(value)?),
        _ => Frame::Other,
    };
    Ok(frame)
}

/// A stdin user turn: `{"type":"user","message":{...},"parent_tool_use_id":null}`.
/// Steering = another such line mid-run (consumed at a step boundary).
pub(crate) fn user_message_line(text: &str) -> String {
    json!({
        "type": "user",
        "message": { "role": "user", "content": text },
        "parent_tool_use_id": null,
    })
    .to_string()
}

/// One inline image for a stdin user turn (Anthropic base64 image source).
pub(crate) struct ImageBlock {
    /// One of the API-supported media types (png/jpeg/gif/webp).
    pub media_type: String,
    /// Raw base64 (no data-URL prefix).
    pub data: String,
}

/// A stdin user turn whose content is an array of blocks: the attached images
/// first, then the text — the standard Anthropic image+text message shape
/// (verified against the real CLI: `--input-format stream-json` accepts image
/// content blocks in user frames). Empty `images` degrades to the plain line.
pub(crate) fn user_message_line_with_images(text: &str, images: &[ImageBlock]) -> String {
    if images.is_empty() {
        return user_message_line(text);
    }
    let mut blocks: Vec<Value> = images
        .iter()
        .map(|img| {
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.media_type,
                    "data": img.data,
                },
            })
        })
        .collect();
    blocks.push(json!({ "type": "text", "text": text }));
    json!({
        "type": "user",
        "message": { "role": "user", "content": blocks },
        "parent_tool_use_id": null,
    })
    .to_string()
}

/// Success reply to a CLI control request (`can_use_tool` allow/deny payloads).
pub(crate) fn control_response_line(request_id: &str, response: Value) -> String {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": response,
        },
    })
    .to_string()
}

/// `can_use_tool` allow payload with the (possibly updated) tool input.
pub(crate) fn allow_response(updated_input: Value) -> Value {
    json!({ "behavior": "allow", "updatedInput": updated_input })
}

/// Client→CLI interrupt control request.
pub(crate) fn interrupt_request_line(request_id: &str) -> String {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": { "subtype": "interrupt" },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_and_unknown_frames() {
        let init = r#"{"type":"system","subtype":"init","model":"m","tools":["Bash"],"cwd":"/x","session_id":"s1"}"#;
        match parse_frame(init).expect("parses") {
            Frame::System(f) => {
                assert_eq!(f.subtype, "init");
                assert_eq!(f.session_id, "s1");
            }
            other => panic!("unexpected frame: {other:?}"),
        }
        assert!(matches!(
            parse_frame(r#"{"type":"mystery_frame"}"#).expect("parses"),
            Frame::Other
        ));
        assert!(parse_frame("not json").is_err());
    }

    #[test]
    fn user_line_shape_matches_protocol() {
        let line = user_message_line("hi");
        let v: Value = serde_json::from_str(&line).expect("json");
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["content"], "hi");
        assert!(v["parent_tool_use_id"].is_null());
    }

    #[test]
    fn user_line_with_images_is_blocks_then_text() {
        let line = user_message_line_with_images(
            "what is this?",
            &[ImageBlock {
                media_type: "image/png".into(),
                data: "QUJD".into(),
            }],
        );
        let v: Value = serde_json::from_str(&line).expect("json");
        assert_eq!(v["type"], "user");
        let content = v["message"]["content"].as_array().expect("array content");
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "QUJD");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "what is this?");
        // No images ⇒ identical to the plain string line.
        assert_eq!(
            user_message_line_with_images("hi", &[]),
            user_message_line("hi")
        );
    }

    /// Bağlam ölçüsü SON iterasyondan gelmeli, üst düzey toplamdan değil.
    ///
    /// Gerçek bir oturumda üst düzey alanlar 22 ve 34 MİLYON token gösterdi.
    /// Bu yalnızca göstergeyi bozmadı: otomatik sıkıştırma aynı sayıya baktığı
    /// için eşik hemen her turda aşılmış görünüyor ve `/compact` durmadan
    /// tetikleniyordu.
    #[test]
    fn baglam_son_iterasyondan_okunuyor() {
        let usage: UsageBody = serde_json::from_value(serde_json::json!({
            // Üst düzey: tur boyunca harcanan + alt ajanlar.
            "input_tokens": 10,
            "cache_creation_input_tokens": 413,
            "cache_read_input_tokens": 52_962,
            "output_tokens": 155,
            "iterations": [
                { "input_tokens": 4, "cache_creation_input_tokens": 100,
                  "cache_read_input_tokens": 20_000 },
                { "input_tokens": 10, "cache_creation_input_tokens": 413,
                  "cache_read_input_tokens": 26_552 }
            ]
        }))
        .expect("usage çözülmeli");

        assert_eq!(usage.context_tokens(), 26_975, "son iterasyonun bağlamı");
        // Üst düzey toplam alınsaydı 53.385 çıkardı — gerçek bağlamın iki katı.
        assert_ne!(usage.context_tokens(), 53_385);
    }

    /// `iterations` taşımayan eski CLI sürümleri.
    ///
    /// Yanlış ama VAR olan bir ölçü, hiç ölçmemekten iyi: sıfır "bilinmiyor"
    /// demek ve göstergeyi tamamen kapatırdı.
    #[test]
    fn iterasyonsuz_surumde_ust_duzeye_dusuyor() {
        let usage: UsageBody = serde_json::from_value(serde_json::json!({
            "input_tokens": 2,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 656_190,
            "output_tokens": 100
        }))
        .expect("usage çözülmeli");
        assert_eq!(usage.context_tokens(), 656_192);
    }

    /// Depodaki gerçek kayıt: tek iterasyonlu bir turda bile üst düzey alanlar
    /// bağlamın iki katını gösteriyor, çünkü alt ajanın kullanımı da orada.
    #[test]
    fn gercek_kayitta_ust_duzey_baglamdan_buyuk() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude/live-2.1.228-background-subagent.jsonl");
        let text = std::fs::read_to_string(&fixture).expect("kayıt okunmalı");

        let mut seen = 0;
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if value.get("type").and_then(|v| v.as_str()) != Some("result") {
                continue;
            }
            let usage: UsageBody =
                serde_json::from_value(value["usage"].clone()).expect("usage çözülmeli");
            if usage.iterations.is_empty() {
                continue;
            }
            seen += 1;
            let top = usage.input_tokens
                + usage.cache_creation_input_tokens
                + usage.cache_read_input_tokens;
            assert!(
                usage.context_tokens() <= top,
                "bağlam üst düzey toplamı aşamaz"
            );
        }
        assert!(seen > 0, "kayıtta iterasyonlu sonuç çerçevesi bulunamadı");
    }
}
