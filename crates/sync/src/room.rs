//! Sohbet odasının SUNUCU tarafı protokol mantığı.
//!
//! Cloudflare'deki `ChatRoom` bir CRDT sunucusu değil, saf röle: kaynağında
//! tek bir `LoroDoc` yok, anlık görüntüyü istemci üretiyor ve satırlar opak
//! bayt olarak akıyor. Sunucunun işi "ekle, oku, dağıt".
//!
//! Bu modül o mantığın kendisi ve `postillion-server` tarafından kullanılıyor.
//! İstemciyle **aynı crate'te** durması bilinçli: codec (`chat_frames`) tek
//! yerde kalıyor ve iki uç arasında protokol kayması imkânsız hale geliyor.
//! Aynı protokolün TypeScript ve Swift kopyaları elle senkron tutuluyordu
//! ("change both together" notu hâlâ codec'in başında duruyor); üçüncü bir
//! elle-senkron kopya eklemek yerine paylaşıyoruz.
//!
//! Katman ayrımı: [`ChatStore`] ne yapılacağını söylüyor, uygulaması sunucuda.
//! Protokol mantığı depo trait'i üzerinden yazıldığı için veritabanı olmadan
//! sınanabiliyor.

use futures::future::BoxFuture;

use crate::chat_frames as wire;
use crate::chat_frames::frame_type;
use crate::types::SyncError;

/// Kayıttan bir satır. `payload` opak bir loro güncellemesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub seq: u64,
    pub device: String,
    pub batch_id: String,
    pub payload: Vec<u8>,
}

/// Odanın özeti — protokolün STATE çerçevesinin içeriği.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoomState {
    pub head_seq: u64,
    pub seq_floor: u64,
    pub checkpoint_seq: u64,
    pub checkpoint_size: u64,
    pub row_count: u64,
    pub row_bytes: u64,
}

