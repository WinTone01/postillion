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

#[tokio::test]
async fn kok_yol_sunucuyu_tanitiyor() {
    // Adresi tarayıcıya yapıştıran biri çıplak bir 404 görmemeli: ilk
    // kurulumda bu, çalışan bir sunucunun bozuk sanılmasına yol açtı.
    let server = start().await;
    let body = reqwest_get(server.port, "/").await;
    assert!(body.starts_with("HTTP/1.1 200"), "kök yol cevabı: {body}");
    assert!(body.contains("postillion-server"), "kimlik yok: {body}");
    // Sürüm duyurulmamalı — saldırganın işini kolaylaştırmaktan başka bir
    // işe yaramıyor.
    assert!(
        !body.contains(env!("CARGO_PKG_VERSION")),
        "sürüm sızdırılmamalı: {body}"
    );
}

/// Tek atışlık ham HTTP isteği — sunucuya bir HTTP istemcisi bağımlılığı
/// eklemeden durum satırını ve gövdeyi okumak için.
async fn reqwest_get(port: u16, path: &str) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("bağlanmalı");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("istek yazılmalı");
    let mut out = String::new();
    stream
        .read_to_string(&mut out)
        .await
        .expect("cevap okunmalı");
    out
}

#[tokio::test]
async fn transkript_ucu_materyalize_mesaj_donduruyor() {
    // Panelin bilgisayar kapalıyken içeriği göstermesi buna bağlı: satırlar
    // sunucuda birleştirilip mesaj olarak sunuluyor.
    let server = start().await;

    // Bir istemcinin yaptığını yapıyoruz: doc'a yaz, güncellemeyi satır olarak
    // sunucuya koy.
    let doc = postillion_doc::SessionDoc::init("c-msg").expect("doc");
    doc.push_message(&postillion_doc::SessionMessageEntry {
        id: "m1".into(),
        role: postillion_doc::MessageRole::User,
        parts: vec![postillion_doc::MessagePart::Text {
            id: "m1-p0".into(),
            text: "webden görünmeli".into(),
        }],
        created_at: 1_700_000_000_000,
        device_id: "dev-a".into(),
        status: None,
        continuation_of: None,
    })
    .expect("yazım");
    let payload = doc
        .doc()
        .export(loro::ExportMode::Snapshot)
        .expect("dışa aktarım");

    postillion_sync::room::ChatStore::append(
        &*server.store,
        "c-msg",
        "dev-a".into(),
        "b1".into(),
        payload,
    )
    .await
    .expect("satır yazılmalı");

    let body = http_get(server.port, "/chat2/c-msg/messages?token=test-jetonu").await;
    assert!(body.contains("HTTP/1.1 200"), "cevap: {body}");
    assert!(
        body.contains("webden görünmeli"),
        "transkript mesajı taşımalı: {body}"
    );
    // Baş sıra dönmeli: panel canlı yoklamada bunu geri veriyor.
    assert!(body.contains("headSeq"), "baş sıra dönmeli: {body}");
}

/// Değişmemiş bir sohbet için belge YENİDEN KURULMAMALI.
///
/// Panel canlı akış için düzenli yokluyor; her yoklamada bütün satırları
/// birleştirmek uzun bir sohbette hiç değişmemiş bir belgeyi saniyede bir
/// yeniden kurmak olurdu.
#[tokio::test]
async fn degismemis_transkript_yeniden_kurulmuyor() {
    let server = start().await;

    let doc = postillion_doc::SessionDoc::init("c-since").expect("doc");
    doc.push_message(&postillion_doc::SessionMessageEntry {
        id: "m1".into(),
        role: postillion_doc::MessageRole::User,
        parts: vec![postillion_doc::MessagePart::Text {
            id: "m1-p0".into(),
            text: "ilk".into(),
        }],
        created_at: 1_700_000_000_000,
        device_id: "dev-a".into(),
        status: None,
        continuation_of: None,
    })
    .expect("yazım");

    postillion_sync::room::ChatStore::append(
        &*server.store,
        "c-since",
        "dev-a".into(),
        "b1".into(),
        doc.doc().export(loro::ExportMode::Snapshot).expect("dışa aktarım"),
    )
    .await
    .expect("satır");

    // Önce baş sırayı öğreniyoruz.
    let first = http_get(server.port, "/chat2/c-since/messages?token=test-jetonu").await;
    assert!(first.contains("\"headSeq\":1"), "baş sıra 1 olmalı: {first}");

    // Aynı sırayla tekrar sorunca mesajlar GELMEMELİ.
    let again = http_get(
        server.port,
        "/chat2/c-since/messages?token=test-jetonu&since=1",
    )
    .await;
    assert!(again.contains("unchanged"), "değişmedi denmeli: {again}");
    assert!(!again.contains("ilk"), "mesaj yeniden gönderilmemeli: {again}");

    // Eski bir sırayla sorunca tam transkript gelmeli.
    let stale = http_get(
        server.port,
        "/chat2/c-since/messages?token=test-jetonu&since=0",
    )
    .await;
    assert!(stale.contains("ilk"), "eski imleç tam transkript almalı: {stale}");
}

/// Tek atışlık ham HTTP GET.
async fn http_get(port: u16, path: &str) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("bağlanmalı");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("istek");
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.expect("cevap");
    String::from_utf8_lossy(&out).into_owned()
}
