//! Sunucu entegrasyon testlerinin ortak koşum takımı.
//!
//! Hem bellek içi hem Postgres destekli testler AYNI sunucuyu ve AYNI gerçek
//! `ChatClient`'ı kullanıyor: iki dosyaya iki ayrı kurulum yazmak, birinin
//! sessizce ötekinden ayrılmasına açık kapı bırakırdı.

#![allow(dead_code)] // her test dosyası takımın tamamını kullanmıyor

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use postillion_doc::{RegistryRow, RowOp};
use postillion_server::rooms::ChatHub;
use postillion_server::{auth::Auth, App};
use postillion_sync::registry_room::{AppliedBatch, RegistryState, RegistryStore};
use postillion_sync::room::{ChatStore, Row, RoomState};
use postillion_sync::{ChatDocSink, CheckpointFetcher, SyncError};

pub const TOKEN: &str = "test-jetonu";

// ── bellek içi depo ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct MemStore {
    pub rows: Mutex<HashMap<String, Vec<Row>>>,
}

impl ChatStore for MemStore {
    fn state(&self, chat_id: &str) -> BoxFuture<'static, Result<RoomState, SyncError>> {
        let rows = self.rows.lock().unwrap();
        let chat = rows.get(chat_id).cloned().unwrap_or_default();
        let state = RoomState {
            head_seq: chat.last().map(|r| r.seq).unwrap_or(0),
            row_count: chat.len() as u64,
            row_bytes: chat.iter().map(|r| r.payload.len() as u64).sum(),
            ..RoomState::default()
        };
        Box::pin(async move { Ok(state) })
    }

    fn rows_after(
        &self,
        chat_id: &str,
        after: u64,
        exclude_device: Option<String>,
    ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>> {
        let rows = self.rows.lock().unwrap();
        let out: Vec<Row> = rows
            .get(chat_id)
            .into_iter()
            .flatten()
            .filter(|r| r.seq > after)
            .filter(|r| exclude_device.as_deref() != Some(r.device.as_str()))
            .cloned()
            .collect();
        Box::pin(async move { Ok(out) })
    }

    fn append(
        &self,
        chat_id: &str,
        device: String,
        batch_id: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<(u64, bool), SyncError>> {
        let mut rows = self.rows.lock().unwrap();
        let chat = rows.entry(chat_id.to_string()).or_default();
        if let Some(existing) = chat.iter().find(|r| r.batch_id == batch_id) {
            let seq = existing.seq;
            return Box::pin(async move { Ok((seq, true)) });
        }
        let seq = chat.last().map(|r| r.seq).unwrap_or(0) + 1;
        chat.push(Row {
            seq,
            device,
            batch_id,
            payload,
        });
        Box::pin(async move { Ok((seq, false)) })
    }
}

// ── bellek içi kayıt deposu ────────────────────────────────────────────────

/// Sunucu tarafındaki `PgRegistry` ile aynı sözleşme, Postgres'siz.
#[derive(Default)]
pub struct MemRegistry {
    rows: Mutex<Vec<RegistryRow>>,
    seq: Mutex<u64>,
}

impl RegistryStore for MemRegistry {
    fn state(&self, _org: &str) -> BoxFuture<'static, Result<RegistryState, SyncError>> {
        let state = RegistryState {
            seq: *self.seq.lock().unwrap(),
            gc_floor: 0,
        };
        Box::pin(async move { Ok(state) })
    }

    fn rows_since(
        &self,
        _org: &str,
        since: u64,
    ) -> BoxFuture<'static, Result<Vec<RegistryRow>, SyncError>> {
        let rows: Vec<RegistryRow> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.seq > since)
            .cloned()
            .collect();
        Box::pin(async move { Ok(rows) })
    }

    fn apply_batch(
        &self,
        _org: &str,
        ops: Vec<RowOp>,
    ) -> BoxFuture<'static, Result<AppliedBatch, SyncError>> {
        let mut rows = self.rows.lock().unwrap();
        let mut seq = self.seq.lock().unwrap();
        let next = *seq + 1;
        let mut touched: Vec<RegistryRow> = Vec::new();

        for op in &ops {
            let before = touched
                .iter()
                .find(|r| r.kind == op.kind && r.id == op.id)
                .cloned()
                .or_else(|| {
                    rows.iter()
                        .find(|r| r.kind == op.kind && r.id == op.id)
                        .cloned()
                });
            let (merged, changed) = postillion_doc::apply_op(before.as_ref(), op);
            let Some(mut merged) = merged.filter(|_| changed) else {
                continue;
            };
            merged.seq = next;
            touched.retain(|r| !(r.kind == merged.kind && r.id == merged.id));
            touched.push(merged);
        }

        let applied_seq = if touched.is_empty() { *seq } else { next };
        if !touched.is_empty() {
            for row in &touched {
                rows.retain(|r| !(r.kind == row.kind && r.id == row.id));
                rows.push(row.clone());
            }
            *seq = next;
        }
        let result = AppliedBatch { rows: touched, seq: applied_seq };
        Box::pin(async move { Ok(result) })
    }
}

