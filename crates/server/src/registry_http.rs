//! Kaydın düz HTTP uçları — WebSocket geçmeyen ağlar için.
//!
//! Gövde biçimleri WebSocket'inkiyle AYNI DEĞİL ve bu bilinçli: push burada
//! `t` etiketi taşımayan çıplak bir `{batch, ops}` nesnesi. İstemci böyle
//! gönderiyor, sunucu da böyle okumak zorunda.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use postillion_doc::RowOp;
use postillion_sync::registry_room::{IncomingFrame, RegistrySession};
use serde::Deserialize;

use crate::App;

#[derive(Deserialize)]
pub struct RowsQuery {
    #[serde(default)]
    since: u64,
    device: Option<String>,
    token: Option<String>,
}

/// `GET /registry/{org}/rows?since=&device=` — `hello`'nun delta cevabının aynısı.
pub async fn get_rows(
    State(app): State<App>,
    Path(org): Path<String>,
    Query(query): Query<RowsQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let identity = app
        .auth
        .identify(&headers, query.token.as_deref())
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !crate::ownership::permits(
        &*app.owners,
        crate::ownership::Scope::Registry,
        &org,
        identity.user_id,
    )
    .await
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Aynı `Session` üzerinden geçiyor: cevabın WebSocket'inkiyle birebir aynı
    // olması gerekiyor ve ikinci bir üretim yolu yazmak, ikisinin zamanla
    // ayrılması demekti.
    let mut session = RegistrySession::new();
    let reply = session
        .respond(
            &*app.registry,
            &org,
            IncomingFrame::Hello {
                // `since = 0` "hiç imleç yok" DEĞİL: istemci ilk çekiminde de
                // sıfır gönderiyor ve tam tablo zaten sıfırdan sonrası.
                cursor: Some(query.since),
                device: query.device.unwrap_or_default(),
            },
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let body = reply
        .to_sender
        .into_iter()
        .next()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(axum::http::header::CONTENT_TYPE, "application/json")], body).into_response())
}

/// HTTP push gövdesi — WebSocket çerçevesinin aksine `t` etiketsiz.
#[derive(Deserialize)]
pub struct PushBody {
    batch: String,
    #[serde(default)]
    ops: Vec<RowOp>,
}

/// `POST /registry/{org}/push` — tek batch, cevap `{batch, seq, applied}`.
pub async fn post_push(
    State(app): State<App>,
    Path(org): Path<String>,
    Query(query): Query<RowsQuery>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<PushBody>,
) -> Result<Response, StatusCode> {
    let identity = app
        .auth
        .identify(&headers, query.token.as_deref())
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !crate::ownership::permits(
        &*app.owners,
        crate::ownership::Scope::Registry,
        &org,
        identity.user_id,
    )
    .await
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Cihaz kimliği push gövdesinde değil sorgu dizesinde; oturum doğrudan
    // hazır kuruluyor.
    let mut session = RegistrySession::ready_for(query.device.unwrap_or_default());

    let reply = session
        .respond(
            &*app.registry,
            &org,
            IncomingFrame::Push {
                batch: body.batch,
                ops: body.ops,
            },
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // HTTP'den gelen bir yazım da bağlı cihazlara ANINDA ulaşmalı: yalnızca
    // WebSocket yolunu yayına bağlamak, kötü ağdaki bir cihazın yazdığını
    // ötekilerin ancak kendi turlarında görmesi demekti.
    for frame in reply.broadcast {
        app.registry_hub.publish(&org, frame);
    }

    let ack = reply
        .to_sender
        .into_iter()
        .next()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(axum::http::header::CONTENT_TYPE, "application/json")], ack).into_response())
}
