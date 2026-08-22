//! Oda sahipliği — çok kullanıcılı sunucunun izolasyonu.
//!
//! Sohbet ve kayıt odalarının kimliği global: `chat_id` ve `org` istemci
//! tarafından üretiliyor. Sahiplik olmadan bunları bilen herkes içeri
//! girebilirdi ve çok kullanıcılı bir sunucuda bu, herkesin herkesin
//! sohbetini okuması demek.
//!
//! Kural basit: **ilk yazan sahiplenir**. Sahipsiz bir odaya erişen ilk
//! kimlik onu alıyor, sonrakiler kontrol ediliyor. Kayıt önceden yapılmadığı
//! için bu, mevcut odaların çalışmaya devam etmesini de sağlıyor.

use futures::future::BoxFuture;
use postillion_sync::SyncError;

/// Odanın hangi kaydı — sohbetler ve kayıt odaları ayrı ad alanları.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Chat,
    Registry,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Chat => "chat",
            Scope::Registry => "registry",
        }
    }
}

/// Sahiplik kaydı.
pub trait OwnerStore: Send + Sync + 'static {
    /// Odayı `user_id` için sahiplenir ya da mevcut sahibi döndürür.
    ///
    /// ATOMİK olmak zorunda: iki cihaz aynı anda katıldığında ikisi de
    /// sahipsiz görüp ikisi de sahiplenirse izolasyon hiç kurulmamış olur.
    fn claim(
        &self,
        scope: Scope,
        room: &str,
        user_id: i64,
    ) -> BoxFuture<'static, Result<i64, SyncError>>;
}

/// Kimlik bu odaya girebilir mi.
///
/// Depo hatasında REDDEDİLİYOR: geçici bir arıza sırasında izolasyonu
/// tamamen devre dışı bırakmak, arızanın kendisinden kötü.
pub async fn permits(
    store: &dyn OwnerStore,
    scope: Scope,
    room: &str,
    user_id: i64,
) -> bool {
    match store.claim(scope, room, user_id).await {
        Ok(owner) => owner == user_id,
        Err(err) => {
            tracing::warn!(error = %err, room, "sahiplik okunamadı; erişim reddedildi");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemOwners {
        rows: Mutex<HashMap<(String, String), i64>>,
    }

    impl OwnerStore for MemOwners {
        fn claim(
            &self,
            scope: Scope,
            room: &str,
            user_id: i64,
        ) -> BoxFuture<'static, Result<i64, SyncError>> {
            let mut rows = self.rows.lock().unwrap();
            let owner = *rows
                .entry((scope.as_str().to_string(), room.to_string()))
                .or_insert(user_id);
            Box::pin(async move { Ok(owner) })
        }
    }

    #[tokio::test]
    async fn ilk_yazan_sahipleniyor() {
        let store = MemOwners::default();
        assert!(permits(&store, Scope::Chat, "c1", 1).await);
        // Aynı kullanıcı tekrar girebilmeli.
        assert!(permits(&store, Scope::Chat, "c1", 1).await);
    }

    #[tokio::test]
    async fn baskasinin_odasina_girilemiyor() {
        let store = MemOwners::default();
        assert!(permits(&store, Scope::Chat, "c1", 1).await);
        // Oda kimliğini bilmek yetmiyor — asıl izolasyon bu.
        assert!(!permits(&store, Scope::Chat, "c1", 2).await);
    }

    #[tokio::test]
    async fn ad_alanlari_ayri() {
        let store = MemOwners::default();
        assert!(permits(&store, Scope::Chat, "ayni-ad", 1).await);
        // Aynı ada sahip bir KAYIT odası, sohbet odasının sahipliğini
        // devralmamalı: ikisi ayrı kavram.
        assert!(permits(&store, Scope::Registry, "ayni-ad", 2).await);
    }
}