/// Adaptörün altındaki depo.
///
/// Trait olmasının sebebi test: protokol mantığı bellek içi bir depoyla
/// baştan sona sınanabiliyor, ağ olmadan.
pub trait ChatStore: Send + Sync + 'static {
    fn state(&self, chat_id: &str) -> BoxFuture<'static, Result<RoomState, SyncError>>;

    /// `after`'dan büyük satırlar, sıralı.
    fn rows_after(
        &self,
        chat_id: &str,
        after: u64,
        exclude_device: Option<String>,
    ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>>;

    /// Satırı ekler ve aldığı sırayı döndürür.
    ///
    /// `batch_id` daha önce yazılmışsa YENİDEN yazılmamalı: istemci
    /// bağlantı koptuğunda aynı gönderimi tekrarlıyor ve iki kez uygulanan
    /// bir güncelleme kaydı şişirirdi. Dönen `bool` "zaten vardı" demek ve
    /// ack'in `dup` alanına gidiyor.
    fn append(
        &self,
        chat_id: &str,
        device: String,
        batch_id: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<(u64, bool), SyncError>>;
}

/// Tek bir bağlantının durumu.
///
/// Neden durumlu: istemci cihaz kimliğini YALNIZCA `HELLO`'da bir kez
/// söylüyor. `PUSH` yalnızca `batchId` taşıyor, `ROWS_REQ` de yalnızca
/// `after` ve `excludeOwn` — ikisi de "hangi cihaz" bilgisini sunucunun
/// hatırlamasını bekliyor. Durumsuz bir adaptör bu yüzden satırları isimsiz
/// yazıyor ve `excludeOwn` süzgecini hiç uygulayamıyordu.
#[derive(Debug, Default)]
pub struct Session {
    device: String,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bağlantının cihaz kimliği; `HELLO` gelmeden boş.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Gelen bir istemci çerçevesine verilecek cevaplar.
    ///
    /// Sıra belirli ve yan etkisi yalnızca depoda — bu yüzden bellek içi bir
    /// depoyla baştan sona sınanabiliyor.
    pub async fn respond(
        &mut self,
        store: &dyn ChatStore,
        chat_id: &str,
        frame: &wire::WireFrame,
    ) -> Result<Vec<Vec<u8>>, SyncError> {
        self.dispatch(store, chat_id, frame).await
    }

    async fn dispatch(
        &mut self,
        store: &dyn ChatStore,
        chat_id: &str,
        frame: &wire::WireFrame,
    ) -> Result<Vec<Vec<u8>>, SyncError> {
    match frame.kind {
        frame_type::HELLO => {
            // Cihaz kimliği bir daha gelmiyor: burada tutulmazsa kaybolur.
            if let Some(device) = frame.header.get("device").and_then(|v| v.as_str()) {
                self.device = device.to_string();
            }
            let state = store.state(chat_id).await?;
            Ok(vec![encode_state(&state)])
        }

        frame_type::ROWS_REQ => {
            let header: RowsReq = serde_json::from_value(frame.header.clone())
                .map_err(|e| SyncError::Protocol(e.to_string()))?;
            let rows = store
                .rows_after(
                    chat_id,
                    header.after,
                    // `excludeOwn`: istemci kendi yazdığı satırları geri
                    // almıyor — zaten yerel doc'unda uygulanmış durumdalar.
                    // Hangi cihaz olduğu `HELLO`'dan biliniyor.
                    header.exclude_own.then(|| self.device.clone()),
                )
                .await?;

            let head = rows.last().map(|r| r.seq).unwrap_or(header.after);
            let mut out: Vec<Vec<u8>> = rows.iter().map(encode_row).collect();
            out.push(wire::encode(
                frame_type::ROWS_DONE,
                &serde_json::json!({ "headSeq": head }),
                &[],
            ));
            Ok(out)
        }

        frame_type::PUSH => {
            let header: Push = serde_json::from_value(frame.header.clone())
                .map_err(|e| SyncError::Protocol(e.to_string()))?;
            let (seq, dup) = store
                .append(
                    chat_id,
                    self.device.clone(),
                    header.batch_id.clone(),
                    frame.payload.clone(),
                )
                .await?;
            Ok(vec![wire::encode(
                frame_type::ACK,
                &serde_json::json!({ "batchId": header.batch_id, "seq": seq, "dup": dup }),
                &[],
            )])
        }

        // Canlılık yoklaması: depo dolaşmadan cevaplanıyor, amacı zaten
        // bağlantının ayakta olup olmadığını ölçmek.
        frame_type::PROBE => Ok(vec![wire::encode(
            frame_type::PROBE_OK,
            &serde_json::json!({}),
            &[],
        )]),

        // Presence bu taşımada yok: Postgres'te "kim bağlı" diye bir kavram
        // yok ve istemci cevapsızlığa zaten dayanıklı.
        frame_type::PRESENCE => Ok(Vec::new()),

        other => {
            tracing::debug!(kind = other, "supabase: beklenmeyen çerçeve, yok sayıldı");
            Ok(Vec::new())
        }
    }
    }
}

pub fn encode_state(state: &RoomState) -> Vec<u8> {
    wire::encode(
        frame_type::STATE,
        &serde_json::json!({
            "headSeq": state.head_seq,
            "seqFloor": state.seq_floor,
            "checkpointSeq": state.checkpoint_seq,
            "checkpointSize": state.checkpoint_size,
            "rowCount": state.row_count,
            "rowBytes": state.row_bytes,
        }),
        &[],
    )
}

pub fn encode_row(row: &Row) -> Vec<u8> {
    wire::encode(
        frame_type::ROW,
        &serde_json::json!({
            "seq": row.seq,
            "device": row.device,
            "batchId": row.batch_id,
        }),
        &row.payload,
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowsReq {
    after: u64,
    #[serde(default)]
    exclude_own: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Push {
    batch_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Bellek içi depo — protokol mantığı ağ olmadan sınanıyor.
    #[derive(Default)]
    struct MemStore {
        rows: Arc<Mutex<Vec<Row>>>,
        checkpoint: Arc<Mutex<Option<(u64, usize)>>>,
    }

    impl ChatStore for MemStore {
        fn state(&self, _chat: &str) -> BoxFuture<'static, Result<RoomState, SyncError>> {
            let rows = self.rows.lock().unwrap().clone();
            let cp = *self.checkpoint.lock().unwrap();
            Box::pin(async move {
                Ok(RoomState {
                    head_seq: rows.last().map(|r| r.seq).unwrap_or(0),
                    seq_floor: cp.map(|(s, _)| s).unwrap_or(0),
                    checkpoint_seq: cp.map(|(s, _)| s).unwrap_or(0),
                    checkpoint_size: cp.map(|(_, n)| n as u64).unwrap_or(0),
                    row_count: rows.len() as u64,
                    row_bytes: rows.iter().map(|r| r.payload.len() as u64).sum(),
                })
            })
        }

        fn rows_after(
            &self,
            _chat: &str,
            after: u64,
            exclude: Option<String>,
        ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>> {
            let rows = self.rows.lock().unwrap().clone();
            Box::pin(async move {
                Ok(rows
                    .into_iter()
                    .filter(|r| r.seq > after)
                    .filter(|r| exclude.as_deref() != Some(r.device.as_str()))
                    .collect())
            })
        }

        fn append(
            &self,
            _chat: &str,
            device: String,
            batch_id: String,
            payload: Vec<u8>,
        ) -> BoxFuture<'static, Result<(u64, bool), SyncError>> {
            let rows = self.rows.clone();
            Box::pin(async move {
                let mut rows = rows.lock().unwrap();
                if let Some(existing) = rows.iter().find(|r| r.batch_id == batch_id) {
                    return Ok((existing.seq, true));
                }
                let seq = rows.last().map(|r| r.seq).unwrap_or(0) + 1;
                rows.push(Row {
                    seq,
                    device,
                    batch_id,
                    payload,
                });
                Ok((seq, false))
            })
        }
    }

    fn lock_rows(store: &MemStore) -> Vec<Row> {
        store.rows.lock().unwrap().clone()
    }

    fn decode(bytes: &[u8]) -> wire::WireFrame {
        wire::decode(bytes).expect("çerçeve çözülmeli")
    }

    fn frame(kind: u8, header: serde_json::Value, payload: &[u8]) -> wire::WireFrame {
        decode(&wire::encode(kind, &header, payload))
    }

    /// `HELLO` göndermiş bir oturum — gerçek akışta istemci daima önce
    /// kendini tanıtıyor ve cihaz kimliği yalnızca orada geçiyor.
    async fn session_for(store: &MemStore, device: &str) -> Session {
        let mut session = Session::new();
        session
            .respond(
                store,
                "c1",
                &frame(
                    frame_type::HELLO,
                    serde_json::json!({ "cursor": 0, "device": device }),
                    &[],
                ),
            )
            .await
            .unwrap();
        session
    }

    #[tokio::test]
    async fn hello_bos_odada_sifir_durum_donduruyor() {
        let store = MemStore::default();
        let out = Session::new().respond(&store, "c1", &frame(frame_type::HELLO, serde_json::json!({}), &[]))
            .await
            .unwrap();

        assert_eq!(out.len(), 1);
        let state = decode(&out[0]);
        assert_eq!(state.kind, frame_type::STATE);
        let header: wire::StateHeader = serde_json::from_value(state.header).unwrap();
        assert_eq!(header.head_seq, 0);
        assert_eq!(header.row_count, 0);
    }

    #[tokio::test]
    async fn push_sira_veriyor_ve_tekrari_dup_isaretliyor() {
        let store = MemStore::default();

        let mut session = session_for(&store, "dev-a").await;
        let out = session
            .respond(
                &store,
                "c1",
                &frame(
                    frame_type::PUSH,
                    serde_json::json!({ "batchId": "b1" }),
                    b"guncelleme",
                ),
            )
            .await
            .unwrap();
        let ack: wire::AckHeader = serde_json::from_value(decode(&out[0]).header).unwrap();
        assert_eq!(ack.seq, 1);
        assert!(!ack.dup);
        assert_eq!(lock_rows(&store)[0].device, "dev-a", "cihaz HELLO'dan gelmeli");

        // Bağlantı koptuğunda istemci aynı gönderimi tekrarlıyor. İkinci kez
        // YAZILMAMALI — yoksa aynı güncelleme kayıtta iki kez durur.
        let out = session
            .respond(
                &store,
                "c1",
                &frame(
                    frame_type::PUSH,
                    serde_json::json!({ "batchId": "b1" }),
                    b"guncelleme",
                ),
            )
            .await
            .unwrap();
        let ack: wire::AckHeader = serde_json::from_value(decode(&out[0]).header).unwrap();
        assert_eq!(ack.seq, 1, "aynı sıra geri verilmeli");
        assert!(ack.dup, "tekrar dup işaretlenmeli");
        assert_eq!(store.rows.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rows_req_sonunda_daima_rows_done_var() {
        let store = MemStore::default();
        for (batch, device) in [("b1", "dev-a"), ("b2", "dev-b")] {
            let mut writer = session_for(&store, device).await;
            writer
                .respond(
                    &store,
                    "c1",
                    &frame(frame_type::PUSH, serde_json::json!({ "batchId": batch }), b"x"),
                )
                .await
                .unwrap();
        }

        let out = Session::new().respond(
            &store,
            "c1",
            &frame(
                frame_type::ROWS_REQ,
                serde_json::json!({ "after": 0, "excludeOwn": false }),
                &[],
            ),
        )
        .await
        .unwrap();

        assert_eq!(out.len(), 3, "iki satır + bitiş");
        assert_eq!(decode(&out[0]).kind, frame_type::ROW);
        assert_eq!(decode(&out[1]).kind, frame_type::ROW);
        let done = decode(&out[2]);
        assert_eq!(done.kind, frame_type::ROWS_DONE);
        assert_eq!(done.header["headSeq"], 2);

        // Boş cevapta bile bitiş çerçevesi gelmeli: istemci onu bekliyor ve
        // gelmezse yakalama aşamasında takılı kalır.
        let out = Session::new().respond(
            &store,
            "c1",
            &frame(
                frame_type::ROWS_REQ,
                serde_json::json!({ "after": 99, "excludeOwn": false }),
                &[],
            ),
        )
        .await
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(decode(&out[0]).kind, frame_type::ROWS_DONE);
        assert_eq!(decode(&out[0]).header["headSeq"], 99, "boşsa istenen nokta");
    }

    #[tokio::test]
    async fn exclude_own_yalnizca_kendi_cihazini_suzuyor() {
        let store = MemStore::default();
        for (batch, device) in [("b1", "dev-a"), ("b2", "dev-b")] {
            let mut writer = session_for(&store, device).await;
            writer
                .respond(
                    &store,
                    "c1",
                    &frame(frame_type::PUSH, serde_json::json!({ "batchId": batch }), b"x"),
                )
                .await
                .unwrap();
        }

        let mut reader = session_for(&store, "dev-a").await;
        let out = reader
            .respond(
                &store,
                "c1",
                &frame(
                    frame_type::ROWS_REQ,
                    serde_json::json!({ "after": 0, "excludeOwn": true }),
                    &[],
                ),
            )
            .await
            .unwrap();

        // dev-a'nın satırı düşüyor, dev-b'ninki kalıyor.
        assert_eq!(out.len(), 2, "bir satır + bitiş");
        let row: wire::RowHeader = serde_json::from_value(decode(&out[0]).header).unwrap();
        assert_eq!(row.device, "dev-b");
    }

    #[tokio::test]
    async fn satir_yuku_bozulmadan_tasiniyor() {
        // Yük opak: sunucu içine bakmıyor, bayt bayt aynı dönmeli. Bozulursa
        // loro güncellemesi uygulanamaz ve sohbet sessizce eksik kalır.
        let store = MemStore::default();
        let payload: Vec<u8> = (0u8..=255).collect();
        let mut writer = session_for(&store, "dev-a").await;
        writer
            .respond(
                &store,
                "c1",
                &frame(frame_type::PUSH, serde_json::json!({ "batchId": "b1" }), &payload),
            )
            .await
            .unwrap();

        let out = Session::new().respond(
            &store,
            "c1",
            &frame(
                frame_type::ROWS_REQ,
                serde_json::json!({ "after": 0, "excludeOwn": false }),
                &[],
            ),
        )
        .await
        .unwrap();
        assert_eq!(decode(&out[0]).payload, payload);
    }

    #[tokio::test]
    async fn probe_depoya_gitmeden_cevaplaniyor() {
        let store = MemStore::default();
        let out = Session::new().respond(&store, "c1", &frame(frame_type::PROBE, serde_json::json!({}), &[]))
            .await
            .unwrap();
        assert_eq!(decode(&out[0]).kind, frame_type::PROBE_OK);
    }

    #[tokio::test]
    async fn taninmayan_cerceve_baglantiyi_dusurmuyor() {
        // İleride eklenen bir çerçeve türü eski istemciyi kırmamalı.
        let store = MemStore::default();
        let out = Session::new().respond(&store, "c1", &frame(0x7f, serde_json::json!({}), &[]))
            .await
            .unwrap();
        assert!(out.is_empty());
    }
}
