//! Panelin kullanıcı adına konuşması — `x-postillion-act-as`.
//!
//! Panel sunucuya İŞLETMECİNİN jetonuyla gidiyor (`POSTILLION_SERVER_TOKEN`),
//! çünkü kullanıcının jetonu tarayıcıya hiç verilmiyor. Ama odalar
//! kullanıcılara ait ve sahiplik denetimi paylaşılan kimliği içeri almıyordu:
//! panelde cihazlar çevrimdışı, transkript "sunucuya ulaşılamadı"
//! görünüyordu — ikisi de 403'tü, ama panel her arızayı aynı şekilde
//! gösterdiği için sebep görünmüyordu.

mod common;

use common::start_shared_and_users;

const SHARED: &str = "isletmeci-jetonu";

/// Cihaz rölesine yükseltme denemesinin HTTP durumu.
///
/// Ham soket: `actAs` SORGU dizesinde, çünkü WebSocket istemcileri el
/// sıkışmaya başlık koyamıyor ve panel röleye tam olarak böyle bağlanıyor.
async fn device_status(port: u16, device: &str, token: &str, act_as: Option<&str>) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("bağlanmalı");
    let extra = act_as.map(|v| format!("&actAs={v}")).unwrap_or_default();
    stream
        .write_all(
            format!(
                "GET /device/{device}/ws?role=client&connId=c1&token={token}{extra} HTTP/1.1\r\n\
                 Host: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Version: 13\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("istek");

    let mut head = [0u8; 12];
    stream.read_exact(&mut head).await.expect("durum satırı");
    String::from_utf8_lossy(&head[9..12]).parse().expect("durum")
}

async fn presence_status(port: u16, org: &str, token: &str, act_as: Option<&str>) -> u16 {
    let mut request = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/registry/{org}/presence"))
        .bearer_auth(token);
    if let Some(user) = act_as {
        request = request.header("x-postillion-act-as", user);
    }
    request.send().await.expect("istek gitmeli").status().as_u16()
}

#[tokio::test]
async fn panel_kullanici_odasina_giriyor() {
    let (port, tokens) = start_shared_and_users(SHARED).await;
    tokens.mint("kullanici-jetonu", 42);

    // Kullanıcı önce giriyor ve odayı sahipleniyor.
    assert_eq!(
        presence_status(port, "org-42", "kullanici-jetonu", None).await,
        200
    );

    // Paylaşılan jeton TEK BAŞINA yetmiyor: oda 42'nin.
    assert_eq!(
        presence_status(port, "org-42", SHARED, None).await,
        403,
        "panelin eski davranışı — cihazlar bu yüzden çevrimdışı görünüyordu"
    );

    // Kimin adına konuştuğunu söyleyince giriyor.
    assert_eq!(
        presence_status(port, "org-42", SHARED, Some("42")).await,
        200
    );
}

/// İZİN YÜKSELTME: bu tutmazsa herhangi bir kullanıcı, oda kimliğini bilerek
/// başkasının odasına girebilir.
#[tokio::test]
async fn kullanici_jetonu_baskasinin_adina_konusamiyor() {
    let (port, tokens) = start_shared_and_users(SHARED).await;
    tokens.mint("ayse", 1);
    tokens.mint("bora", 2);

    assert_eq!(presence_status(port, "org-ayse", "ayse", None).await, 200);
    assert_eq!(
        presence_status(port, "org-ayse", "bora", Some("1")).await,
        403,
        "başlık yalnızca paylaşılan jetonla geçerli olmalı"
    );
}

#[tokio::test]
async fn bozuk_baslik_reddediliyor() {
    let (port, _tokens) = start_shared_and_users(SHARED).await;
    // Çözülemeyen bir başlık paylaşılan kimliğe DÜŞMEMELİ: düşseydi istek
    // sessizce yanlış kullanıcı adına çalışırdı.
    assert_eq!(
        presence_status(port, "org-x", SHARED, Some("abc")).await,
        401
    );
}

/// Panel cihaz rölesine de kullanıcı adına bağlanabilmeli.
///
/// Bu uç sohbet göndermenin tek yolu: mesaj cihazdaki motora buradan
/// gidiyor. Sahiplik denetimi eklendiğinde panel de dışarıda kalırdı.
#[tokio::test]
async fn panel_roleye_kullanici_adina_baglaniyor() {
    let (port, tokens) = start_shared_and_users(SHARED).await;
    tokens.mint("kullanici-jetonu", 42);

    // Kullanıcının motoru odayı sahipleniyor.
    assert_eq!(device_status(port, "dizustu", "kullanici-jetonu", None).await, 101);

    assert_eq!(
        device_status(port, "dizustu", SHARED, None).await,
        403,
        "paylaşılan jeton tek başına kullanıcının cihazına giremez"
    );
    assert_eq!(device_status(port, "dizustu", SHARED, Some("42")).await, 101);
}

/// İZİN YÜKSELTME: sorgu ikizi de yalnızca paylaşılan jetonla geçerli.
#[tokio::test]
async fn kullanici_jetonu_sorguyla_da_baskasi_olamiyor() {
    let (port, tokens) = start_shared_and_users(SHARED).await;
    tokens.mint("ayse", 1);
    tokens.mint("bora", 2);

    assert_eq!(device_status(port, "ayse-dizustu", "ayse", None).await, 101);
    assert_eq!(
        device_status(port, "ayse-dizustu", "bora", Some("1")).await,
        403,
        "sorgu parametresi üretilmiş jetona ayrıcalık vermemeli"
    );
}
