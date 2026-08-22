//! Metin `"ping"`/`"pong"` canlılık çifti — üç WebSocket ucunun ortak sözü.
//!
//! Bu, Cloudflare Durable Object'in
//! `setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"))`
//! davranışının kopyası. TS uçta çalışma zamanı cevaplıyor ve DO hiç
//! uyanmıyor; kendi sunucumuzda böyle bir mekanizma YOK, dolayısıyla cevabı
//! elle yazmak zorundayız.
//!
//! Yazılmadığında hata gibi görünmüyor: soket kuruluyor, çerçeveler gidiyor,
//! ama istemciye hiçbir şey GELMEDİĞİ için sessizlik kirası doluyor ve
//! bağlantı 25 saniyede bir kendini yeniden kuruyor — uygulama kalıcı olarak
//! "reconnecting" gösteriyor. Üç uçta da tam olarak bu oldu.
//!
//! Protokol çerçevesi DEĞİL: kayıt ucu JSON metin, diğer ikisi ikili konuşuyor
//! ve bu iki sözcük hiçbirinin gramerine girmiyor.

/// İstemcinin gönderdiği yoklama.
pub const PING: &str = "ping";
/// Sunucunun vermesi gereken karşılık.
pub const PONG: &str = "pong";

/// Metin çerçevesi bir yoklama mı?
pub fn is_ping(text: &str) -> bool {
    text == PING
}
