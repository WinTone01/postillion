//! Cihaz rölesi: host ile istemci arasında çerçeve yönlendirme.
//!
//! Bu uç eksikti ve günlükte `device room unreachable: HTTP error: 404` olarak
//! görünüyordu — uzaktan terminal ve hedef cihaza mesaj bu yüzden hiç
//! çalışmıyordu.
//!
//! Testler çerçeveleri ham WebSocket üzerinden gönderiyor: amaç RPC katmanını
//! değil RÖLENİN kendisini sınamak — kimin ne aldığı ve yönlendirme
//! anahtarlarının doğru işlenip işlenmediği.

mod common;

use common::{start, TOKEN};
use futures::{SinkExt, StreamExt};
use postillion_rpc::device_room::{
    decode_device_frame, encode_device_frame, DeviceFrameHeader, HOST_OFFLINE, RELAY_KIND,
};
use tokio_tungstenite::tungstenite::Message;

type Socket = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn dial(port: u16, device: &str, role: &str, conn: Option<&str>) -> Socket {
    let conn = conn.map(|c| format!("&connId={c}")).unwrap_or_default();
    let url =
        format!("ws://127.0.0.1:{port}/device/{device}/ws?role={role}{conn}&token={TOKEN}");
    let (socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("röleye bağlanmalı");
    socket
}

async fn send(socket: &mut Socket, header: &DeviceFrameHeader, payload: &[u8]) {
    let frame = encode_device_frame(header, payload).expect("çerçeve kodlanmalı");
    socket.send(Message::Binary(frame.into())).await.unwrap();
}

/// Bir sonraki ikili çerçeveyi çözer; süre dolarsa testi düşürür.
async fn recv(socket: &mut Socket) -> (DeviceFrameHeader, Vec<u8>) {
    let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("çerçeve zamanında gelmeli")
        .expect("soket açık")
        .expect("okunabilir");
    match message {
        Message::Binary(bytes) => decode_device_frame(&bytes).expect("çerçeve çözülmeli"),
        other => panic!("ikili çerçeve bekleniyordu, gelen: {other:?}"),
    }
}

#[tokio::test]
async fn istemci_cercevesi_hosta_kaynak_damgasiyla_ulasiyor() {
    let server = start().await;
    let mut host = dial(server.port, "dev-1", "host", None).await;
    let mut client = dial(server.port, "dev-1", "client", Some("c1")).await;

    send(
        &mut client,
        &DeviceFrameHeader::new("s1", "rpc"),
        b"merhaba",
    )
    .await;

    let (header, payload) = recv(&mut host).await;
    assert_eq!(payload, b"merhaba");
    assert_eq!(header.s, "s1");
    // Kaynak damgası SUNUCUDA basılıyor: istemcinin kendi söylediğine
    // güvenmek, bir istemcinin başkasının kimliğiyle konuşabilmesi olurdu.
    assert_eq!(header.from.as_deref(), Some("c1"));
    assert_eq!(header.to, None);
}

#[tokio::test]
async fn host_cevabi_dogru_istemciye_gidiyor() {
    let server = start().await;
    let mut host = dial(server.port, "dev-2", "host", None).await;
    let mut c1 = dial(server.port, "dev-2", "client", Some("c1")).await;
    let mut c2 = dial(server.port, "dev-2", "client", Some("c2")).await;

    send(&mut c1, &DeviceFrameHeader::new("s1", "rpc"), b"soru").await;
    let _ = recv(&mut host).await;

    let mut reply = DeviceFrameHeader::new("s1", "rpc");
    reply.to = Some("c1".into());
    send(&mut host, &reply, b"cevap").await;

    let (header, payload) = recv(&mut c1).await;
    assert_eq!(payload, b"cevap");
    // Yönlendirme anahtarları sökülüyor: `to` odanın iç meselesi.
    assert_eq!(header.to, None);
    assert_eq!(header.from, None);

    // c2 bu cevabı GÖRMEMELİ; röle yayın değil hedefli teslimat yapıyor.
    let leaked = tokio::time::timeout(std::time::Duration::from_millis(400), c2.next()).await;
    assert!(leaked.is_err(), "cevap yanlış istemciye sızdı");
}

#[tokio::test]
async fn hostsuz_odada_istemci_hemen_ogreniyor() {
    let server = start().await;
    let mut client = dial(server.port, "dev-3", "client", Some("c1")).await;

    send(&mut client, &DeviceFrameHeader::new("s9", "rpc"), b"x").await;

    // Zaman aşımını beklemek yerine anında cevap: asılı kalan bir istemci
    // kullanıcının gözünde donmuş bir arayüz demek.
    let (header, payload) = recv(&mut client).await;
    assert_eq!(header.k, RELAY_KIND);
    assert_eq!(header.s, "s9", "hata, sorulan akışa dönmeli");
    let body: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(body["error"], HOST_OFFLINE);
}

#[tokio::test]
async fn odalar_birbirine_sizmiyor() {
    let server = start().await;
    let mut host_a = dial(server.port, "dev-a", "host", None).await;
    let mut client_b = dial(server.port, "dev-b", "client", Some("c1")).await;

    send(&mut client_b, &DeviceFrameHeader::new("s1", "rpc"), b"b").await;

    // dev-b'nin host'u yok; çerçeve dev-a'nın host'una GİTMEMELİ.
    let leaked = tokio::time::timeout(std::time::Duration::from_millis(400), host_a.next()).await;
    assert!(leaked.is_err(), "çerçeve başka cihazın odasına sızdı");

    // Ve dev-b'deki istemci host_offline almalı.
    let (header, _) = recv(&mut client_b).await;
    assert_eq!(header.k, RELAY_KIND);
}

#[tokio::test]
async fn jetonsuz_role_baglantisi_reddediliyor() {
    let server = start().await;
    let url = format!("ws://127.0.0.1:{}/device/dev-x/ws?role=host", server.port);
    assert!(
        tokio_tungstenite::connect_async(url).await.is_err(),
        "jetonsuz bağlantı kabul edilmemeli"
    );
}
