//! [`ChatStore`]'un PostgREST implementasyonu.
//!
//! Supabase'in REST arayüzü Postgres fonksiyonlarını `/rest/v1/rpc/{ad}`
//! altında yayınlıyor. Tabloya doğrudan gitmek yerine fonksiyon çağırıyoruz;
//! gerekçe `supabase/schema.sql` içinde yazıyor — özetle `bytea` JSON'da
//! taşınamıyor ve tekilleştirmenin atomik olması gerekiyor.
//!
//! Bu katman kasıtlı olarak ince: karar vermiyor, yalnızca çağırıyor ve
//! çeviriyor. Protokol mantığının tamamı [`super`] içinde ve depo trait'i
//! üzerinden ağsız sınanıyor.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::future::BoxFuture;
use serde::Deserialize;

use super::{ChatStore, Row, RoomState};
use crate::types::SyncError;

/// Bir Supabase projesine bağlı depo.
#[derive(Clone)]
pub struct RestStore {
    http: reqwest::Client,
    /// `https://<proje>.supabase.co`
    base_url: String,
    /// Projenin `anon` anahtarı — PostgREST'in `apikey` başlığı.
    api_key: String,
    /// Oturum açmış kullanıcının erişim jetonu. RLS politikaları `auth.uid()`
    /// üzerinden çalıştığı için bu OLMADAN hiçbir satır görünmüyor; anahtar
    /// tek başına yetmiyor ve yetmemeli.
    access_token: String,
}

impl RestStore {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            access_token: access_token.into(),
        }
    }

    /// Test ve özel yapılandırma için hazır bir istemciyle kurar.
    pub fn with_client(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            access_token: access_token.into(),
        }
    }

    fn rpc_url(&self, name: &str) -> String {
        format!("{}/rest/v1/rpc/{name}", self.base_url)
    }

    /// Bir fonksiyonu çağırıp cevabı çözer.
    async fn rpc<T: for<'de> Deserialize<'de>>(
        &self,
        name: &str,
        body: serde_json::Value,
    ) -> Result<T, SyncError> {
        let response = self
            .http
            .post(self.rpc_url(name))
            .header("apikey", &self.api_key)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::Http(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| SyncError::Http(e.to_string()))?;

        if !status.is_success() {
            // Postgres'in kendi mesajı bizimkinden yararlı: hangi politika
            // reddetti, hangi kısıt çakıştı orada yazıyor.
            return Err(SyncError::Http(format!(
                "{name}: HTTP {} — {}",
                status.as_u16(),
                text.trim()
            )));
        }

        serde_json::from_str(&text)
            .map_err(|e| SyncError::Protocol(format!("{name}: cevap çözülemedi: {e}")))
    }
}

