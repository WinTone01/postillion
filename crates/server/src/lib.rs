//! postillion-server — kendi sunucumuzdaki eşitleme ucu.
//!
//! Cloudflare Durable Object'in yaptığı işi yapıyor: sohbet satırlarını
//! saklamak ve bağlı cihazlara duyurmak. Protokol mantığı `postillion-sync`
//! içindeki [`postillion_sync::room`] modülünden geliyor — istemci ile sunucu
//! AYNI kodu kullanıyor, bu yüzden iki ucun birbirinden kayması mümkün değil.
//!
//! Kütüphane olarak da açılıyor çünkü entegrasyon testleri sunucuyu gerçek bir
//! TCP soketinde ayağa kaldırıp gerçek `ChatClient` ile konuşturuyor. Durumsuz
//! adaptör hatasını yakalayan buydu: birim testleri geçiyordu ama gerçek
//! istemci cihaz kimliği boş satırlar üretiyordu.

pub mod auth;
pub mod db;
pub mod health;
pub mod registry_db;
pub mod registry_http;
pub mod registry_ws;
pub mod http;
pub mod hub;
pub mod rooms;

use std::sync::Arc;

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use postillion_sync::room::ChatStore;
use tower_http::trace::TraceLayer;

use crate::registry_ws::RegistryHub;
use crate::rooms::ChatHub;

#[derive(Clone)]
pub struct App {
    pub store: Arc<dyn ChatStore>,
    pub registry: Arc<dyn postillion_sync::registry_room::RegistryStore>,
    pub hub: ChatHub,
    pub registry_hub: RegistryHub,
    pub auth: auth::Auth,
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/chat2/{chat_id}/ws", get(chat_ws))
        // Uçak-wifi taşıması: WebSocket kurulamayan ağlarda istemci aynı
        // satırları düz HTTP üzerinden çekip gönderiyor.
        .route(
            "/chat2/{chat_id}/rows",
            get(http::get_rows).post(http::post_rows),
        )
        .route("/chat2/{chat_id}/checkpoint", get(http::get_checkpoint))
        // Çalışma alanı kaydı: kenar çubuğu satırları ve presence. Uygulama
        // bu uç olmadan bağlanamıyor — 404 alıp sonsuza kadar yeniden
        // bağlanmayı deniyor.
        .route("/registry/{org}/ws", get(registry_ws_handler))
        .route("/registry/{org}/rows", get(registry_http::get_rows))
        .route("/registry/{org}/push", axum::routing::post(registry_http::post_push))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

/// Ters vekilin ve dağıtım betiğinin bakacağı uç.
async fn health() -> &'static str {
    "ok"
}

/// Kök yol.
///
/// Yalnızca `/health` ve `/chat2/…` olsaydı, adresi tarayıcıya yapıştıran
/// biri çıplak bir 404 görüp sunucuyu bozuk sanardı — ilk kurulumda tam da
/// bu oldu. Sürüm bilgisi bilerek yok: kimliğini söylemek yeterli, sürümünü
/// duyurmak yalnızca saldırganın işini kolaylaştırır.
async fn root() -> &'static str {
    "postillion-server\n\nSağlık: /health\nSohbet soketi: /chat2/{chatId}/ws\n"
}

/// `?token=…` — tarayıcı WebSocket API'si başlık koyamadığı için istemci
/// jetonu sorguda taşıyor.
#[derive(serde::Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn registry_ws_handler(
    State(app): State<App>,
    Path(org): Path<String>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    if !app.auth.permits(&headers, query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(ws.on_upgrade(move |socket| {
        registry_ws::serve(socket, app.registry, app.registry_hub, org)
    }))
}

async fn chat_ws(
    State(app): State<App>,
    Path(chat_id): Path<String>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // Yükseltmeden ÖNCE doğrulanıyor: yükseltme tamamlandıktan sonra
    // reddetmek istemciye düzgün bir HTTP durumu döndüremezdi.
    if !app.auth.permits(&headers, query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(ws.on_upgrade(move |socket| rooms::serve(socket, app.store, app.hub, chat_id)))
}
