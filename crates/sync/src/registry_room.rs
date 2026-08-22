//! Çalışma alanı kaydının SUNUCU tarafı protokol mantığı.
//!
//! Kayıt yalnızca GÜNCEL DURUMU tutuyor: bir satır alan torbası, her alan son
//! yazımının saatini taşıyor ve bir yazım ancak saati depodakini yendiğinde
//! uygulanıyor. Geçmiş, doğru olmaktan çıktığı anda atılıyor — odayı
//! tıkanmaz kılan özellik bu: hiçbir şey büyümüyor, hiçbir şey tekrarlanmıyor.
//!
//! Birleştirme mantığı burada DEĞİL: [`postillion_doc::apply_op`] hem
//! istemcide hem burada aynı kodu çalıştırıyor. Bu modül yalnızca taşıma —
//! hangi çerçeveye ne cevap verileceği ve neyin yayılacağı.
//!
//! [`ServerFrame`] doğrudan istemcinin tanımından geliyor; istemcinin
//! gönderdiği çerçevelerin sahipli ikizi ise burada, çünkü oradaki alanlar
//! gönderim yolunda kopya olmasın diye ödünç alınmış. İkisinin aynı teli
//! konuştuğu bu dosyadaki gidiş-dönüş testiyle bağlanıyor.

use std::collections::HashMap;

use futures::future::BoxFuture;
use postillion_doc::{RegistryRow, RowOp};
use serde::{Deserialize, Serialize};

use crate::registry::ServerFrame;
use crate::types::SyncError;

/// İstemci çerçevesinin sahipli ikizi.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum IncomingFrame {
    Hello {
        #[serde(default)]
        cursor: Option<u64>,
        device: String,
    },
    Push {
        batch: String,
        ops: Vec<RowOp>,
    },
    Presence {
        at: i64,
    },
    Probe,
}

/// Odanın özeti.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegistryState {
    pub seq: u64,
    /// Bu sıranın altındaki imleçler tam yeniden eşitleme alıyor: altındaki
    /// mezar taşları toplanmış olabilir ve delta artık doğruyu anlatmaz.
    pub gc_floor: u64,
}

/// Bir batch'in uygulanma sonucu.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedBatch {
    /// Yayılacak birleşmiş satırlar. Boşsa hiçbir op durumu değiştirmedi.
    pub rows: Vec<RegistryRow>,
    /// Ack'e giden sıra: batch bir şey değiştirdiyse yeni sıra, yoksa mevcut.
    pub seq: u64,
}

pub trait RegistryStore: Send + Sync + 'static {
    fn state(&self, org: &str) -> BoxFuture<'static, Result<RegistryState, SyncError>>;

    /// `since`'dan büyük sıralı satırlar. `since = 0` bütün tablo.
    fn rows_since(
        &self,
        org: &str,
        since: u64,
    ) -> BoxFuture<'static, Result<Vec<RegistryRow>, SyncError>>;

    /// Bir batch'i ATOMİK uygular.
    ///
    /// Atomiklik şart: batch'ler işlemsel (art arda silmeler taşıyorlar) ve
    /// yarısı uygulanmış bir batch kaydı tutarsız bırakır.
    fn apply_batch(
        &self,
        org: &str,
        ops: Vec<RowOp>,
    ) -> BoxFuture<'static, Result<AppliedBatch, SyncError>>;
}

/// Tek bir bağlantının durumu.
///
/// Cihaz kimliği yalnızca `hello`'da geliyor; `push` ve `presence` sunucunun
/// hatırlamasını bekliyor.
#[derive(Debug, Default)]
pub struct RegistrySession {
    device: String,
    ready: bool,
}

/// `respond` çıktısı: bu sokete gidenler ve odaya yayılacaklar ayrı.
#[derive(Debug, Default)]
pub struct Reply {
    /// Yalnızca bu bağlantıya.
    pub to_sender: Vec<String>,
    /// Odadaki HERKESE — gönderen dahil.
    pub broadcast: Vec<String>,
}

impl RegistrySession {
    pub fn new() -> Self {
        Self::default()
    }

    /// `hello` gidip gelmeden hazır bir oturum.
    ///
    /// HTTP push yolu için: orada cihaz kimliği sorgu dizesinden geliyor ve
    /// yalnızca oturumu hazır işaretlemek uğruna `hello` çalıştırmak, her
    /// push'ta bütün tabloyu okumak demekti.
    pub fn ready_for(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            ready: true,
        }
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// `hello` geldi mi. Gelmeden hiçbir şey kabul edilmiyor.
    pub fn ready(&self) -> bool {
        self.ready
    }

