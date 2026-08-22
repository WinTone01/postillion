//! `registry/:org/ws` — çalışma alanı kaydının WebSocket ucu.
//!
//! Sohbet odasıyla aynı iskelet ([`crate::rooms`]) ama iki farkla: çerçeveler
//! JSON METİN, ve yayılanlar satır değil kodlanmış çerçeveler — `rows` ile
//! `presence` aynı kanaldan geçiyor.
//!
//! Yayın GÖNDERENİ de kapsıyor. Sohbette kendi satırını geri almak gereksizdi;
//! burada gerekli: gönderenin op'u son-yazan-kazanır kuralında kaybetmiş
//! olabilir ve göstermesi gereken doğru, birleşmiş satır.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt as _, StreamExt as _};
use postillion_sync::registry_room::{IncomingFrame, RegistrySession, RegistryStore};

use crate::hub::Hub;

/// Kayıt odalarının yayın merkezi — taşınan şey kodlanmış JSON çerçeve.
pub type RegistryHub = Hub<String>;

pub async fn serve(
    socket: WebSocket,
    store: Arc<dyn RegistryStore>,
    hub: RegistryHub,
    org: String,
) {
    let (mut sink, mut stream) = socket.split();
    let mut session = RegistrySession::new();

    // Abonelik `hello` cevaplanmadan ÖNCE: aradaki pencerede başka bir cihaz
    // yazarsa o satır kaçmasın.
    let mut live = hub.subscribe(&org);

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else {
                    break;
                };
                let text = match message {
                    Message::Text(text) => text.to_string(),
                    // İstemci taşıma canlılığı için düz metin `"ping"`
                    // gönderiyor; protokol çerçevesi değil, yok sayılıyor.
                    Message::Ping(_) | Message::Pong(_) => continue,
                    Message::Close(_) => break,
                    _ => continue,
                };
                if text == "ping" {
                    continue;
                }

                let Ok(frame) = serde_json::from_str::<IncomingFrame>(&text) else {
                    tracing::warn!(org = %org, "kayıt: çözülemeyen çerçeve");
                    continue;
                };

                let reply = match session.respond(&*store, &org, frame).await {
                    Ok(reply) => reply,
                    Err(err) => {
                        tracing::warn!(org = %org, error = %err, "kayıt: çerçeve işlenemedi");
                        continue;
                    }
                };

                // Yayın ÖNCE: `rows` ack'ten önce gitmeli, yoksa gönderen
                // iyimser batch'ini emekliye ayırdığı an ile doğru durumun
                // geldiği an arasında bir titreme penceresi kalır.
                for frame in reply.broadcast {
                    hub.publish(&org, frame);
                }
                for frame in reply.to_sender {
                    if sink.send(Message::Text(frame.into())).await.is_err() {
                        return;
                    }
                }
            }

            broadcast = live.recv() => {
                match broadcast {
                    Ok(frame) => {
                        // `hello` almamış bir sokete yazmak, istemcinin
                        // beklediği `state`'ten önce satır göndermek olurdu.
                        if !session.ready() {
                            continue;
                        }
                        if sink.send(Message::Text(frame.into())).await.is_err() {
                            return;
                        }
                    }
                    // Geride kalmak kayıp DEĞİL: istemci yeniden bağlandığında
                    // imlecinden itibaren delta alıyor.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(org = %org, missed, "kayıt yayınında geride kalındı");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
