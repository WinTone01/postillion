//! Oda merkezi — bir sohbete yazılan satırın diğer cihazlara ulaşması.
//!
//! Depo kalıcılığı sağlıyor ama canlılığı sağlamıyor: bir cihaz satır
//! yazdığında bağlı diğer cihazların bunu **beklemeden** öğrenmesi gerekiyor,
//! yoksa eşitleme yoklamaya iner ve "anlık" olmaz.
//!
//! Cloudflare'de bunu Durable Object'in tek örnekliliği veriyordu; Supabase
//! düşünülürken Realtime verecekti. Kendi sunucumuzda bu iş burada ve
//! kontrolümüzde.
//!
//! Sohbet başına bir yayın kanalı var ve kanallar **abonesi kalmayınca
//! düşüyor**: aksi halde uzun ömürlü bir süreçte her açılmış sohbet için bir
//! kanal sonsuza kadar bellekte kalırdı.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

/// Kanal kapasitesi.
///
/// Yavaş bir abone bu kadar satır geride kalırsa yayından düşüyor. Kayıp
/// değil: istemci bağlantıyı yeniden kurduğunda imlecinden itibaren depodan
/// yakalıyor — canlı yol bir hızlandırma, doğruluk kaynağı değil.
const CAPACITY: usize = 256;

/// `T` yayılan mesaj tipi: sohbet odaları satır taşıyor, kayıt odası kodlanmış
/// JSON çerçeve. Aynı fanout mantığının iki kopyasını tutmak, birinde
/// düzeltilen bir hatanın ötekinde kalması demekti.
#[derive(Clone)]
pub struct Hub<T: Clone + Send + 'static> {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<T>>>>,
}

impl<T: Clone + Send + 'static> Default for Hub<T> {
    fn default() -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<T: Clone + Send + 'static> Hub<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Odanın yayınına abone olur; oda yoksa açılıyor.
    pub fn subscribe(&self, chat_id: &str) -> broadcast::Receiver<T> {
        let mut rooms = self.rooms.lock().expect("hub kilidi");
        if let Some(tx) = rooms.get(chat_id) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(CAPACITY);
        rooms.insert(chat_id.to_string(), tx);
        rx
    }

    /// Yazılan satırı odadaki dinleyicilere duyurur.
    ///
    /// Dinleyici yoksa kanal kaldırılıyor: tek seferlik bir sohbete yazıp
    /// çıkan bir cihaz arkasında kalıcı bir kanal bırakmamalı.
    pub fn publish(&self, chat_id: &str, row: T) {
        let mut rooms = self.rooms.lock().expect("hub kilidi");
        let Some(tx) = rooms.get(chat_id) else {
            return;
        };
        if tx.send(row).is_err() {
            rooms.remove(chat_id);
        }
    }

    /// Açık oda sayısı — teşhis ve test için.
    pub fn open_rooms(&self) -> usize {
        self.rooms.lock().expect("hub kilidi").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postillion_sync::room::Row;

    type TestHub = Hub<Row>;

    fn row(seq: u64, device: &str) -> Row {
        Row {
            seq,
            device: device.into(),
            batch_id: format!("b{seq}"),
            payload: vec![seq as u8],
        }
    }

    #[tokio::test]
    async fn yazilan_satir_abonelere_ulasiyor() {
        let hub = TestHub::new();
        let mut a = hub.subscribe("c1");
        let mut b = hub.subscribe("c1");

        hub.publish("c1", row(1, "dev-a"));

        // İki cihaz da aynı satırı görmeli — yayın kopyalıyor.
        assert_eq!(a.recv().await.unwrap().seq, 1);
        assert_eq!(b.recv().await.unwrap().seq, 1);
    }

    #[tokio::test]
    async fn odalar_birbirine_sizmiyor() {
        let hub = TestHub::new();
        let mut c1 = hub.subscribe("c1");
        let _c2 = hub.subscribe("c2");

        hub.publish("c2", row(1, "dev-a"));
        // c1 aboneliği c2'nin satırını GÖRMEMELİ.
        assert!(c1.try_recv().is_err(), "odalar arası sızıntı");
    }

    #[tokio::test]
    async fn dinleyicisiz_oda_temizleniyor() {
        let hub = TestHub::new();
        {
            let _rx = hub.subscribe("c1");
            assert_eq!(hub.open_rooms(), 1);
        } // abone düştü

        // İlk yayın kanalın ölü olduğunu fark edip kaldırıyor: uzun ömürlü
        // bir süreçte her sohbet için kalıcı kanal birikmesi bellek sızıntısı.
        hub.publish("c1", row(1, "dev-a"));
        assert_eq!(hub.open_rooms(), 0, "abonesiz oda kaldırılmalı");
    }

    #[tokio::test]
    async fn yayin_olmayan_odaya_zarar_vermiyor() {
        // Hiç abonesi olmayan bir odaya yazmak hata değil: cihaz tek başına
        // çalışıyor olabilir.
        let hub = TestHub::new();
        hub.publish("hic-acilmamis", row(1, "dev-a"));
        assert_eq!(hub.open_rooms(), 0);
    }
}
