//! Bağlam eşiği aşılınca otomatik `/compact`.
//!
//! Neden var: otomatik sıkıştırma headless modda çalışmıyor ve uygulama bunu
//! kendisi yapmazsa bağlam sınırsız büyüyor — ölçüldü, yalnızca arayüzden
//! sürülen oturumlar hiç sıkışmadan 788k token'a çıkmış ve tur başına
//! maliyetleri sıkışan oturumların ~2,5 katı olmuştu.
//!
//! Test ettiği asıl şey eşiğin **modelin penceresine göre** ölçeklendiği:
//! 1M'lik bir modelde 200k'da sıkıştırmak, kullanıcının ödediği bağlamın
//! dörtte üçünü hiç kullanmadan atmak olurdu.

use std::sync::{Arc, Once};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::sync::{Mutex, mpsc};

use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionStatus, SteeringMode,
};

const CHAT: &str = "chat-auto-compact";
/// Normal window: far beyond the test horizon — any park inside the test
/// window must have come through the self-continued path.
const QUIESCE_MS: u64 = 600_000;
/// Short window under test.
const SELF_QUIESCE_MS: u64 = 400;

fn init_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: called before any engine (and thus any reader of the vars)
        // exists in this test process.
        unsafe {
            std::env::set_var("ZERON_TURN_QUIESCE_MS", QUIESCE_MS.to_string());
            std::env::set_var("ZERON_SELF_TURN_QUIESCE_MS", SELF_QUIESCE_MS.to_string());
        }
    });
}

fn run_request(prompt: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.into(),
        harness: None,
        model: None,
        reasoning: None,
        model_options: Default::default(),
        cwd: "/tmp".into(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: true,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    }
}

fn done(status: DoneStatus) -> AgentEvent {
    AgentEvent::Done {
        status,
        result: None,
        error: None,
        session_id: Some("hs-ac".into()),
    }
}

fn session_started() -> AgentEvent {
    AgentEvent::SessionStarted {
        harness: HarnessId::Mock,
        model: "mock-1".into(),
        tools: vec![],
        cwd: "/tmp".into(),
        session_id: "hs-ac".into(),
        assistant_message_id: "a-ac".into(),
    }
}

fn text(t: &str) -> AgentEvent {
    AgentEvent::TextDelta { text: t.into() }
}

/// Feed-by-hand harness (see `turn_quiesce.rs`): the test pushes events
/// through a channel; accepted steers confirm with a `Steered` boundary.
struct FeedHarness {
    main_prompt: String,
    feed: Mutex<Option<mpsc::UnboundedReceiver<AgentEvent>>>,
}

