//! Kullanıcı izolasyonu — çok kullanıcılı sunucunun asıl güvenlik iddiası.
//!
//! Oda kimlikleri (`chatId`, `org`, cihaz kimliği) istemci tarafından
//! üretiliyor ve tahmin edilebilir. Sahiplik olmadan bunları bilen herkes
//! içeri girebilirdi; bu testler o kapının kapalı olduğunu ölçüyor.
//!
//! Paylaşılan jeton bilerek kapalı: o bütün odalara açılan bir ana anahtar ve
//! varlığında bu testler hiçbir şey kanıtlamazdı.

mod common;

use common::start_multi_user;

/// Yükseltme denemesinin HTTP durumunu döndürür.
async fn ws_status(port: u16, path: &str, token: &str) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("bağlanmalı");
    let sep = if path.contains('?') { '&' } else { '?' };
    stream
        .write_all(
            format!(
                "GET {path}{sep}token={token} HTTP/1.1\r\nHost: localhost\r\n\
                 Upgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Version: 13\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("istek");

    let mut head = [0u8; 12];
    stream.read_exact(&mut head).await.expect("durum satırı");
    String::from_utf8_lossy(&head[9..12])
        .parse()
        .expect("durum kodu")
}

async fn http_status(port: u16, path: &str, token: &str) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("bağlanmalı");
    let sep = if path.contains('?') { '&' } else { '?' };
    stream
        .write_all(
            format!(
                "GET {path}{sep}token={token} HTTP/1.1\r\n\
                 Host: localhost\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("istek");
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).await.expect("durum satırı");
    String::from_utf8_lossy(&head[9..12])
        .parse()
        .expect("durum kodu")
}

#[tokio::test]
async fn baskasinin_sohbetine_girilemiyor() {
    let (port, tokens) = start_multi_user().await;
    tokens.mint("ayse-jetonu", 1);
    tokens.mint("bora-jetonu", 2);

    // Ayşe sohbeti açıyor ve sahipleniyor.
    assert_eq!(ws_status(port, "/chat2/gizli/ws", "ayse-jetonu").await, 101);

    // Bora sohbetin KİMLİĞİNİ biliyor. İzolasyonun sınandığı yer tam burası:
    // kimlik tahmin edilebilir, dolayısıyla tek koruma sahiplik.
    assert_eq!(
        ws_status(port, "/chat2/gizli/ws", "bora-jetonu").await,
        403,
        "başkasının sohbetine girilebiliyor"
    );

    // Ayşe kendi sohbetine geri girebilmeli.
    assert_eq!(ws_status(port, "/chat2/gizli/ws", "ayse-jetonu").await, 101);
}

#[tokio::test]
async fn baskasinin_kaydina_girilemiyor() {
    let (port, tokens) = start_multi_user().await;
    tokens.mint("ayse-jetonu", 1);
    tokens.mint("bora-jetonu", 2);

    assert_eq!(ws_status(port, "/registry/org-a/ws", "ayse-jetonu").await, 101);
    assert_eq!(ws_status(port, "/registry/org-a/ws", "bora-jetonu").await, 403);
}

#[tokio::test]
async fn baskasinin_cihazina_baglanilamiyor() {
    let (port, tokens) = start_multi_user().await;
    tokens.mint("ayse-jetonu", 1);
    tokens.mint("bora-jetonu", 2);

    // Ayşe'nin motoru host olarak bağlanıyor.
    assert_eq!(
        ws_status(port, "/device/ayse-dizustu/ws?role=host", "ayse-jetonu").await,
        101
    );

    // Bora o cihaza istemci olarak bağlanamamalı: bağlanabilseydi Ayşe'nin
    // makinesinde RPC çalıştırabilirdi.
    assert_eq!(
        ws_status(
            port,
            "/device/ayse-dizustu/ws?role=client&connId=c1",
            "bora-jetonu"
        )
        .await,
        403,
        "başkasının cihazına bağlanılabiliyor"
    );
}

/// Transkript ucu da sahipliğe tabi: içeriği okumak, odaya girmekten daha
/// hassas.
#[tokio::test]
async fn baskasinin_transkripti_okunamiyor() {
    let (port, tokens) = start_multi_user().await;
    tokens.mint("ayse-jetonu", 1);
    tokens.mint("bora-jetonu", 2);

    assert_eq!(http_status(port, "/chat2/gizli/messages", "ayse-jetonu").await, 200);
    assert_eq!(http_status(port, "/chat2/gizli/messages", "bora-jetonu").await, 403);
}

#[tokio::test]
async fn gecersiz_jeton_hicbir_yere_giremiyor() {
    let (port, tokens) = start_multi_user().await;
    tokens.mint("ayse-jetonu", 1);

    assert_eq!(ws_status(port, "/chat2/c1/ws", "uydurma").await, 401);
    assert_eq!(http_status(port, "/chat2/c1/messages", "uydurma").await, 401);
    assert_eq!(ws_status(port, "/registry/org/ws", "uydurma").await, 401);
}
