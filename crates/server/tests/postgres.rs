//! `db.rs`'in gerçek Postgres'e karşı sınanması.
//!
//! Bellek içi depo protokol mantığını kanıtlıyor ama SQL'i kanıtlamıyor:
//! tekilleştirme indeksi, `on conflict` davranışı ve `bytea` gidiş dönüşü
//! ancak gerçek veritabanında ortaya çıkıyor.
//!
//! `POSTILLION_TEST_DATABASE_URL` yoksa testler ATLANIYOR — veritabanı
//! olmayan bir makinede kırmızı yanmak, gerçek bir hatayı gürültüye boğardı.
//!
//! ```sh
//! docker run -d --rm --name pg -e POSTGRES_PASSWORD=test -e POSTGRES_DB=postillion \
//!   -p 55432:5432 postgres:17-alpine
//! POSTILLION_TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:55432/postillion \
//!   cargo test -p postillion-server --test postgres
//! ```

mod common;

use std::sync::Arc;

use common::{eventually, start_with, ws_url, NoCheckpoint, Recorder};
use postillion_server::db::{self, PgStore};
use postillion_sync::room::ChatStore;
use postillion_sync::ChatClient;
use uuid::Uuid;

/// Depo ve her testin kendine ait sohbet kimliği.
///
/// Testler aynı veritabanını paylaşıyor; sabit bir kimlik onları birbirine
/// bağlar ve paralel koşuda sızdırırdı.
async fn store() -> Option<(PgStore, String)> {
    let url = std::env::var("POSTILLION_TEST_DATABASE_URL").ok()?;
    let pool = db::connect(&url).await.expect("veritabanına bağlanmalı");
    Some((PgStore::new(pool), Uuid::new_v4().to_string()))
}

macro_rules! store_or_skip {
    () => {
        match store().await {
            Some(pair) => pair,
            None => {
                eprintln!("POSTILLION_TEST_DATABASE_URL yok; atlanıyor");
                return;
            }
        }
    };
}

#[tokio::test]
async fn bos_oda_sifir_durum_veriyor() {
    let (store, chat) = store_or_skip!();
    let state = store.state(&chat).await.expect("durum");
    assert_eq!(state.head_seq, 0);
    assert_eq!(state.row_count, 0);
    assert_eq!(state.row_bytes, 0);
    assert_eq!(state.checkpoint_seq, 0);
}

#[tokio::test]
async fn satir_yazilip_geri_okunuyor() {
    let (store, chat) = store_or_skip!();

    // `bytea` gidiş dönüşü: metin olarak kaçırılan bir bayt dizisi burada
    // bozulurdu ve loro güncellemesi ikili.
    let payload = vec![0u8, 255, 1, 128, 0];
    let (seq, dup) = store
        .append(&chat, "cihaz-a".into(), "b1".into(), payload.clone())
        .await
        .expect("yazılmalı");
    assert!(!dup);

    let rows = store.rows_after(&chat, 0, None).await.expect("okunmalı");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].seq, seq);
    assert_eq!(rows[0].device, "cihaz-a");
    assert_eq!(rows[0].batch_id, "b1");
    assert_eq!(rows[0].payload, payload);

    let state = store.state(&chat).await.expect("durum");
    assert_eq!(state.head_seq, seq);
    assert_eq!(state.row_count, 1);
    assert_eq!(state.row_bytes, payload.len() as u64);
}

#[tokio::test]
async fn ayni_gonderim_iki_kez_yazilmiyor() {
    let (store, chat) = store_or_skip!();

    let (first, dup1) = store
        .append(&chat, "cihaz-a".into(), "tekrar".into(), b"bir".to_vec())
        .await
        .expect("ilk yazım");
    let (second, dup2) = store
        .append(&chat, "cihaz-a".into(), "tekrar".into(), b"bir".to_vec())
        .await
        .expect("ikinci yazım");

    assert!(!dup1);
    assert!(dup2, "ikinci gönderim tekrar olarak işaretlenmeli");
    // AYNI sıra dönmeli: yeni bir sıra, istemcinin imlecini almadığı
    // satırların ötesine taşır ve aradakiler sessizce kaybolurdu.
    assert_eq!(first, second, "tekrarlanan gönderim aynı sırayı almalı");

    assert_eq!(store.state(&chat).await.expect("durum").row_count, 1);
}

#[tokio::test]
async fn satirlar_sirali_ve_imlecten_sonra_geliyor() {
    let (store, chat) = store_or_skip!();

    let mut seqs = Vec::new();
    for i in 0..5u8 {
        let (seq, _) = store
            .append(&chat, "cihaz-a".into(), format!("b{i}"), vec![i])
            .await
            .expect("yazım");
        seqs.push(seq);
    }
    // Sıra kimliği GLOBAL artıyor; şart olan sohbet içinde artan olması.
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "sıralar artmalı");

    let after = store
        .rows_after(&chat, seqs[2], None)
        .await
        .expect("okuma");
    assert_eq!(after.len(), 2, "imleçten sonrakiler gelmeli");
    assert_eq!(after[0].payload, vec![3]);
    assert_eq!(after[1].payload, vec![4]);
}

