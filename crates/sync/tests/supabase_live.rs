//! Gerçek bir Supabase projesine karşı uçtan uca doğrulama.
//!
//! `#[ignore]`: ağ ve gerçek kimlik bilgisi gerektiriyor. Bilgiler ORTAM
//! DEĞİŞKENİNDEN okunuyor, kodda durmuyor — anahtarların depoya girmemesi
//! için tek güvenilir yol bu.
//!
//! Çalıştırmak için:
//!
//! ```sh
//! export POSTILLION_SUPABASE_URL="https://<proje>.supabase.co"
//! export POSTILLION_SUPABASE_ANON_KEY="<anon anahtarı>"
//! export POSTILLION_SUPABASE_TOKEN="<oturum açmış kullanıcının erişim jetonu>"
//! cargo test -p postillion-sync --test supabase_live -- --ignored --nocapture
//! ```
//!
//! Jeton nereden: Supabase panelinde bir kullanıcı oluşturup
//! `POST /auth/v1/token?grant_type=password` ile alınabiliyor; ya da tarayıcıda
//! oturum açıp `localStorage`'daki `access_token`. Kısa ömürlü ve kapsamlı
//! olduğu için parolanızdan çok daha güvenli.
//!
//! ÖNCE `supabase/schema.sql` dosyasını projenizin SQL editöründe çalıştırın.

use postillion_sync::supabase::{ChatStore, rest::RestStore};

fn store() -> Option<RestStore> {
    let url = std::env::var("POSTILLION_SUPABASE_URL").ok()?;
    let key = std::env::var("POSTILLION_SUPABASE_ANON_KEY").ok()?;
    let token = std::env::var("POSTILLION_SUPABASE_TOKEN").ok()?;
    Some(RestStore::new(url, key, token))
}

/// Yazma, okuma, tekilleştirme ve süzgeç — şemanın tamamı tek turda.
#[tokio::test]
#[ignore]
async fn gercek_supabase_turu() {
    let Some(store) = store() else {
        eprintln!("ortam değişkenleri yok, atlanıyor (dosya başındaki nota bakın)");
        return;
    };

    // Her koşu kendi odasında: art arda çalıştırmak birikmiş satır bırakmasın.
    let chat = format!("test-{}", uuid::Uuid::new_v4());
    eprintln!("oda: {chat}");

    // Boş oda sıfır durum vermeli.
    let state = store.state(&chat).await.expect("durum okunmalı");
    assert_eq!(state.head_seq, 0, "yeni oda boş olmalı");
    assert_eq!(state.row_count, 0);

    // Yazma. Yük ikili ve tüm bayt aralığını kapsıyor: base64 gidiş dönüşü
    // burada bozulursa loro güncellemesi uygulanamaz.
    let payload: Vec<u8> = (0u8..=255).collect();
    let (seq, dup) = store
        .append(&chat, "dev-a".into(), "b1".into(), payload.clone())
        .await
        .expect("yazma başarılı olmalı");
    assert!(seq > 0, "sıra verilmeli");
    assert!(!dup, "ilk yazım tekrar olmamalı");

    // Aynı gönderim tekrar: YENİ satır açmamalı, aynı sırayı vermeli.
    let (again, dup) = store
        .append(&chat, "dev-a".into(), "b1".into(), payload.clone())
        .await
        .expect("tekrar yazım hata vermemeli");
    assert_eq!(again, seq, "tekrarda aynı sıra dönmeli");
    assert!(dup, "tekrar dup işaretlenmeli");

    // Okuma: yük bayt bayt aynı gelmeli.
    let rows = store.rows_after(&chat, 0, None).await.expect("okuma");
    assert_eq!(rows.len(), 1, "tekrar ikinci satır açmamalı");
    assert_eq!(rows[0].payload, payload, "yük bozulmuş");
    assert_eq!(rows[0].device, "dev-a");

    // İkinci cihazdan bir satır.
    store
        .append(&chat, "dev-b".into(), "b2".into(), vec![9, 9])
        .await
        .expect("ikinci yazma");

    // `excludeOwn`: kendi satırını almamalı.
    let rows = store
        .rows_after(&chat, 0, Some("dev-a".into()))
        .await
        .expect("süzgeçli okuma");
    assert_eq!(rows.len(), 1, "dev-a'nın satırı süzülmeli");
    assert_eq!(rows[0].device, "dev-b");

    // `after`: imleçten sonrası.
    let rows = store.rows_after(&chat, seq, None).await.expect("imleçli okuma");
    assert_eq!(rows.len(), 1, "imleçten sonra tek satır kalmalı");

    // Durum artık iki satırı görmeli.
    let state = store.state(&chat).await.expect("durum");
    assert_eq!(state.row_count, 2);
    assert!(state.head_seq >= seq + 1);

    eprintln!("tamam: {} satır, head={}", state.row_count, state.head_seq);
}

/// RLS gerçekten koruyor mu — jetonsuz hiçbir şey görünmemeli.
///
/// Bu, `anon` anahtarını uygulamaya gömmenin güvenli olmasının TEK dayanağı.
/// Politikalar yanlış yazılmışsa anahtar herkese herkesin verisini açar.
#[tokio::test]
#[ignore]
async fn jetonsuz_erisim_reddediliyor() {
    let (Ok(url), Ok(key)) = (
        std::env::var("POSTILLION_SUPABASE_URL"),
        std::env::var("POSTILLION_SUPABASE_ANON_KEY"),
    ) else {
        eprintln!("ortam değişkenleri yok, atlanıyor");
        return;
    };

    // Anahtar var, kullanıcı jetonu yok: `auth.uid()` boş kalıyor.
    let anonymous = RestStore::new(url, key, "");

    let chat = format!("test-{}", uuid::Uuid::new_v4());
    let result = anonymous
        .append(&chat, "dev".into(), "b".into(), vec![1])
        .await;

    // Yazabiliyorsa politikalar yanlış: `with check (owner = auth.uid())`
    // anonim bir çağrıda tutmuyor demektir.
    assert!(
        result.is_err(),
        "RLS AÇIK DEĞİL — anon anahtarıyla yazma başarılı oldu, bu anahtarı \
         dağıtmak veriyi herkese açar"
    );
    eprintln!("RLS çalışıyor: {}", result.unwrap_err());
}
