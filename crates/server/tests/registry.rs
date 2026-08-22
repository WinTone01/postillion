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
