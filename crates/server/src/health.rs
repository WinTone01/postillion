//! Konteyner sağlık kontrolü — `postillion-server --health-check`.
//!
//! Alternatifi çalışma imajına `curl` kurmaktı. Bunun için imajda paket
//! yöneticisi ve bir kabuk tutmak gerekiyordu; ikili kendi kendini
//! yoklayınca çalışma katmanı kabuksuz kalabiliyor ve saldırı yüzeyi
//! sunucunun kendisinden ibaret oluyor.
//!
//! Kasten ham TCP: bir HTTP istemcisi kütüphanesi eklemek, yalnızca bu
//! yoklama için sunucuya koca bir bağımlılık ağacı takardı.

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

/// Yoklamanın toplam süresi.
///
/// Docker sağlık kontrolünün kendi zaman aşımı var ama ona bırakmak,
/// süreç sonlandırılana kadar asılı bir bağlantı bırakırdı.
const DEADLINE: Duration = Duration::from_secs(5);

/// `/health` `200` mü. Doğruysa `0`, değilse `1` ile çıkılıyor.
pub async fn check(bind: &str) -> bool {
    // `BIND` çoğunlukla `0.0.0.0:8787`. Bu adrese BAĞLANILAMAZ; yoklama
    // konteynerin içinden geldiği için geri döngüye çevriliyor.
    let port = bind.rsplit(':').next().unwrap_or("8787");
    let addr = format!("127.0.0.1:{port}");

    matches!(
        tokio::time::timeout(DEADLINE, probe(addr)).await,
        Ok(Ok(true))
    )
}

async fn probe(addr: String) -> std::io::Result<bool> {
    let mut stream = TcpStream::connect(&addr).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;

    // Yalnızca durum satırı okunuyor; gövdeyi beklemenin bir faydası yok.
    let mut head = [0u8; 15];
    stream.read_exact(&mut head).await?;
    Ok(head.starts_with(b"HTTP/1.1 200"))
}