#[tokio::test]
async fn kendi_satirlari_suzulebiliyor() {
    let (store, chat) = store_or_skip!();

    store
        .append(&chat, "cihaz-a".into(), "a1".into(), b"a".to_vec())
        .await
        .expect("yazım");
    store
        .append(&chat, "cihaz-b".into(), "b1".into(), b"b".to_vec())
        .await
        .expect("yazım");

    let rows = store
        .rows_after(&chat, 0, Some("cihaz-a".into()))
        .await
        .expect("okuma");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].device, "cihaz-b");
}

#[tokio::test]
async fn sohbetler_birbirine_karismiyor() {
    let (store, chat) = store_or_skip!();
    let other = Uuid::new_v4().to_string();

    store
        .append(&chat, "cihaz-a".into(), "ortak".into(), b"burada".to_vec())
        .await
        .expect("yazım");
    // AYNI batch kimliği başka sohbette engellenmemeli: tekilleştirme
    // sohbet başına, evrensel değil.
    let (_, dup) = store
        .append(&other, "cihaz-a".into(), "ortak".into(), b"orada".to_vec())
        .await
        .expect("yazım");
    assert!(!dup, "başka sohbetteki aynı batch tekrar sayılmamalı");

    let rows = store.rows_after(&chat, 0, None).await.expect("okuma");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].payload, b"burada");
}

// ── uçtan uca: gerçek sunucu + gerçek Postgres + gerçek istemciler ─────────

#[tokio::test]
async fn iki_cihaz_postgres_uzerinden_konusuyor() {
    let (store, chat) = store_or_skip!();
    // En güçlü sınama bu: parçaların hepsi gerçek. Bellek içi depo protokolü,
    // Postgres testleri SQL'i ayrı ayrı kanıtlıyor; ikisinin arasındaki
    // bağlantı ancak burada görünüyor.
    let port = start_with(Arc::new(store)).await;

    let a = ChatClient::connect(
        &ws_url(port, &chat),
        Arc::new(Recorder::default()),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await
    .expect("a bağlanmalı");

    let sink_b = Arc::new(Recorder::default());
    let b = ChatClient::connect(
        &ws_url(port, &chat),
        sink_b.clone(),
        Arc::new(NoCheckpoint),
        "cihaz-b",
        0,
    )
    .await
    .expect("b bağlanmalı");

    a.enqueue_update(b"postgres uzerinden".to_vec());

    let seen = eventually(|| {
        let payloads = sink_b.payloads();
        (!payloads.is_empty()).then_some(payloads)
    })
    .await;
    assert_eq!(seen, vec![b"postgres uzerinden".to_vec()]);

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test]
async fn sonradan_baglanan_gecmisi_tam_ve_tekrarsiz_aliyor() {
    let (store, chat) = store_or_skip!();
    let store = Arc::new(store);
    let port = start_with(store.clone()).await;

    let a = ChatClient::connect(
        &ws_url(port, &chat),
        Arc::new(Recorder::default()),
        Arc::new(NoCheckpoint),
        "cihaz-a",
        0,
    )
    .await
    .expect("a bağlanmalı");
    a.enqueue_update(b"bir".to_vec());
    a.enqueue_update(b"iki".to_vec());

    // Depoyu doğrudan `await` ile yokluyoruz: `eventually`'nin eşzamanlı
    // kapanışı içinde `block_on` çağırmak tokio iş parçacığını bloke ediyor
    // ve tek iş parçacıklı çalışma zamanında testi tamamen kilitliyor.
    for _ in 0..200 {
        let rows = store.rows_after(&chat, 0, None).await.expect("okuma");
        if rows.len() == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // B sonradan katılıyor. İmleçle katılsa BİLE tüm geçmişi çekiyor:
    // istemcide süreç başına bir kez çalışan "cursor amnesty" var — anlık
    // görüntüsü olmayan bir odada imleci sıfıra çekip günlüğü yeniden
    // okuyor, çünkü imlecin üstünde park kalıp düşmüş içe aktarımlar aksi
    // halde bir daha hiç okunmuyor. Yani buradaki asıl iddia "az satır"
    // değil, **TAM ve TEKRARSIZ**.
    let sink_b = Arc::new(Recorder::default());
    let b = ChatClient::connect(
        &ws_url(port, &chat),
        sink_b.clone(),
        Arc::new(NoCheckpoint),
        "cihaz-b",
        0,
    )
    .await
    .expect("b bağlanmalı");

    let seen = eventually(|| {
        let payloads = sink_b.payloads();
        (payloads.len() == 2).then_some(payloads)
    })
    .await;
    assert_eq!(seen, vec![b"bir".to_vec(), b"iki".to_vec()]);

    // Ve tekrar gelmiyor. Sıraları sohbet başına bitişik tutmayan bir şema
    // burada patlıyordu: istemcinin imleci yalnızca satır bitişikse
    // ilerliyor, boşluk gören istemci her turda geçmişi yeniden çekip aynı
    // satırları tekrar uyguluyordu.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(
        sink_b.payloads().len(),
        2,
        "satırlar yeniden gönderilmemeli"
    );

    a.shutdown().await;
    b.shutdown().await;
}
