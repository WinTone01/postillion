//! `chat2/:chatId/ws` — sohbet odasının WebSocket ucu.
//!
//! İki kaynak aynı sokete yazıyor ve tek bir `select!` içinde toplanıyorlar
//! (iki ayrı görev soketi paylaşamazdı):
//!
//! 1. **İstemcinin kendi çerçeveleri** — `Session` cevaplıyor.
//! 2. **Diğer cihazların yazdıkları** — [`Hub`] duyuruyor.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::future::BoxFuture;
use futures::{SinkExt as _, StreamExt as _};
use postillion_sync::chat_frames as wire;
use postillion_sync::keepalive;
use postillion_sync::room::{ChatStore, Row, RoomState, Session};
use postillion_sync::SyncError;

use crate::hub::Hub;

/// Sohbet odalarının yayın merkezi.
pub type ChatHub = Hub<Row>;

/// Yazılan satırı yayına da düşüren depo sarmalayıcı.
///
/// Alternatifi, yazmadan önce ve sonra baş sırayı sorup farkı okumaktı —
/// mesaj başına üç fazladan gidiş dönüş. Burada satırın alanları zaten
/// elimizde ve `append` sırayı döndürüyor; duyuru bedava.
pub struct Publishing {
    inner: Arc<dyn ChatStore>,
    hub: ChatHub,
}

impl ChatStore for Publishing {
    fn state(&self, chat_id: &str) -> BoxFuture<'static, Result<RoomState, SyncError>> {
        self.inner.state(chat_id)
    }

    fn rows_after(
        &self,
        chat_id: &str,
        after: u64,
        exclude_device: Option<String>,
    ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>> {
        self.inner.rows_after(chat_id, after, exclude_device)
    }

    fn append(
        &self,
        chat_id: &str,
        device: String,
        batch_id: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<(u64, bool), SyncError>> {
        let inner = self.inner.clone();
        let hub = self.hub.clone();
        let chat = chat_id.to_string();
        Box::pin(async move {
            let (seq, dup) = inner
                .append(&chat, device.clone(), batch_id.clone(), payload.clone())
                .await?;

            // Tekrarlanan gönderim yeni satır açmadı; ilk yazımda zaten
            // duyurulmuştu, ikinci kez duyurmak abonelere kopya gönderirdi.
            if !dup {
                hub.publish(
                    &chat,
                    Row {
                        seq,
                        device,
                        batch_id,
                        payload,
                    },
                );
            }
            Ok((seq, dup))
        })
    }
}

/// Depoyu yayın sarmalayıcısıyla giydirir. HTTP yolu da kullanıyor: oradan
/// gelen bir satır da bağlı cihazlara anında ulaşmalı.
pub fn publishing(inner: Arc<dyn ChatStore>, hub: ChatHub) -> Arc<dyn ChatStore> {
    Arc::new(Publishing { inner, hub })
}

/// Tek bir bağlantıyı sonuna kadar sürer.
pub async fn serve(socket: WebSocket, store: Arc<dyn ChatStore>, hub: ChatHub, chat_id: String) {
    let (mut sink, mut stream) = socket.split();
    let mut session = Session::new();

    // Abonelik ÖNCE açılıyor: `HELLO` cevaplanırken başka bir cihaz yazarsa o
    // satır kaçmasın. Sıra tersine olsaydı yakalama ile abonelik arasında bir
    // pencere kalırdı.
    let mut live = hub.subscribe(&chat_id);

    let store = publishing(store, hub.clone());

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else {
                    break; // kapandı ya da hata
                };
                // Protokol tamamen ikili; tek metin çerçevesi canlılık
                // yoklaması ve KARŞILIK BEKLİYOR — cevapsız kalırsa istemci
                // sağlıklı soketi sessizlik kirasıyla kapatıyor.
                if let Message::Text(text) = &message {
                    if keepalive::is_ping(text) {
                        if sink.send(Message::Text(keepalive::PONG.into())).await.is_err() {
                            return;
                        }
                    }
                    continue;
                }
                let Message::Binary(bytes) = message else {
                    continue;
                };
                let Some(frame) = wire::decode(&bytes) else {
                    tracing::warn!(chat = %chat_id, "çözülemeyen çerçeve");
                    continue;
                };

                let replies = match session.respond(&*store, &chat_id, &frame).await {
                    Ok(replies) => replies,
                    Err(err) => {
                        tracing::warn!(chat = %chat_id, error = %err, "çerçeve işlenemedi");
                        continue;
                    }
                };
                for reply in replies {
                    if sink.send(Message::Binary(reply.into())).await.is_err() {
                        return;
                    }
                }
            }

            broadcast = live.recv() => {
                match broadcast {
                    Ok(row) => {
                        // Kendi satırını geri göndermek gereksiz: istemci onu
                        // zaten uyguladı ve onayla imlecini ilerletti.
                        if row.device == session.device() {
                            continue;
                        }
                        let frame = postillion_sync::room::encode_row(&row);
                        if sink.send(Message::Binary(frame.into())).await.is_err() {
                            return;
                        }
                    }
                    // Yavaş abone geride kaldı. Kayıp DEĞİL: istemci
                    // imlecinden itibaren depodan yakalıyor; canlı yol bir
                    // hızlandırma, doğruluk kaynağı değil.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(chat = %chat_id, missed, "yayında geride kalındı");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