/// `chat_state` bir satır döndürüyor; PostgREST tablo fonksiyonlarını daima
/// dizi olarak sarıyor.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct StateRow {
    head_seq: i64,
    seq_floor: i64,
    checkpoint_seq: i64,
    checkpoint_size: i64,
    row_count: i64,
    row_bytes: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct RowRow {
    seq: i64,
    device: String,
    batch_id: String,
    /// base64 — `bytea` JSON'da taşınamıyor.
    payload: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct AppendRow {
    seq: i64,
    dup: bool,
}

/// Negatif ya da taşan değerleri sıfıra çekiyor.
///
/// Postgres tarafı `bigint` (işaretli) veriyor, protokol `u64` istiyor.
/// Negatif bir sayı buraya ancak veri bozulmuşsa gelir; panik yerine sıfır
/// vermek istemciyi baştan yakalamaya düşürüyor, çökertmiyor.
fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl ChatStore for RestStore {
    fn state(&self, chat_id: &str) -> BoxFuture<'static, Result<RoomState, SyncError>> {
        let this = self.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            let rows: Vec<StateRow> = this
                .rpc("chat_state", serde_json::json!({ "p_chat_id": chat_id }))
                .await?;
            // Fonksiyon daima tek satır veriyor; boş dönerse oda hiç
            // yazılmamış demektir ve sıfır durum doğru cevap.
            let Some(row) = rows.into_iter().next() else {
                return Ok(RoomState::default());
            };
            Ok(RoomState {
                head_seq: to_u64(row.head_seq),
                seq_floor: to_u64(row.seq_floor),
                checkpoint_seq: to_u64(row.checkpoint_seq),
                checkpoint_size: to_u64(row.checkpoint_size),
                row_count: to_u64(row.row_count),
                row_bytes: to_u64(row.row_bytes),
            })
        })
    }

    fn rows_after(
        &self,
        chat_id: &str,
        after: u64,
        exclude_device: Option<String>,
    ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>> {
        let this = self.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            let rows: Vec<RowRow> = this
                .rpc(
                    "chat_rows_after",
                    serde_json::json!({
                        "p_chat_id": chat_id,
                        "p_after": after,
                        "p_exclude_device": exclude_device,
                    }),
                )
                .await?;

            rows.into_iter()
                .map(|row| {
                    let payload = BASE64
                        .decode(row.payload.as_bytes())
                        // Çözülemeyen bir yük sessizce atlanmamalı: eksik bir
                        // güncelleme sohbeti sessizce bozar, hata görünür olur.
                        .map_err(|e| {
                            SyncError::Protocol(format!("satır {}: base64 çözülemedi: {e}", row.seq))
                        })?;
                    Ok(Row {
                        seq: to_u64(row.seq),
                        device: row.device,
                        batch_id: row.batch_id,
                        payload,
                    })
                })
                .collect()
        })
    }

    fn append(
        &self,
        chat_id: &str,
        device: String,
        batch_id: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<(u64, bool), SyncError>> {
        let this = self.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            let rows: Vec<AppendRow> = this
                .rpc(
                    "chat_append",
                    serde_json::json!({
                        "p_chat_id": chat_id,
                        "p_device": device,
                        "p_batch_id": batch_id,
                        "p_payload": BASE64.encode(&payload),
                    }),
                )
                .await?;

            let Some(row) = rows.into_iter().next() else {
                return Err(SyncError::Protocol("chat_append boş döndü".into()));
            };
            Ok((to_u64(row.seq), row.dup))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_yolu_taban_adresten_kuruluyor() {
        let store = RestStore::new("https://abc.supabase.co/", "anon", "jwt");
        // Sondaki eğik çizgi kırpılmalı, yoksa çift eğik çizgi çıkar.
        assert_eq!(
            store.rpc_url("chat_state"),
            "https://abc.supabase.co/rest/v1/rpc/chat_state"
        );
    }

    #[test]
    fn isaretli_tamsayi_guvenle_ceviriliyor() {
        assert_eq!(to_u64(0), 0);
        assert_eq!(to_u64(42), 42);
        // Bozuk veri panikletmemeli; sıfır istemciyi baştan yakalamaya düşürür.
        assert_eq!(to_u64(-1), 0);
        assert_eq!(to_u64(i64::MIN), 0);
        assert_eq!(to_u64(i64::MAX), i64::MAX as u64);
    }

    // ── sahte PostgREST ────────────────────────────────────────────────────
    //
    // Kütüphane eklemek yerine ham TCP: doğrulanan şey HTTP'nin kendisi değil,
    // BİZİM ürettiğimiz istek — yol, başlıklar, gövde — ve cevabın çözülmesi.
    // Bunlar Supabase ile aramızdaki sözleşme ve sessizce kayabilecek tek yer.

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    struct Captured {
        path: String,
        headers: String,
        body: serde_json::Value,
    }

    /// Tek istek karşılayıp verilen gövdeyi döndüren sunucu.
    async fn fake_postgrest(
        response_body: &'static str,
        status_line: &'static str,
    ) -> (String, tokio::task::JoinHandle<Captured>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            // Başlık sonuna kadar oku, sonra Content-Length kadar gövde.
            let mut raw = Vec::new();
            let mut buf = [0u8; 1024];
            let head_end = loop {
                let n = socket.read(&mut buf).await.unwrap();
                raw.extend_from_slice(&buf[..n]);
                if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
                if n == 0 {
                    panic!("istek yarıda kesildi");
                }
            };
            let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
            let len: usize = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .or_else(|| line.strip_prefix("Content-Length: "))
                })
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);

            while raw.len() < head_end + len {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
            }

            let body: serde_json::Value =
                serde_json::from_slice(&raw[head_end..head_end + len]).unwrap();
            let path = head.lines().next().unwrap().to_string();

            let reply = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();

            Captured {
                path,
                headers: head,
                body,
            }
        });

        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn append_istegi_dogru_sekilde_gidiyor() {
        let (url, server) = fake_postgrest(r#"[{"seq":7,"dup":false}]"#, "HTTP/1.1 200 OK").await;
        let store = RestStore::new(url, "anon-anahtar", "kullanici-jetonu");

        let (seq, dup) = store
            .append("c1", "dev-a".into(), "b1".into(), vec![1, 2, 3])
            .await
            .unwrap();
        assert_eq!(seq, 7);
        assert!(!dup);

        let captured = server.await.unwrap();
        assert!(
            captured.path.starts_with("POST /rest/v1/rpc/chat_append"),
            "yol: {}",
            captured.path
        );
        // Anahtar VE jeton birlikte gitmeli: RLS `auth.uid()` üzerinden
        // çalıştığı için yalnız anahtarla hiçbir satır görünmez.
        let headers = captured.headers.to_lowercase();
        assert!(headers.contains("apikey: anon-anahtar"), "apikey eksik");
        assert!(
            headers.contains("authorization: bearer kullanici-jetonu"),
            "bearer eksik"
        );
        // Yük base64: ikili veri JSON'da böyle taşınıyor.
        assert_eq!(captured.body["p_payload"], BASE64.encode([1u8, 2, 3]));
        assert_eq!(captured.body["p_chat_id"], "c1");
        assert_eq!(captured.body["p_device"], "dev-a");
        assert_eq!(captured.body["p_batch_id"], "b1");
    }

    #[tokio::test]
    async fn satirlar_base64ten_cozuluyor() {
        let body = r#"[{"seq":3,"device":"dev-b","batch_id":"b9","payload":"AQID"}]"#;
        let (url, server) = fake_postgrest(body, "HTTP/1.1 200 OK").await;
        let store = RestStore::new(url, "anon", "jwt");

        let rows = store.rows_after("c1", 2, Some("dev-a".into())).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 3);
        assert_eq!(rows[0].payload, vec![1, 2, 3], "AQID → 01 02 03");

        let captured = server.await.unwrap();
        assert_eq!(captured.body["p_after"], 2);
        assert_eq!(captured.body["p_exclude_device"], "dev-a");
    }

    #[tokio::test]
    async fn suzgec_yokken_null_gidiyor() {
        let (url, server) = fake_postgrest("[]", "HTTP/1.1 200 OK").await;
        let store = RestStore::new(url, "anon", "jwt");
        store.rows_after("c1", 0, None).await.unwrap();

        let captured = server.await.unwrap();
        // SQL tarafı `is null` kontrolü yapıyor: eksik alan değil, açık null.
        assert!(captured.body["p_exclude_device"].is_null());
    }

    #[tokio::test]
    async fn hata_cevabi_postgres_mesajini_tasiyor() {
        // RLS reddi gibi durumlarda Postgres'in mesajı teşhis için tek ipucu.
        let (url, server) = fake_postgrest(
            r#"{"message":"new row violates row-level security policy"}"#,
            "HTTP/1.1 403 Forbidden",
        )
        .await;
        let store = RestStore::new(url, "anon", "jwt");

        let err = store
            .append("c1", "dev".into(), "b".into(), vec![])
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("403"), "durum kodu görünmeli: {text}");
        assert!(
            text.contains("row-level security"),
            "postgres mesajı korunmalı: {text}"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn bos_durum_cevabi_sifir_oda_demek() {
        let (url, server) = fake_postgrest("[]", "HTTP/1.1 200 OK").await;
        let store = RestStore::new(url, "anon", "jwt");
        // Hiç yazılmamış oda: hata değil, sıfır durum.
        assert_eq!(store.state("yeni").await.unwrap(), RoomState::default());
        server.await.unwrap();
    }

    #[test]
    fn yuk_base64_gidip_geliyor() {
        // Yük opak ve ikili: kodlama gidiş dönüşte bozulursa loro
        // güncellemesi uygulanamaz.
        let payload: Vec<u8> = (0u8..=255).collect();
        let encoded = BASE64.encode(&payload);
        assert_eq!(BASE64.decode(encoded.as_bytes()).unwrap(), payload);
    }
}
