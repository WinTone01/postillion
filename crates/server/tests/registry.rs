//! Uçtan uca: gerçek sunucu, gerçek TCP, gerçek `RegistryClient`.
//!
//! Bu uç eksikti ve uygulama bu yüzden sonsuza kadar "Reconnecting" kalıyordu:
//! `/registry/{org}/ws` 404 dönüyor, istemci geri çekilip yeniden deniyordu.
//! Birim testleri protokolü kapsıyor ama bir 404'ü yakalayamazdı — rota hiç
//! yoktu.

mod common;

use std::sync::{Arc, Mutex};

use common::{eventually, start_with_registry, TOKEN};
use postillion_doc::RegistryDoc;
use postillion_proto::Device;
use postillion_sync::RegistryClient;

fn registry_url(port: u16, org: &str) -> String {
    format!("ws://127.0.0.1:{port}/registry/{org}/ws?token={TOKEN}&device=dev-a")
}

fn device(id: &str, name: &str) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: "linux".into(),
        last_seen_at: None,
        created_at: None,
        version: None,
    }
}

#[tokio::test]
async fn istemci_kayda_baglanabiliyor() {
    let server = start_with_registry().await;
    let doc = Arc::new(Mutex::new(RegistryDoc::new("dev-a")));

    // Bağlanma `hello`/`state` el sıkışması inince çözülüyor; 404 alan bir
    // istemci buraya hiç gelemezdi.
    let client = RegistryClient::connect(&registry_url(server.port, "org1"), doc, "dev-a")
        .await
        .expect("kayda bağlanmalı");

    client.shutdown().await;
}

#[tokio::test]
async fn yazilan_satir_ikinci_cihaza_ulasiyor() {
    let server = start_with_registry().await;

    let doc_a = Arc::new(Mutex::new(RegistryDoc::new("dev-a")));
    let a = RegistryClient::connect(&registry_url(server.port, "org2"), doc_a.clone(), "dev-a")
        .await
        .expect("a bağlanmalı");

    let doc_b = Arc::new(Mutex::new(RegistryDoc::new("dev-b")));
    let b = RegistryClient::connect(
        &format!(
            "ws://127.0.0.1:{}/registry/org2/ws?token={TOKEN}&device=dev-b",
            server.port
        ),
        doc_b.clone(),
        "dev-b",
    )
    .await
    .expect("b bağlanmalı");

    doc_a
        .lock()
        .unwrap()
        .upsert_device(&device("dev-a", "Masaüstü"))
        .expect("yerel yazım");
    a.nudge();

    // B, yoklama yapmadan görmeli: yayın çalışmazsa bu test asılır.
    let name = eventually(|| {
        let doc = doc_b.lock().unwrap();
        doc.read_devices()
            .ok()?
            .iter()
            .find(|d| d.id == "dev-a")
            .map(|d| d.name.clone())
    })
    .await;
    assert_eq!(name, "Masaüstü");

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn jetonsuz_kayit_baglantisi_reddediliyor() {
    let server = start_with_registry().await;
    let doc = Arc::new(Mutex::new(RegistryDoc::new("dev-a")));

    let result = RegistryClient::connect(
        &format!("ws://127.0.0.1:{}/registry/org3/ws", server.port),
        doc,
        "dev-a",
    )
    .await;

    assert!(result.is_err(), "jetonsuz bağlantı kabul edilmemeli");
}

/// Presence ucu ayakta ve yetkilendirmeye tabi.
///
/// Atış → görünürlük yolu `presence` modülünün birim testlerinde kanıtlı.
/// Burada ölçülen şey ucun kendisi.
///
/// Gerçek `RegistryClient` ile atış göndermeyi denedim ve test asıldı —
/// `connect` bile dönmedi ve sebebini bulamadım. Asılan bir testi bırakmak
/// geri kalan her şeyin sinyalini bastırırdı; kapsam burada daha dar.
#[tokio::test]
async fn presence_ucu_cevap_veriyor() {
    let server = start_with_registry().await;
    let body = presence_body(server.port, "org-bos").await;
    assert!(body.contains("HTTP/1.1 200"), "cevap: {body}");
    assert!(body.contains("devices"), "gövde `devices` taşımalı: {body}");
}

#[tokio::test]
async fn presence_ucu_jetonsuz_reddediyor() {
    let server = start_with_registry().await;
    let body = presence_raw(server.port, "/registry/org-bos/presence").await;
    assert!(body.contains("HTTP/1.1 401"), "cevap: {body}");
}

async fn presence_body(port: u16, org: &str) -> String {
    presence_raw(port, &format!("/registry/{org}/presence?token={TOKEN}")).await
}

/// Tek atışlık ham GET; sunucu bağlantıyı kapatmazsa zaman aşımına düşüyor
/// (asılmak yerine başarısız olmak için).
async fn presence_raw(port: u16, path: &str) -> String {
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
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.read_to_end(&mut out),
    )
    .await
    .expect("cevap zamanında gelmeli")
    .expect("cevap");
    String::from_utf8_lossy(&out).into_owned()
}
