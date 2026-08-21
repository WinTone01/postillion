//! Yerel Claude Code oturumlarının algılanması ve benimsenmesi: tarama
//! elemesi, dokümana yazılan geçmiş, `--resume` sürekliliği ve idempotens.

use std::sync::Arc;

use postillion_doc::{MessagePart, MessageRole, SessionDoc};
use postillion_engine::local_chats::LocalChats;
use postillion_engine::{EngineCore, EngineProfile, HarnessId, default_registry};

/// Gerçek bir transcript'in şekli: konuşma + araç turu + başlık kaydı.
const TRANSCRIPT: &str = concat!(
    r#"{"type":"user","message":{"role":"user","content":"dosyayı oku"},"cwd":"/tmp/proj","gitBranch":"main","timestamp":"2026-08-20T10:00:00Z"}"#,
    "\n",
    r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"text","text":"Tamam."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/proj/a.rs"}}]},"timestamp":"2026-08-20T10:00:01Z"}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false}]},"timestamp":"2026-08-20T10:00:02Z"}"#,
    "\n",
    r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"text","text":"Okudum."}]},"timestamp":"2026-08-20T10:00:03Z"}"#,
    "\n",
    r#"{"type":"custom-title","customTitle":"Dosya okuma"}"#,
    "\n",
);

/// Yalnızca `claude -p "/usage"` yoklamasının bıraktığı transcript.
const PROBE: &str = concat!(
    r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>…</local-command-caveat>"}}"#,
    "\n",
    r#"{"type":"user","message":{"role":"user","content":"<command-name>/usage</command-name>"},"cwd":"/tmp/proj"}"#,
    "\n",
);

struct Fixture {
    _dir: tempfile::TempDir,
    core: EngineCore,
    local: LocalChats,
    store_root: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("engine");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let projects = dir.path().join("claude-projects").join("-tmp-proj");
    std::fs::create_dir_all(&projects).expect("projects dir");
    std::fs::write(projects.join("sess-1.jsonl"), TRANSCRIPT).expect("transcript");
    std::fs::write(projects.join("sess-probe.jsonl"), PROBE).expect("probe transcript");
    // Alt-ajan yan dalı: oturum değil.
    std::fs::create_dir_all(projects.join("sess-1").join("subagents")).expect("subagents dir");
    std::fs::write(
        projects.join("sess-1").join("subagents").join("agent-a.jsonl"),
        TRANSCRIPT,
    )
    .expect("subagent transcript");

    let profile = EngineProfile::local(&data_dir).expect("local profile");
    let store_root = profile.store_root().to_path_buf();
    let core =
        EngineCore::assemble_with_profile(profile, Arc::new(default_registry()), HarnessId::Mock, None)
            .expect("assemble");
    let store = Arc::new(postillion_sync::DocsStore::open(&store_root).expect("store"));
    let local = LocalChats::with_root(
        Some(dir.path().join("claude-projects")),
        &data_dir,
        &core.device_id,
        store,
        core.workspace.clone(),
    );
    Fixture {
        _dir: dir,
        core,
        local,
        store_root,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tarama_yalnizca_gercek_oturumlari_listeler() {
    let f = fixture();

    let listed = f.local.list().expect("list");
    assert_eq!(listed.len(), 1, "{listed:#?}");
    let row = &listed[0];
    assert_eq!(row.transcript.session_id, "sess-1");
    assert_eq!(row.transcript.title.as_deref(), Some("Dosya okuma"));
    assert_eq!(row.transcript.cwd.as_deref(), Some("/tmp/proj"));
    assert_eq!(row.transcript.git_branch.as_deref(), Some("main"));
    assert_eq!(row.transcript.model.as_deref(), Some("claude-opus-5"));
    assert!(row.chat_id.is_none(), "henüz içe aktarılmadı");

    f.core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn benimseme_gecmisi_ve_resume_surekliligini_kurar() {
    let f = fixture();

    let adopted = f.local.adopt("sess-1").expect("adopt");
    assert!(!adopted.already_imported);
    assert_eq!(
        adopted.messages, 2,
        "bir kullanıcı mesajı + araç turuyla birlikte tek asistan girdisi"
    );

    // Kayıt satırı: başlık, klasör, dal, harness ve resume kimliği.
    let chat = f
        .core
        .workspace
        .chat(&adopted.chat_id)
        .expect("read chat")
        .expect("chat row");
    assert_eq!(chat.title.as_deref(), Some("Dosya okuma"));
    assert_eq!(chat.cwd.as_deref(), Some("/tmp/proj"));
    assert_eq!(chat.branch.as_deref(), Some("main"));
    assert_eq!(chat.harness_session_id.as_deref(), Some("sess-1"));
    // Resume cwd'ye bağlı: kayıt olmadan sonraki tur konuşmayı bulamaz.
    assert_eq!(chat.harness_session_cwd.as_deref(), Some("/tmp/proj"));
    assert_eq!(
        chat.config.as_ref().map(|c| c.harness),
        Some(HarnessId::ClaudeCode)
    );
    // chat2 soyağacı açıkça yazılmalı, yoksa doküman ilk açılışta düşüyor.
    assert_eq!(chat.room_gen, Some(2));
    // cwd için bir alan açılmış olmalı.
    let space_id = chat.space_id.clone().expect("space");
    let space = f
        .core
        .workspace
        .space(&space_id)
        .expect("read space")
        .expect("space row");
    assert_eq!(space.path, "/tmp/proj");

    // Doküman: konuşma sırayla, araç çağrısı sonucuna kapanmış.
    let bytes = postillion_sync::DocsStore::open(&f.store_root)
        .expect("store")
        .load_snapshot(&adopted.chat_id)
        .expect("load")
        .expect("snapshot exists");
    let loro = loro::LoroDoc::new();
    loro.import(&bytes).expect("import snapshot");
    let doc = SessionDoc::from_doc(loro);
    let entries = doc.read_entries().expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].role, MessageRole::User);
    assert!(matches!(&entries[0].parts[0], MessagePart::Text { text, .. } if text == "dosyayı oku"));
    assert_eq!(entries[1].role, MessageRole::Assistant);
    assert!(
        entries[1]
            .parts
            .iter()
            .any(|p| matches!(p, MessagePart::Tool { id, resolved, .. } if id == "t1" && *resolved)),
        "araç sonucu parçaya kapanmalı: {:#?}",
        entries[1].parts
    );

    f.core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ayni_oturum_iki_kez_benimsenmez() {
    let f = fixture();

    let first = f.local.adopt("sess-1").expect("adopt");
    let listed = f.local.list().expect("list");
    assert_eq!(
        listed[0].chat_id.as_deref(),
        Some(first.chat_id.as_str()),
        "liste satırı içe aktarılmış görünmeli"
    );

    let again = f.local.adopt("sess-1").expect("re-adopt");
    assert!(again.already_imported);
    assert_eq!(again.chat_id, first.chat_id, "yeni sohbet açılmamalı");
    assert_eq!(again.messages, 0);

    f.core.shutdown().await;
}