    pub async fn respond(
        &mut self,
        store: &dyn RegistryStore,
        org: &str,
        frame: IncomingFrame,
    ) -> Result<Reply, SyncError> {
        match frame {
            IncomingFrame::Hello { cursor, device } => {
                self.device = device;
                self.ready = true;

                let state = store.state(org).await?;
                // Tam yeniden eşitleme üç durumda: imleç hiç yok, mezar taşı
                // ufkunun altında kalmış, ya da sunucunun sırasını AŞIYOR.
                // Sonuncusu sunucunun durum kaybettiğini söylüyor (sıfırlama
                // ya da silme) ve istemcinin tablosunu değiştirmesi gerekiyor.
                let full = match cursor {
                    None => true,
                    Some(c) => c < state.gc_floor || c > state.seq,
                };
                let rows = store
                    .rows_since(org, if full { 0 } else { cursor.unwrap_or(0) })
                    .await?;

                Ok(Reply {
                    to_sender: vec![encode(&ServerFrame::State {
                        seq: state.seq,
                        full,
                        gc_floor: state.gc_floor,
                        rows,
                        // Presence bu taşımada tutulmuyor: canlı bir ölçü ve
                        // yeniden bağlanan istemciler beat'lerini kendileri
                        // gönderiyor.
                        presence: HashMap::new(),
                    })?],
                    broadcast: Vec::new(),
                })
            }

            IncomingFrame::Push { batch, ops } => {
                if !self.ready {
                    return Ok(Reply {
                        to_sender: vec![encode(&ServerFrame::Error {
                            code: "bad_push".into(),
                            message: "hello first / malformed push".into(),
                        })?],
                        broadcast: Vec::new(),
                    });
                }

                let applied = store.apply_batch(org, ops).await?;
                let count = applied.rows.len() as u64;

                let mut reply = Reply::default();
                if !applied.rows.is_empty() {
                    // Satırlar ack'ten ÖNCE ve GÖNDEREN DAHİL herkese:
                    // gönderenin op'u son-yazan-kazanır kuralında kaybetmiş
                    // olabilir ve göstermesi gereken doğru, birleşmiş satır.
                    // Ack'ten sonra göndermek, gönderenin iyimser batch'ini
                    // emekliye ayırdığı an ile doğru durumun geldiği an
                    // arasında bir titreme penceresi bırakırdı.
                    reply.broadcast.push(encode(&ServerFrame::Rows {
                        seq: applied.seq,
                        rows: applied.rows,
                    })?);
                }
                reply.to_sender.push(encode(&ServerFrame::Ack {
                    batch,
                    seq: applied.seq,
                    applied: count,
                })?);
                Ok(reply)
            }

            IncomingFrame::Presence { at } => {
                if !self.ready {
                    return Ok(Reply::default());
                }
                Ok(Reply {
                    to_sender: Vec::new(),
                    broadcast: vec![encode(&ServerFrame::Presence {
                        device: self.device.clone(),
                        at,
                    })?],
                })
            }

            // Canlılık yoklaması: depoya gidiyor çünkü amacı odanın gerçekten
            // cevap verebildiğini kanıtlamak. Bellekten dönen bir cevap
            // sokedin ayakta olduğunu söyler, odanın sağlığını değil.
            IncomingFrame::Probe => {
                let state = store.state(org).await?;
                Ok(Reply {
                    to_sender: vec![encode(&ServerFrame::ProbeOk { seq: state.seq })?],
                    broadcast: Vec::new(),
                })
            }
        }
    }
}

fn encode(frame: &ServerFrame) -> Result<String, SyncError> {
    serde_json::to_string(frame).map_err(|e| SyncError::Protocol(e.to_string()))
}