#[async_trait]
impl Harness for FeedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Feed"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        mut controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        if request.prompt != self.main_prompt {
            let events = vec![Ok(done(DoneStatus::Completed))];
            return Ok(futures::stream::iter(events).boxed());
        }
        let mut feed = self
            .feed
            .lock()
            .await
            .take()
            .expect("FeedHarness serves the main dispatch once per test");
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(64);
        tokio::spawn(async move {
            let mut steering_open = true;
            loop {
                tokio::select! {
                    biased;
                    steer = controls.steering.recv(), if steering_open => match steer {
                        Some(_) => {
                            let boundary = AgentEvent::Steered {
                                assistant_message_id: None,
                                next_assistant_message_id: None,
                            };
                            if tx.send(Ok(boundary)).await.is_err() {
                                return;
                            }
                        }
                        None => steering_open = false,
                    },
                    event = feed.recv() => match event {
                        Some(event) => {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        None => return,
                    },
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

struct Rig {
    core: EngineCore,
    feed: mpsc::UnboundedSender<AgentEvent>,
    _dir: tempfile::TempDir,
}

fn assemble(main_prompt: &str) -> Rig {
    let (feed, rx) = mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(FeedHarness {
        main_prompt: main_prompt.into(),
        feed: Mutex::new(Some(rx)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");
    Rig {
        core,
        feed,
        _dir: dir,
    }
}

fn status(core: &EngineCore) -> Option<SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

async fn wait_for<F>(mut predicate: F, what: &str)
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}


/// Steer kanalına düşen promptları kaydeden harness.
///
/// `/compact` doc'a kullanıcı mesajı olarak YAZILMIYOR (bakım komutu, konuşma
/// değil), dolayısıyla transcript'ten doğrulanamaz — gözlenebilir tek yer
/// harness'in steer kanalı.
struct RecordingHarness {
    /// Feed yalnızca ANA dispatch'e servis ediliyor. Engine başlık üretimi
    /// gibi yan koşular için de `run` çağırıyor ve prompt kontrolü olmadan
    /// besleme onlardan birine gidiyordu (ölçüldü: döngüye tek olay ulaştı).
    main_prompt: String,
    feed: Mutex<Option<mpsc::UnboundedReceiver<AgentEvent>>>,
    steers: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Harness for RecordingHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Recording"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        mut controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        if request.prompt != self.main_prompt {
            let events = vec![Ok(done(DoneStatus::Completed))];
            return Ok(futures::stream::iter(events).boxed());
        }
        let mut feed = self
            .feed
            .lock()
            .await
            .take()
            .expect("feed ana dispatch'e bir kez servis edilir");
        let steers = self.steers.clone();
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(64);
        tokio::spawn(async move {
            let mut steering_open = true;
            loop {
                tokio::select! {
                    biased;
                    steer = controls.steering.recv(), if steering_open => match steer {
                        Some(message) => {
                            steers.lock().await.push(message.prompt.clone());
                            let boundary = AgentEvent::Steered {
                                assistant_message_id: None,
                                next_assistant_message_id: None,
                            };
                            if tx.send(Ok(boundary)).await.is_err() {
                                return;
                            }
                        }
                        None => steering_open = false,
                    },
                    event = feed.recv() => match event {
                        Some(event) => {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        None => return,
                    },
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

struct CompactRig {
    core: EngineCore,
    feed: mpsc::UnboundedSender<AgentEvent>,
    steers: Arc<Mutex<Vec<String>>>,
    _dir: tempfile::TempDir,
}

fn assemble_recording() -> CompactRig {
    let (feed, rx) = mpsc::unbounded_channel();
    let steers = Arc::new(Mutex::new(Vec::new()));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(RecordingHarness {
        main_prompt: "go".into(),
        feed: Mutex::new(Some(rx)),
        steers: steers.clone(),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");
    CompactRig {
        core,
        feed,
        steers,
        _dir: dir,
    }
}

fn usage(context_tokens: u64) -> AgentEvent {
    AgentEvent::Usage {
        input_tokens: 2,
        output_tokens: 50,
        context_tokens,
    }
}

/// `model` verilen bir istek — pencere bundan türetiliyor.
fn request_with_model(model: Option<&str>) -> RunRequest {
    RunRequest {
        model: model.map(str::to_string),
        ..run_request("go")
    }
}

async fn steers_of(rig: &CompactRig) -> Vec<String> {
    rig.steers.lock().await.clone()
}

/// 200k'lık modelde eşik 170k: 180k ölçümü sıkıştırmayı tetiklemeli.
#[tokio::test]
async fn asilan_baglam_sikistirmayi_tetikler() {
    let rig = assemble_recording();
    rig.core
        .sessions
        .dispatch(CHAT, HarnessId::Mock, request_with_model(None), None)
        .await
        .expect("dispatch");

    rig.feed.send(session_started()).unwrap();
    rig.feed.send(text("tamam")).unwrap();
    rig.feed.send(usage(180_000)).unwrap();
    rig.feed.send(done(DoneStatus::Completed)).unwrap();

    wait_for_async(
        || {
            let rig = &rig;
            async move { steers_of(rig).await.iter().any(|p| p == "/compact") }
        },
        "otomatik /compact",
    )
    .await;

    // Ölçüm oturuma da yazılmalı: gösterge onu okuyor.
    assert_eq!(
        rig.core
            .sessions
            .session_status(CHAT)
            .and_then(|s| s.context_tokens),
        Some(180_000),
    );
}

/// Aynı 180k, 1M'lik bir modelde eşiğin (850k) çok altında — sıkıştırma YOK.
///
/// Düzeltilen hata tam olarak buydu: sabit 200k eşiği uzun bağlamlı modellerde
/// ödenmiş bağlamın dörtte üçünü kullanılmadan attırıyordu.
#[tokio::test]
async fn uzun_baglamli_modelde_ayni_olcum_sikistirmiyor() {
    let rig = assemble_recording();
    rig.core
        .sessions
        .dispatch(
            CHAT,
            HarnessId::Mock,
            request_with_model(Some("mock-1[1m]")),
            None,
        )
        .await
        .expect("dispatch");

    rig.feed.send(session_started()).unwrap();
    rig.feed.send(usage(180_000)).unwrap();
    rig.feed.send(done(DoneStatus::Completed)).unwrap();

    // Park, sıkıştırma olmadığının gözlenebilir işareti.
    wait_for(
        || rig.core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "sıkıştırmadan park",
    )
    .await;
    assert!(
        !steers_of(&rig).await.iter().any(|p| p == "/compact"),
        "1M penceresinde 180k eşiğin çok altında; sıkıştırılmamalıydı"
    );
}

/// Async koşul için `wait_for` ikizi.
async fn wait_for_async<F, Fut>(mut predicate: F, what: &str)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if predicate().await {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
