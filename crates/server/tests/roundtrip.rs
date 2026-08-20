//! Uçtan uca: gerçek sunucu, gerçek TCP soketi, gerçek `ChatClient`.
//!
//! Birim testleri sunucu mantığını sahte bir depoyla zaten kapsıyor ama
//! bunlar yeterli DEĞİL: durumsuz adaptör hatasında bütün birim testleri
//! geçiyordu, buna rağmen gerçek istemci cihaz kimliği BOŞ satırlar
//! üretiyordu — çünkü kimlik yalnızca `HELLO`'da geliyor ve sahte kurulum
//! `HELLO` göndermiyordu. Hatayı yakalayan tek şey gerçek istemciyi gerçek
//! bir sokete bağlamaktı.

mod common;

use std::sync::Arc;

use common::{eventually, start, ws_url, NoCheckpoint, Recorder};
use postillion_sync::ChatClient;

// ── testler ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn yazilan_satir_cihaz_kimligiyle_kaydediliyor() {
    let server = start().await;
    let client = ChatClient::connect(
        &ws_url(server.port, "c1"),
        Arc::new(Recorder::default()),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await
    .expect("bağlanmalı");

    client.enqueue_update(b"merhaba".to_vec());

    let rows = eventually(|| {
        let rows = server.store.rows.lock().unwrap();
        let chat = rows.get("c1")?;
        (!chat.is_empty()).then(|| chat.clone())
    })
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].payload, b"merhaba");
    // Durumsuz adaptörün kaçırdığı iddia: kimlik yalnızca `HELLO`'da geliyor
    // ve satıra kadar taşınmak zorunda.
    assert_eq!(rows[0].device, "cihaz-a", "cihaz kimliği satıra işlenmeli");

    client.shutdown().await;
}

#[tokio::test]
async fn iki_cihaz_birbirinin_satirini_goruyor() {
    let server = start().await;

    let sink_a = Arc::new(Recorder::default());
    let a = ChatClient::connect(
        &ws_url(server.port, "c2"),
        sink_a.clone(),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await
    .expect("a bağlanmalı");

    let sink_b = Arc::new(Recorder::default());
    let b = ChatClient::connect(
        &ws_url(server.port, "c2"),
        sink_b.clone(),
        Arc::new(NoCheckpoint),
        "cihaz-b",
        0,
    )
    .await
    .expect("b bağlanmalı");

    a.enqueue_update(b"a-dan".to_vec());

    // B, yoklama yapmadan görmeli: canlı yayın çalışmazsa bu test asılır.
    let seen = eventually(|| {
        let payloads = sink_b.payloads();
        (!payloads.is_empty()).then_some(payloads)
    })
    .await;
    assert_eq!(seen, vec![b"a-dan".to_vec()]);

    // A kendi satırını GERİ ALMAMALI: yerel doc'unda zaten uygulanmış
    // durumda ve tekrar uygulamak boşuna trafik.
    assert!(
        sink_a.payloads().is_empty(),
        "cihaz kendi satırını geri almamalı"
    );

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn gecmis_sonradan_baglanana_aktariliyor() {
    let server = start().await;

    let a = ChatClient::connect(
        &ws_url(server.port, "c3"),
        Arc::new(Recorder::default()),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await
    .expect("a bağlanmalı");
    a.enqueue_update(b"eski".to_vec());

    eventually(|| {
        let rows = server.store.rows.lock().unwrap();
        rows.get("c3").filter(|c| !c.is_empty()).map(|_| ())
    })
    .await;

    // B sonradan katılıyor ve imleci sıfır: geçmişin tamamını almalı.
    let sink_b = Arc::new(Recorder::default());
    let b = ChatClient::connect(
        &ws_url(server.port, "c3"),
        sink_b.clone(),
        Arc::new(NoCheckpoint),
        "cihaz-b",
        0,
    )
    .await
    .expect("b bağlanmalı");

    let seen = eventually(|| {
        let payloads = sink_b.payloads();
        (!payloads.is_empty()).then_some(payloads)
    })
    .await;
    assert_eq!(seen, vec![b"eski".to_vec()]);

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn jetonsuz_baglanti_reddediliyor() {
    let server = start().await;
    let url = format!("ws://127.0.0.1:{}/chat2/c4/ws", server.port);

    let result = ChatClient::connect(
        &url,
        Arc::new(Recorder::default()),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await;

    assert!(result.is_err(), "jetonsuz bağlantı kabul edilmemeli");
}

#[tokio::test]
async fn saglik_kontrolu_ayakta_olani_dogruluyor() {
    let server = start().await;
    // Docker bu yoklamaya bakıp konteyneri yeniden başlatıyor; yanlış
    // olumsuz sağlıklı bir sunucuyu sürekli döngüye sokardı.
    assert!(
        postillion_server::health::check(&format!("0.0.0.0:{}", server.port)).await,
        "ayakta olan sunucu sağlıklı görünmeli"
    );
}

#[tokio::test]
async fn saglik_kontrolu_kapali_olani_yakaliyor() {
    // Dinleyen kimse olmayan bir port. Bağlantı reddi yoklamanın hata
    // vermesini değil, `false` dönmesini sağlamalı.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    assert!(
        !postillion_server::health::check(&format!("0.0.0.0:{port}")).await,
        "kapalı sunucu sağlıklı görünmemeli"
    );
}