/// Bir satır kümesini `rows` çerçevesi olarak kodlar — HTTP yolu ve yayın için.
pub fn encode_rows(seq: u64, rows: Vec<RegistryRow>) -> Result<String, SyncError> {
    encode(&ServerFrame::Rows { seq, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ClientFrame;
    use std::sync::Mutex;

    /// İstemcinin YAZDIĞINI sunucu OKUYABİLMELİ.
    ///
    /// İki taraf ayrı tiplerle çalışıyor (istemcininki gönderim yolunda kopya
    /// olmasın diye ödünç alınmış). Bu test onları bağlayan tek şey: biri
    /// değişip öteki değişmezse burada düşer.
    #[test]
    fn istemcinin_yazdigini_sunucu_okuyor() {
        let op = RowOp {
            kind: "chat".into(),
            id: "c1".into(),
            op: postillion_doc::OpKind::Upsert,
            set: Some([("title".to_string(), serde_json::json!("merhaba"))].into()),
            hlc: "0000000000001-000001-devA".into(),
            clocks: None,
        };

        let cases = vec![
            serde_json::to_string(&ClientFrame::Hello {
                cursor: Some(7),
                device: "devA",
            })
            .unwrap(),
            serde_json::to_string(&ClientFrame::Hello {
                cursor: None,
                device: "devA",
            })
            .unwrap(),
            serde_json::to_string(&ClientFrame::Push {
                batch: "b1",
                ops: std::slice::from_ref(&op),
            })
            .unwrap(),
            serde_json::to_string(&ClientFrame::Presence { at: 1_700_000 }).unwrap(),
            serde_json::to_string(&ClientFrame::Probe).unwrap(),
        ];

        let decoded: Vec<IncomingFrame> = cases
            .iter()
            .map(|json| {
                serde_json::from_str(json)
                    .unwrap_or_else(|e| panic!("sunucu çözemedi: {json} — {e}"))
            })
            .collect();

        assert_eq!(
            decoded[0],
            IncomingFrame::Hello {
                cursor: Some(7),
                device: "devA".into()
            }
        );
        assert_eq!(
            decoded[1],
            IncomingFrame::Hello {
                cursor: None,
                device: "devA".into()
            }
        );
        assert_eq!(
            decoded[2],
            IncomingFrame::Push {
                batch: "b1".into(),
                ops: vec![op]
            }
        );
        assert_eq!(decoded[3], IncomingFrame::Presence { at: 1_700_000 });
        assert_eq!(decoded[4], IncomingFrame::Probe);
    }

    // ── protokol davranışı ─────────────────────────────────────────────────

    #[derive(Default)]
    struct MemStore {
        rows: Mutex<Vec<RegistryRow>>,
        seq: Mutex<u64>,
        gc_floor: Mutex<u64>,
    }

    impl RegistryStore for MemStore {
        fn state(&self, _org: &str) -> BoxFuture<'static, Result<RegistryState, SyncError>> {
            let state = RegistryState {
                seq: *self.seq.lock().unwrap(),
                gc_floor: *self.gc_floor.lock().unwrap(),
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
            let result = AppliedBatch {
                rows: touched,
                seq: applied_seq,
            };
            Box::pin(async move { Ok(result) })
        }
    }

    fn upsert(id: &str, title: &str, hlc: &str) -> RowOp {
        RowOp {
            kind: "chat".into(),
            id: id.into(),
            op: postillion_doc::OpKind::Upsert,
            set: Some([("title".to_string(), serde_json::json!(title))].into()),
            hlc: hlc.into(),
            clocks: None,
        }
    }

    fn parse(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("cevap JSON")
    }

    #[tokio::test]
    async fn hello_imlecsizken_tam_tablo_veriyor() {
        let store = MemStore::default();
        let mut session = RegistrySession::new();

        let reply = session
            .respond(
                &store,
                "org",
                IncomingFrame::Hello {
                    cursor: None,
                    device: "devA".into(),
                },
            )
            .await
            .unwrap();

        let state = parse(&reply.to_sender[0]);
        assert_eq!(state["t"], "state");
        assert_eq!(state["full"], true, "imleçsiz hello tam tablo almalı");
        assert!(session.ready());
        assert_eq!(session.device(), "devA");
    }

    /// Sunucunun sırasını AŞAN bir imleç, sunucunun durum kaybettiğini
    /// söylüyor; istemci tablosunu değiştirmeli.
    #[tokio::test]
    async fn sunucunun_gerisinde_kalan_imlec_tam_tablo_tetikliyor() {
        let store = MemStore::default();
        *store.seq.lock().unwrap() = 3;
        let mut session = RegistrySession::new();

        let reply = session
            .respond(
                &store,
                "org",
                IncomingFrame::Hello {
                    cursor: Some(99),
                    device: "devA".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(parse(&reply.to_sender[0])["full"], true);
    }

    /// Birleşmiş satırlar GÖNDEREN DAHİL herkese ve ack'ten ÖNCE.
    #[tokio::test]
    async fn satirlar_gonderene_de_ve_ackten_once_gidiyor() {
        let store = MemStore::default();
        let mut session = RegistrySession::new();
        session
            .respond(
                &store,
                "org",
                IncomingFrame::Hello {
                    cursor: None,
                    device: "devA".into(),
                },
            )
            .await
            .unwrap();

        let reply = session
            .respond(
                &store,
                "org",
                IncomingFrame::Push {
                    batch: "b1".into(),
                    ops: vec![upsert("c1", "merhaba", "0000000000001-000001-devA")],
                },
            )
            .await
            .unwrap();

        // Gönderenin op'u son-yazan-kazanır kuralında kaybetmiş olabilir;
        // göstermesi gereken doğru, birleşmiş satır.
        assert_eq!(reply.broadcast.len(), 1, "satırlar yayına da düşmeli");
        let rows = parse(&reply.broadcast[0]);
        assert_eq!(rows["t"], "rows");
        assert_eq!(rows["rows"][0]["id"], "c1");

        let ack = parse(&reply.to_sender[0]);
        assert_eq!(ack["t"], "ack");
        assert_eq!(ack["batch"], "b1");
        assert_eq!(ack["applied"], 1);
    }

    /// Hiçbir şeyi değiştirmeyen batch yayın yapmamalı ama ACK ALMALI:
    /// ack gelmezse istemci iyimser batch'ini sonsuza kadar tutar.
    #[tokio::test]
    async fn degistirmeyen_batch_yayin_yapmiyor_ama_ack_aliyor() {
        let store = MemStore::default();
        let mut session = RegistrySession::new();
        session
            .respond(
                &store,
                "org",
                IncomingFrame::Hello {
                    cursor: None,
                    device: "devA".into(),
                },
            )
            .await
            .unwrap();
        let op = upsert("c1", "merhaba", "0000000000001-000001-devA");
        session
            .respond(
                &store,
                "org",
                IncomingFrame::Push {
                    batch: "b1".into(),
                    ops: vec![op.clone()],
                },
            )
            .await
            .unwrap();

        // Aynı op yeniden: saat depodakini yenmiyor, hiçbir şey değişmiyor.
        let reply = session
            .respond(
                &store,
                "org",
                IncomingFrame::Push {
                    batch: "b2".into(),
                    ops: vec![op],
                },
            )
            .await
            .unwrap();

        assert!(reply.broadcast.is_empty(), "değişiklik yoksa yayın yok");
        let ack = parse(&reply.to_sender[0]);
        assert_eq!(ack["t"], "ack");
        assert_eq!(ack["applied"], 0);
    }

    /// `hello` gelmeden push kabul edilmemeli: cihaz kimliği oradan geliyor.
    #[tokio::test]
    async fn hellosuz_push_reddediliyor() {
        let store = MemStore::default();
        let mut session = RegistrySession::new();

        let reply = session
            .respond(
                &store,
                "org",
                IncomingFrame::Push {
                    batch: "b1".into(),
                    ops: vec![upsert("c1", "x", "0000000000001-000001-devA")],
                },
            )
            .await
            .unwrap();

        assert_eq!(parse(&reply.to_sender[0])["t"], "error");
        assert!(reply.broadcast.is_empty());
    }

    #[tokio::test]
    async fn presence_yayiliyor_gonderene_donmuyor() {
        let store = MemStore::default();
        let mut session = RegistrySession::new();
        session
            .respond(
                &store,
                "org",
                IncomingFrame::Hello {
                    cursor: None,
                    device: "devA".into(),
                },
            )
            .await
            .unwrap();

        let reply = session
            .respond(&store, "org", IncomingFrame::Presence { at: 42 })
            .await
            .unwrap();

        assert!(reply.to_sender.is_empty());
        let beat = parse(&reply.broadcast[0]);
        assert_eq!(beat["t"], "presence");
        assert_eq!(beat["device"], "devA");
        assert_eq!(beat["at"], 42);
    }

    #[tokio::test]
    async fn probe_odanin_sirasiyla_cevapliyor() {
        let store = MemStore::default();
        *store.seq.lock().unwrap() = 5;
        let mut session = RegistrySession::new();

        let reply = session
            .respond(&store, "org", IncomingFrame::Probe)
            .await
            .unwrap();
        let ok = parse(&reply.to_sender[0]);
        assert_eq!(ok["t"], "probe-ok");
        assert_eq!(ok["seq"], 5);
    }
}