// ── istemci tarafı sahteleri ───────────────────────────────────────────────

/// Uygulanan satırları biriktiren, doc yerine geçen alıcı.
#[derive(Default)]
pub struct Recorder {
    applied: Mutex<Vec<(u64, Vec<u8>)>>,
}

impl Recorder {
    pub fn payloads(&self) -> Vec<Vec<u8>> {
        self.applied
            .lock()
            .unwrap()
            .iter()
            .map(|(_, bytes)| bytes.clone())
            .collect()
    }
}

impl ChatDocSink for Recorder {
    fn apply_row(&self, bytes: &[u8], cursor: u64) {
        self.applied.lock().unwrap().push((cursor, bytes.to_vec()));
    }
    fn apply_checkpoint(&self, _bytes: &[u8], _cursor: u64) -> Result<(), String> {
        Ok(())
    }
    fn contains_frontier(&self, _frontier: &[u8]) -> bool {
        true
    }
    fn advance_cursor(&self, _cursor: u64) {}
}

/// Aşama 1'de anlık görüntü yok; istemcinin bu yola sapmaması gerekiyor.
pub struct NoCheckpoint;

impl CheckpointFetcher for NoCheckpoint {
    fn fetch(&self) -> BoxFuture<'static, Result<Vec<u8>, SyncError>> {
        Box::pin(async { Err(SyncError::Protocol("anlık görüntü yok".into())) })
    }
}

// ── sunucuyu ayağa kaldırma ────────────────────────────────────────────────

pub struct Server {
    pub port: u16,
    pub store: Arc<MemStore>,
}

/// Bellek içi depolu sunucu.
pub async fn start() -> Server {
    let store = Arc::new(MemStore::default());
    let port = start_with(store.clone()).await;
    Server { port, store }
}

/// Kayıt uçlarını sınamak için sunucu — sohbet deposu kullanılmıyor.
pub async fn start_with_registry() -> Server {
    start().await
}

/// Verilen depoyla sunucu; portu döndürür.
pub async fn start_with(store: Arc<dyn ChatStore>) -> u16 {
    let app = App {
        store,
        registry: Arc::new(MemRegistry::default()),
        hub: ChatHub::new(),
        registry_hub: postillion_server::registry_ws::RegistryHub::new(),
        device_hub: postillion_server::device_room::DeviceHub::new(),
        auth: Auth::new(TOKEN),
    };
    // Port 0: çekirdek boş bir port veriyor, böylece testler paralel koşarken
    // sabit bir porta çakışmıyorlar.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, postillion_server::router(app))
            .await
            .unwrap();
    });
    port
}

pub fn ws_url(port: u16, chat: &str) -> String {
    format!("ws://127.0.0.1:{port}/chat2/{chat}/ws?token={TOKEN}")
}

/// Beklenen sonuç oluşana kadar kısa aralıklarla yoklar.
///
/// Sabit bir bekleme yavaş makinede kırılgan, hızlı makinede boşa zaman.
pub async fn eventually<T>(mut probe: impl FnMut() -> Option<T>) -> T {
    for _ in 0..200 {
        if let Some(value) = probe() {
            return value;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("beklenen durum 5 saniyede oluşmadı");
}

