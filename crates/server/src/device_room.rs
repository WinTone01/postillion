//! `device/:deviceId/ws` — cihazlar arası RPC rölesi.
//!
//! Bir cihazın motoru `role=host` ile bağlanıyor, ona iş yaptırmak isteyenler
//! `role=client&connId=…` ile. Röle çerçeveleri arada taşıyor ve içeriğe hiç
//! bakmıyor: RPC, terminal akışı, gelecekteki her şey aynı bayt borusundan
//! geçiyor.
//!
//! Yönlendirme kuralları (belirtim: `crates/rpc/src/device_room.rs` başlığı):
//! - istemci → röle: `from = connId` damgalanıyor, host soketine iletiliyor
//! - host → röle: `to = connId` taşımak ZORUNDA; röle yönlendirme anahtarlarını
//!   söküp teslim ediyor
//!
//! Yayın değil hedefli teslimat, o yüzden bağlantı başına kanal var —
//! `broadcast` herkese kopyalardı.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt as _, StreamExt as _};
use postillion_sync::keepalive;
use postillion_rpc::device_room::{
    decode_device_frame, encode_device_frame, DeviceFrameHeader, HOST_CLOSED, HOST_OFFLINE,
    RELAY_KIND,
};
use tokio::sync::mpsc;

/// Bağlantı başına çıkış kanalı.
type Outbox = mpsc::Sender<Vec<u8>>;

#[derive(Default)]
struct Room {
    /// Tek host. Yeni bir host katıldığında eskisinin yerini alıyor:
    /// ağı sessizce ölmüş bir host (kapak kapandı, NAT bağlantıyı topladı)
    /// soket olarak açık görünmeye devam ediyor ve ona yönlendirmek her
    /// istemci çerçevesini bir cesede göndermek olurdu.
    host: Option<Outbox>,
    clients: HashMap<String, Outbox>,
}

#[derive(Clone, Default)]
pub struct DeviceHub {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
}

/// Röle kontrol çerçevesi — `{"error": kod}`.
fn control(stream: &str, code: &str) -> Vec<u8> {
    let header = DeviceFrameHeader::new(stream, RELAY_KIND);
    let payload = serde_json::json!({ "error": code }).to_string();
    encode_device_frame(&header, payload.as_bytes()).unwrap_or_default()
}

