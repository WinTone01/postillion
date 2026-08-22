//! Kimin çevrimiçi olduğu — BELLEKTE.
//!
//! Diske yazılmıyor ve bu bilinçli: çevrimiçi kalmak sunucu durumunu
//! büyütmemeli. Bir cihaz her 15 saniyede bir atış gönderiyor ve kayıt
//! satırlarındaki `lastSeenAt` bunu taşımıyor — o yalnızca açılış ve
//! kapanışta yazılan bir satır.
//!
//! Sunucu yeniden başladığında harita boşalıyor. Kayıp değil: cihazlar bir
//! sonraki atışlarında geri görünüyor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bir atışın geçerlilik süresi.
///
/// İstemcinin 15 saniyelik atış aralığının iki katı: tek bir kaçırılmış atış
/// cihazı çevrimdışı göstermemeli.
pub const TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub struct Presence {
    /// oda → (cihaz → (istemcinin bildirdiği an, bizim aldığımız an))
    ///
    /// İki zaman ayrı tutuluyor: istemcinin damgası arayüzde gösteriliyor ama
    /// saati yanlış bir cihaz sonsuza kadar çevrimiçi görünebilirdi, o yüzden
    /// süre dolumu BİZİM saatimize göre.
    rooms: Arc<Mutex<HashMap<String, HashMap<String, (i64, Instant)>>>>,
}

impl Presence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn beat(&self, org: &str, device: &str, at: i64) {
        let mut rooms = self.rooms.lock().expect("presence kilidi");
        rooms
            .entry(org.to_string())
            .or_default()
            .insert(device.to_string(), (at, Instant::now()));
    }

    /// TTL içindeki cihazlar.
    ///
    /// Süresi dolanlar okuma sırasında temizleniyor: ayrı bir süpürme görevi,
    /// hiç okunmayan bir odayı sonsuza kadar bellekte tutmamak dışında bir
    /// şey kazandırmazdı ve o oda zaten büyümüyor.
    pub fn live(&self, org: &str) -> HashMap<String, i64> {
        let mut rooms = self.rooms.lock().expect("presence kilidi");
        let Some(room) = rooms.get_mut(org) else {
            return HashMap::new();
        };
        room.retain(|_, (_, seen)| seen.elapsed() < TTL);
        let out: HashMap<String, i64> = room.iter().map(|(d, (at, _))| (d.clone(), *at)).collect();
        if room.is_empty() {
            rooms.remove(org);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atis_cihazi_canli_yapiyor() {
        let p = Presence::new();
        p.beat("org", "dev-a", 1_700);
        assert_eq!(p.live("org").get("dev-a"), Some(&1_700));
    }

    #[test]
    fn odalar_birbirine_karismiyor() {
        let p = Presence::new();
        p.beat("org-a", "dev-a", 1);
        assert!(p.live("org-b").is_empty());
    }

    #[test]
    fn bos_oda_bos_harita() {
        assert!(Presence::new().live("hic-olmayan").is_empty());
    }

    /// İstemcinin damgası TTL'i belirlememeli: saati ileri kaçmış bir cihaz
    /// aksi hâlde sonsuza kadar çevrimiçi görünürdü.
    #[test]
    fn istemcinin_saati_ttl_belirlemiyor() {
        let p = Presence::new();
        // Çok uzak bir gelecek damgası.
        p.beat("org", "dev-a", i64::MAX);
        // Yine de canlı — çünkü BİZ az önce aldık.
        assert!(p.live("org").contains_key("dev-a"));
    }
}
