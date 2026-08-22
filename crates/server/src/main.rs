//! Sunucu ikilisi — yapılandırmayı okuyup [`postillion_server::router`]'ı bağlar.

use std::net::SocketAddr;
use std::sync::Arc;

use postillion_server::{
    auth::Auth, db, device_room::DeviceHub, health, identity_db::PgIdentity,
    registry_db::PgRegistry, registry_ws::RegistryHub, rooms::ChatHub, App,
};

/// Sunucunun dinleyeceği adres. Konteynerde tüm arayüzler; ağ sınırını
/// Docker ve vekil çiziyor.
fn bind_addr() -> String {
    std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8787".into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Sağlık kontrolü sunucuyu başlatmadan ÖNCE ele alınıyor: veritabanı
    // yapılandırması bu yolda hiç gerekmiyor ve gereksiz kılmak yoklamanın
    // sunucunun kendi sorunlarından bağımsız kalmasını sağlıyor.
    if std::env::args().any(|arg| arg == "--health-check") {
        std::process::exit(if health::check(&bind_addr()).await {
            0
        } else {
            1
        });
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postillion_server=info,tower_http=warn".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL gerekli (postgres://…)"))?;

    // Paylaşılan jeton artık TEK yol değil: panelden üretilen jetonlar da
    // kabul ediliyor. İkisi birlikte çalışıyor çünkü geçiş sırasında ikisi de
    // kullanımda.
    let shared = Auth::from_env()?;

    let pool = db::connect(&database_url).await?;
    tracing::info!("veritabanı hazır");

    let identity = Arc::new(PgIdentity::new(pool.clone()));
    if shared.is_none() {
        tracing::info!("paylaşılan jeton yok; yalnızca panelden üretilen jetonlar kabul ediliyor");
    } else {
        // Paylaşılan jeton BÜTÜN odalara açılan bir ana anahtar. Panelden
        // jeton üretildikten sonra kaldırılmalı; sessizce bırakmak onu
        // unutulmuş bir arka kapıya çevirirdi.
        tracing::warn!(
            "POSTILLION_SERVER_TOKEN tanımlı: tek kullanıcılık kip. \
             Panelden jeton ürettikten sonra bu değişkeni kaldırın."
        );
    }

    let app = App {
        store: Arc::new(db::PgStore::new(pool.clone())),
        registry: Arc::new(PgRegistry::new(pool)),
        owners: identity.clone(),
        hub: ChatHub::new(),
        registry_hub: RegistryHub::new(),
        device_hub: DeviceHub::new(),
        auth: Auth::new_with_store(shared, identity),
    };

    let addr: SocketAddr = bind_addr().parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "dinleniyor");

    axum::serve(listener, postillion_server::router(app)).await?;
    Ok(())
}