impl DeviceHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Host'u kaydeder; varsa eskisini döndürür (kapatılması için).
    fn join_host(&self, device: &str, outbox: Outbox) -> Option<Outbox> {
        let mut rooms = self.rooms.lock().expect("röle kilidi");
        rooms.entry(device.to_string()).or_default().host.replace(outbox)
    }

    fn join_client(&self, device: &str, conn_id: &str, outbox: Outbox) {
        let mut rooms = self.rooms.lock().expect("röle kilidi");
        rooms
            .entry(device.to_string())
            .or_default()
            .clients
            .insert(conn_id.to_string(), outbox);
    }

    fn host_outbox(&self, device: &str) -> Option<Outbox> {
        self.rooms
            .lock()
            .expect("röle kilidi")
            .get(device)
            .and_then(|r| r.host.clone())
    }

    fn client_outbox(&self, device: &str, conn_id: &str) -> Option<Outbox> {
        self.rooms
            .lock()
            .expect("röle kilidi")
            .get(device)
            .and_then(|r| r.clients.get(conn_id).cloned())
    }

    fn client_outboxes(&self, device: &str) -> Vec<Outbox> {
        self.rooms
            .lock()
            .expect("röle kilidi")
            .get(device)
            .map(|r| r.clients.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Host ayrıldı. Yalnızca hâlâ BİZ kayıtlıysak siliyoruz: bizi devralmış
    /// yeni bir host varsa onu kaldırmak odayı boşaltırdı.
    fn leave_host(&self, device: &str, ours: &Outbox) {
        let mut rooms = self.rooms.lock().expect("röle kilidi");
        let Some(room) = rooms.get_mut(device) else {
            return;
        };
        if room.host.as_ref().is_some_and(|h| h.same_channel(ours)) {
            room.host = None;
        }
        if room.host.is_none() && room.clients.is_empty() {
            rooms.remove(device);
        }
    }

    fn leave_client(&self, device: &str, conn_id: &str) {
        let mut rooms = self.rooms.lock().expect("röle kilidi");
        let Some(room) = rooms.get_mut(device) else {
            return;
        };
        room.clients.remove(conn_id);
        if room.host.is_none() && room.clients.is_empty() {
            rooms.remove(device);
        }
    }

    /// Açık oda sayısı — teşhis ve test için.
    pub fn open_rooms(&self) -> usize {
        self.rooms.lock().expect("röle kilidi").len()
    }
}

/// Tek bir bağlantıyı sonuna kadar sürer.
pub async fn serve(socket: WebSocket, hub: DeviceHub, device: String, role: Role, conn_id: String) {
    let (mut sink, mut stream) = socket.split();
    let (outbox, mut inbox) = mpsc::channel::<Vec<u8>>(64);

    let superseded = match role {
        Role::Host => hub.join_host(&device, outbox.clone()),
        Role::Client => {
            hub.join_client(&device, &conn_id, outbox.clone());
            None
        }
    };
    // Devralınan host'a kapandığını bildiriyoruz; kanalı düşmüş olabilir,
    // hata önemsiz.
    if let Some(old) = superseded {
        let _ = old.try_send(control(RELAY_KIND, HOST_CLOSED));
    }

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                // Metin `"ping"` canlılık yoklaması, protokol çerçevesi değil.
                // Cevapsız bırakmak host'un soketini 25 saniyede bir kapattırıyor
                // ve cihaz kalıcı olarak "reconnecting"de kalıyor.
                if let Message::Text(text) = &message {
                    if keepalive::is_ping(text)
                        && sink.send(Message::Text(keepalive::PONG.into())).await.is_err()
                    {
                        break;
                    }
                    continue;
                }
                let Message::Binary(bytes) = message else { continue };

                let Ok((header, payload)) = decode_device_frame(&bytes) else {
                    tracing::warn!(device = %device, "röle: çözülemeyen çerçeve");
                    continue;
                };

                match role {
                    Role::Client => {
                        // Kaynak damgası SUNUCUDA basılıyor: istemcinin
                        // kendi söylediğine güvenmek, bir istemcinin başka
                        // bir istemcinin kimliğiyle konuşabilmesi demekti.
                        let mut stamped = header.clone();
                        stamped.to = None;
                        stamped.from = Some(conn_id.clone());

                        match hub.host_outbox(&device) {
                            Some(host) => {
                                if let Ok(frame) = encode_device_frame(&stamped, &payload) {
                                    let _ = host.send(frame).await;
                                }
                            }
                            // Canlı host yoksa istemci ASILMAMALI: zaman
                            // aşımını beklemek yerine hemen öğrenmeli.
                            None => {
                                let _ = outbox
                                    .send(control(&header.s, HOST_OFFLINE))
                                    .await;
                            }
                        }
                    }
                    Role::Host => {
                        let Some(target) = header.to.as_deref().map(str::to_owned) else {
                            tracing::warn!(device = %device, "röle: hedefsiz host çerçevesi");
                            continue;
                        };
                        // Yönlendirme anahtarları SÖKÜLÜYOR: istemci onları
                        // beklemiyor ve `to` alanı odanın iç meselesi.
                        let mut clean = header.clone();
                        clean.to = None;
                        clean.from = None;
                        if let Some(client) = hub.client_outbox(&device, &target) {
                            if let Ok(frame) = encode_device_frame(&clean, &payload) {
                                let _ = client.send(frame).await;
                            }
                        }
                    }
                }
            }

            outgoing = inbox.recv() => {
                let Some(frame) = outgoing else { break };
                if sink.send(Message::Binary(frame.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    match role {
        Role::Host => {
            hub.leave_host(&device, &outbox);
            // Host gitti: bekleyen istemciler zaman aşımına kadar asılmak
            // yerine hemen öğrenmeli.
            for client in hub.client_outboxes(&device) {
                let _ = client.try_send(control(RELAY_KIND, HOST_CLOSED));
            }
        }
        Role::Client => hub.leave_client(&device, &conn_id),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Host,
    Client,
}

impl Role {
    /// `role=host` dışındaki her şey istemci — referans uygulamanın kuralı.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("host") => Role::Host,
            _ => Role::Client,
        }
    }
}
