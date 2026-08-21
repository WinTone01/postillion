//! Yerel yoklama: bu makinedeki gerçek Claude Code transcript'lerini tara ve
//! ne bulduğunu yaz. Tarama maliyeti (bu ağaç yüz megabaytlar tutabiliyor) ve
//! başlık/klasör çıkarımının gerçek dosyalarda tuttuğu, ancak gerçek diskte
//! ölçülebilir.
//!
//!     cargo run -p postillion-harness --example local_transcript_scan
//!     cargo run -p postillion-harness --example local_transcript_scan -- <sessionId>
//!
//! Bir oturum kimliği verilirse o transcript olaylara çevrilip özetleniyor —
//! motorun dokümana yazacağı şeyin ta kendisi.

use std::time::Instant;

use postillion_harness::claude::transcript;
use postillion_proto::AgentEvent;

fn main() {
    let Some(root) = transcript::projects_dir() else {
        eprintln!("HOME çözülemedi");
        return;
    };
    if !root.is_dir() {
        eprintln!("{} yok — Claude Code hiç çalışmamış", root.display());
        return;
    }

    let started = Instant::now();
    let paths = transcript::transcript_paths(&root);
    let mut sessions = Vec::new();
    let mut bytes = 0u64;
    for path in &paths {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        bytes += meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        match transcript::parse_summary(path, meta.len(), mtime) {
            Ok(summary) => sessions.push(summary),
            Err(err) => eprintln!("{}: {err}", path.display()),
        }
    }
    let elapsed = started.elapsed();
    sessions.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    let conversations = sessions.iter().filter(|s| s.has_conversation).count();
    println!(
        "{} dosya · {:.1} MB · {conversations}/{} konuşma · {elapsed:?}",
        paths.len(),
        bytes as f64 / (1024.0 * 1024.0),
        sessions.len(),
    );

    match std::env::args().nth(1) {
        None => {
            for s in sessions.iter().filter(|s| s.has_conversation).take(20) {
                println!(
                    "  {} · {} · {} · {}",
                    s.session_id,
                    s.cwd.as_deref().unwrap_or("-"),
                    s.model.as_deref().unwrap_or("-"),
                    s.title.as_deref().unwrap_or("(başlıksız)"),
                );
            }
        }
        Some(session_id) => {
            let Some(session) = sessions.iter().find(|s| s.session_id == session_id) else {
                eprintln!("oturum bulunamadı: {session_id}");
                return;
            };
            let events = match transcript::read_events(&session.path, 100_000) {
                Ok(events) => events,
                Err(err) => {
                    eprintln!("okunamadı: {err}");
                    return;
                }
            };
            let (mut user, mut text, mut calls, mut results) = (0, 0, 0, 0);
            for item in &events {
                match item.event {
                    AgentEvent::UserMessage { .. } => user += 1,
                    AgentEvent::TextDelta { .. } => text += 1,
                    AgentEvent::ToolCall { .. } => calls += 1,
                    AgentEvent::ToolResult { .. } => results += 1,
                    _ => {}
                }
            }
            println!(
                "{}: {} olay — {user} kullanıcı, {text} metin, {calls} araç çağrısı, {results} sonuç",
                session.session_id,
                events.len(),
            );
        }
    }
}
