//! Üç WebSocket ucunun da metin `"ping"` yoklamasına karşılık vermesi.
//!
//! Bu eksikti ve uygulama kalıcı olarak "reconnecting" gösteriyordu. Arıza
//! hiçbir yerde HATA gibi görünmüyor: soket kuruluyor, kimlik doğrulanıyor,
//! çerçeveler gidiyor — ama sunucudan hiçbir şey GELMEDİĞİ için istemcinin
//! sessizlik kirası (25 sn) doluyor ve sağlıklı soketi ölü sayıp kapatıyor.
//! Günlükte yalnızca "host socket silent past lease; reconnecting" var.
//!
//! Sözleşme TS uçtan geliyor: orada Durable Object'in
//! `setWebSocketAutoResponse` çifti cevaplıyor. Kendi sunucumuzda böyle bir
//! mekanizma yok, karşılığı elle yazmak zorundayız — ve hiçbir test bunu
//! istemediği için üç uçta birden atlanmış.

mod common;

use common::{start, ws_url, TOKEN};
use futures::{SinkExt, StreamExt};
use postillion_sync::keepalive;
use tokio_tungstenite::tungstenite::Message;

/// Ucu açar, `"ping"` gönderir ve `"pong"` bekler.
async fn expects_pong(url: String, uc: &str) {
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .unwrap_or_else(|e| panic!("{uc}: bağlanmalı: {e}"));

    socket
        .send(Message::Text(keepalive::PING.into()))
        .await
        .unwrap_or_else(|e| panic!("{uc}: yoklama gönderilmeli: {e}"));

    // Kısa süre sınırı: cevap gelmediğinde test asılı kalmamalı, düşmeli.
    let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .unwrap_or_else(|_| {
            panic!("{uc}: karşılık gelmedi — istemci bu soketi ölü sayıp yeniden bağlanır")
        })
        .unwrap_or_else(|| panic!("{uc}: soket kapandı"))
        .unwrap_or_else(|e| panic!("{uc}: okunabilir olmalı: {e}"));

    match message {
        Message::Text(text) => assert_eq!(
            text.as_str(),
            keepalive::PONG,
            "{uc}: karşılık tam olarak \"{}\" olmalı",
            keepalive::PONG
        ),
        other => panic!("{uc}: metin karşılık bekleniyordu, gelen: {other:?}"),
    }
}

#[tokio::test]
async fn sohbet_odasi_yoklamaya_karsilik_veriyor() {
    let server = start().await;
    expects_pong(ws_url(server.port, "sohbet-yoklama"), "chat2").await;
}

#[tokio::test]
async fn kayit_odasi_yoklamaya_karsilik_veriyor() {
    let server = start().await;
    let url = format!(
        "ws://127.0.0.1:{}/registry/org-yoklama/ws?token={TOKEN}&device=dev-a",
        server.port
    );
    expects_pong(url, "registry").await;
}

#[tokio::test]
async fn cihaz_rolesi_yoklamaya_karsilik_veriyor() {
    let server = start().await;
    let url = format!(
        "ws://127.0.0.1:{}/device/cihaz-yoklama/ws?role=host&token={TOKEN}",
        server.port
    );
    expects_pong(url, "device relay").await;
}
