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
pub mod device_room;
pub mod health;
pub mod registry_db;
pub mod registry_http;
pub mod registry_ws;
pub mod http;
pub mod identity_db;
pub mod hub;
pub mod ownership;
pub mod presence;
pub mod rooms;
pub mod transcript;

use std::sync::Arc;

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use postillion_sync::room::ChatStore;
use tower_http::trace::TraceLayer;

use crate::device_room::DeviceHub;
use crate::registry_ws::RegistryHub;
use crate::rooms::ChatHub;

#[derive(Clone)]
pub struct App {
    pub store: Arc<dyn ChatStore>,
    pub registry: Arc<dyn postillion_sync::registry_room::RegistryStore>,
    pub hub: ChatHub,
    pub registry_hub: RegistryHub,
    pub device_hub: DeviceHub,
    pub owners: Arc<dyn ownership::OwnerStore>,
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
        // Panel için materyalize transkript — bilgisayar kapalıyken de okunur.
        .route("/chat2/{chat_id}/messages", get(http::get_messages))
        // Çalışma alanı kaydı: kenar çubuğu satırları ve presence. Uygulama
        // bu uç olmadan bağlanamıyor — 404 alıp sonsuza kadar yeniden
        // bağlanmayı deniyor.
        .route("/registry/{org}/ws", get(registry_ws_handler))
        .route("/registry/{org}/rows", get(registry_http::get_rows))
        .route("/registry/{org}/push", axum::routing::post(registry_http::post_push))
        // Panel için: kimin çevrimiçi olduğu. Kayıt satırları bu soruyu
        // cevaplamıyor — `lastSeenAt` yalnızca açılış/kapanışta yazılıyor.
        .route("/registry/{org}/presence", get(registry_http::get_presence))
        // Cihazlar arası RPC rölesi: uzaktan terminal, hedef cihaza mesaj.
        .route("/device/{device_id}/ws", get(device_ws_handler))
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

/// Röle bağlantısının rolü ve kimliği.
#[derive(serde::Deserialize)]
struct DeviceQuery {
    token: Option<String>,
    role: Option<String>,
    #[serde(rename = "connId")]
    conn_id: Option<String>,
    /// Panelin "şu kullanıcı adına" değeri. Başlık DEĞİL sorgu, çünkü
    /// WebSocket istemcileri el sıkışmaya başlık koyamıyor — jetonun da
    /// burada olmasının sebebi aynı.
    #[serde(rename = "actAs")]
    act_as: Option<String>,
}

async fn device_ws_handler(
    State(app): State<App>,
    Path(device_id): Path<String>,
    Query(query): Query<DeviceQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // Cihaz odası sahipliği cihazın KENDİSİNE bağlı: bir kullanıcının
    // cihazına başka bir kullanıcının host ya da istemci olarak bağlanması,
    // izolasyonun en doğrudan ihlali olurdu.
    let identity = app
        .auth
        .identify(&headers, query.token.as_deref(), query.act_as.as_deref())
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !ownership::permits(
        &*app.owners,
        ownership::Scope::Device,
        &device_id,
        identity.user_id,
    )
    .await
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let role = device_room::Role::parse(query.role.as_deref());
    // `connId` vermeyen istemciye biz üretiyoruz: host'un cevabı bir yere
    // gitmek zorunda ve kimliksiz bir istemciye yönlendirme yapılamaz.
    let conn_id = query
        .conn_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Ok(ws.on_upgrade(move |socket| {
        device_room::serve(socket, app.device_hub, device_id, role, conn_id)
    }))
}

async fn registry_ws_handler(
    State(app): State<App>,
    Path(org): Path<String>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    let identity = app
        .auth
        .identify(&headers, query.token.as_deref(), None)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !ownership::permits(
        &*app.owners,
        ownership::Scope::Registry,
        &org,
        identity.user_id,
    )
    .await
    {
        return Err(StatusCode::FORBIDDEN);
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
    let identity = app
        .auth
        .identify(&headers, query.token.as_deref(), None)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !ownership::permits(
        &*app.owners,
        ownership::Scope::Chat,
        &chat_id,
        identity.user_id,
    )
    .await
    {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(ws.on_upgrade(move |socket| rooms::serve(socket, app.store, app.hub, chat_id)))
}
