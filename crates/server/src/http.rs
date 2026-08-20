//! Düz HTTP satır uçları — "uçak wifi" taşıması.
//!
//! Bazı ağlar WebSocket yükseltmesine izin vermiyor. İstemci o durumda aynı
//! satırları GET/POST ile çekip gönderiyor. Gövde formatı WebSocket'inkiyle
//! AYNI çerçeveler, sadece u32-LE uzunlukla önekli — bu yüzden burada ayrı
//! bir protokol uygulaması yok: sentetik `HELLO` + gerçek çerçeve ile aynı
//! [`Session`] çalıştırılıyor. İkinci bir uygulama yazmak, iki yolun zamanla
//! birbirinden ayrılması demekti.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use postillion_sync::chat_frames::{self as wire, frame_type};
use postillion_sync::room::Session;

use crate::App;

#[derive(serde::Deserialize)]
pub struct RowsQuery {
    #[serde(default)]
    after: u64,
    device: Option<String>,
    #[serde(rename = "batchId")]
    batch_id: Option<String>,
    token: Option<String>,
}

/// Cihaz kimliğini oturuma yerleştiren sentetik `HELLO`.
///
/// WebSocket'te bu bilgi bağlantı boyunca yaşıyor; HTTP'de her istek kendi
/// başına olduğu için her seferinde yeniden kuruluyor. `HELLO`'nun cevabı
/// zaten STATE — GET yolunda gövdenin ilk çerçevesi olarak kullanılıyor.
fn hello_frame(device: &str) -> wire::WireFrame {
    decoded(wire::encode(
        frame_type::HELLO,
        &serde_json::json!({ "device": device }),
        &[],
    ))
}

/// u32-LE uzunluk önekiyle çerçeveleri birleştirir.
fn frame_body(frames: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        out.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        out.extend_from_slice(&frame);
    }
    out
}

fn authorized(app: &App, headers: &HeaderMap, token: Option<&str>) -> bool {
    app.auth.permits(headers, token)
}

/// `GET /chat2/{id}/rows?after=&device=` — STATE, ROW*, ROWS_DONE.
pub async fn get_rows(
    State(app): State<App>,
    Path(chat_id): Path<String>,
    Query(query): Query<RowsQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if !authorized(&app, &headers, query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let device = query.device.unwrap_or_default();
    let mut session = Session::new();

    let mut frames = Vec::new();
    // STATE önce: istemci gövdenin ilk çerçevesini STATE olarak okuyor.
    frames.extend(
        session
            .respond(&*app.store, &chat_id, &hello_frame(&device))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    frames.extend(
        session
            .respond(
                &*app.store,
                &chat_id,
                &decoded(wire::encode(
                    frame_type::ROWS_REQ,
                    &serde_json::json!({ "after": query.after, "excludeOwn": true }),
                    &[],
                )),
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );

    Ok(frame_body(frames).into_response())
}

/// `POST /chat2/{id}/rows?batchId=&device=` — tek bir gönderim, JSON ack.
pub async fn post_rows(
    State(app): State<App>,
    Path(chat_id): Path<String>,
    Query(query): Query<RowsQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    if !authorized(&app, &headers, query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let Some(batch_id) = query.batch_id else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let device = query.device.unwrap_or_default();

    // WebSocket'teki gibi yayına da düşmeli: aynı sohbette bağlı duran bir
    // cihaz, HTTP'den gelen satırı beklemeden görmeli.
    let store = crate::rooms::publishing(app.store.clone(), app.hub.clone());
    let mut session = Session::new();
    let _ = session
        .respond(&*store, &chat_id, &hello_frame(&device))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let replies = session
        .respond(
            &*store,
            &chat_id,
            &decoded(wire::encode(
                frame_type::PUSH,
                &serde_json::json!({ "batchId": batch_id }),
                &body,
            )),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // İstemci ACK'in BAŞLIĞINI düz JSON olarak bekliyor, çerçeveyi değil.
    let ack = replies
        .first()
        .and_then(|frame| wire::decode(frame))
        .map(|frame| frame.header)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(axum::Json(ack).into_response())
}

/// `GET /chat2/{id}/checkpoint`.
///
/// Aşama 1'de anlık görüntü yazan bir yol yok, dolayısıyla hiç görüntü yok.
/// 404 istemcinin beklediği cevap: tüm satırları baştan çekiyor. Sessizce boş
/// gövde döndürmek istemcinin bozuk bir görüntüyü içe aktarmasına yol açardı.
pub async fn get_checkpoint(
    State(app): State<App>,
    Query(query): Query<RowsQuery>,
    headers: HeaderMap,
) -> StatusCode {
    if !authorized(&app, &headers, query.token.as_deref()) {
        return StatusCode::UNAUTHORIZED;
    }
    StatusCode::NOT_FOUND
}

fn decoded(bytes: Vec<u8>) -> wire::WireFrame {
    wire::decode(&bytes).expect("kendi ürettiğimiz çerçeve çözülebilir")
}
