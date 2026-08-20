//! Sunucu ikilisi — yapılandırmayı okuyup [`postillion_server::router`]'ı bağlar.

use std::net::SocketAddr;
use std::sync::Arc;

use postillion_server::{auth::Auth, db, hub::Hub, App};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postillion_server=info,tower_http=warn".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL gerekli (postgres://…)"))?;

    // Aşama 1 tek kullanıcılık: jeton ortamdan geliyor. Aşama 3'te yerini
    // gerçek oturumlar alacak.
    let auth = Auth::from_env()?;

    let pool = db::connect(&database_url).await?;
    tracing::info!("veritabanı hazır");

    let app = App {
        store: Arc::new(db::PgStore::new(pool)),
        hub: Hub::new(),
        auth,
    };

    let addr: SocketAddr = std::env::var("BIND")
        .unwrap_or_else(|_| "0.0.0.0:8787".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "dinleniyor");

    axum::serve(listener, postillion_server::router(app)).await?;
    Ok(())
}
