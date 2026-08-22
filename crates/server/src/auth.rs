//! Jeton doğrulama ve kimlik çözümü.
//!
//! Bir jeton artık yalnızca "geçerli mi" sorusuna değil, **kim** sorusuna da
//! cevap veriyor. Sohbet ve kayıt odalarının sahibi buradan çıkıyor;
//! sahiplik olmadan çok kullanıcılı bir sunucuda herkes herkesin sohbetini
//! okurdu.
//!
//! İki jeton kaynağı var ve ikisi de gerekli:
//!
//! - **Paylaşılan jeton** (`POSTILLION_SERVER_TOKEN`). Tek kullanıcılık kip.
//!   Kaldırılmadı çünkü çalışan kurulumları kırardı; ama artık tek yol değil
//!   ve panelden jeton üretildikten sonra kaldırılması gerekiyor — sunucu
//!   açılışta bunu hatırlatıyor.
//! - **Panelden üretilen jetonlar** (`api_tokens` tablosu). Her biri bir
//!   kullanıcıya ait.
//!
//! Jeton İKİ yoldan sunulabiliyor: WebSocket `?token=` sorgusuyla (tarayıcı
//! WS API'si başlık koymaya izin vermiyor) ve HTTP'de `Authorization: Bearer`.

use std::sync::Arc;

use axum::http::HeaderMap;
use futures::future::BoxFuture;
use postillion_sync::SyncError;
use sha2::{Digest, Sha256};

/// Paylaşılan jetonun temsil ettiği kullanıcı.
///
/// Negatif: gerçek kullanıcı kimlikleri Postgres'ten geliyor ve pozitif.
/// Çakışma olamaz, dolayısıyla tek kullanıcılık kipteki veri gerçek bir
/// kullanıcıya ait görünmüyor.
pub const SHARED_USER: i64 = -1;

/// Panelin kullanıcı adına konuşurken kullandığı başlık — bkz. [`act_as`].
pub const ACT_AS_HEADER: &str = "x-postillion-act-as";

/// Bir isteğin arkasındaki kimlik.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub user_id: i64,
}

impl Identity {
    /// Tek kullanıcılık kipten mi geliyor.
    pub fn is_shared(&self) -> bool {
        self.user_id == SHARED_USER
    }
}

/// Panelden üretilmiş jetonların deposu.
pub trait TokenStore: Send + Sync + 'static {
    /// Jetonun ÖZETİNİ arar. Ham jeton hiç saklanmıyor.
    fn lookup(&self, token_hash: &str) -> BoxFuture<'static, Result<Option<i64>, SyncError>>;
}

/// Jetonun saklanan biçimi.
///
/// SHA-256, bcrypt/scrypt değil: bu bir parola değil, yüksek entropili rastgele
/// bir dize. Yavaş bir türetme fonksiyonu kaba kuvvete karşı hiçbir şey
/// kazandırmaz (tahmin edilecek bir "kolay parola" yok) ama her isteğe
/// milisaniyeler ekler — jeton her WebSocket açılışında doğrulanıyor.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Clone)]
pub struct Auth {
    shared: Option<String>,
    store: Option<Arc<dyn TokenStore>>,
}

impl Auth {
    /// Yalnızca paylaşılan jeton — testler ve tek kullanıcılık kip.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            shared: Some(token.into()),
            store: None,
        }
    }

    /// Ortamdaki paylaşılan jeton (varsa) ve panel jetonlarının deposu.
    ///
    /// İkisi de yoksa hiçbir istek geçemez; bu bir yapılandırma hatası ve
    /// sessizce kapalı bir sunucu yerine açılışta durmak doğru.
    pub fn new_with_store(shared: Option<String>, store: Arc<dyn TokenStore>) -> Self {
        Self {
            shared: shared.filter(|t| !t.trim().is_empty()),
            store: Some(store),
        }
    }

    pub fn from_env() -> anyhow::Result<Option<String>> {
        let token = std::env::var("POSTILLION_SERVER_TOKEN").ok();
        if let Some(token) = &token
            && token.trim().is_empty()
        {
            // Boş bir jeton, tanımlı ama işlevsiz. Sessizce yok saymak
            // "ayarladım" sanan birini korumasız bırakırdı.
            anyhow::bail!("POSTILLION_SERVER_TOKEN tanımlı ama boş");
        }
        Ok(token)
    }

    /// İsteğin arkasındaki kimlik; jeton geçersizse `None`.
    pub async fn identify(&self, headers: &HeaderMap, query_token: Option<&str>) -> Option<Identity> {
        let presented = query_token
            .map(str::to_string)
            .or_else(|| bearer(headers))?;
        let presented = presented.trim();
        if presented.is_empty() {
            return None;
        }

        // Paylaşılan jeton ÖNCE: veritabanına gitmeden karara varılabiliyorsa
        // gidilmiyor, ve tek kullanıcılık kurulumda depo hiç olmayabilir.
        if let Some(shared) = &self.shared
            && constant_time_eq(shared, presented)
        {
            return match act_as(headers) {
                // Başlık var ama okunamıyor: paylaşılan kimliğe DÜŞMEK
                // yerine reddediliyor. Düşseydi, bozuk bir başlık isteği
                // sessizce yanlış kullanıcı adına çalıştırırdı.
                Some(None) => None,
                Some(Some(user_id)) => Some(Identity { user_id }),
                None => Some(Identity {
                    user_id: SHARED_USER,
                }),
            };
        }

        let store = self.store.as_ref()?;
        match store.lookup(&hash_token(presented)).await {
            Ok(Some(user_id)) => Some(Identity { user_id }),
            Ok(None) => None,
            Err(err) => {
                // Veritabanı hatasında REDDEDİLİYOR. Açık kalmak, geçici bir
                // arıza sırasında kimlik denetimini tamamen devre dışı
                // bırakmak olurdu.
                tracing::warn!(error = %err, "jeton aranamadı; istek reddedildi");
                None
            }
        }
    }
}

/// Panelin "şu kullanıcı adına" başlığı.
///
/// Panel bir SUNUCU, kendi adına konuşmuyor: listeleri veritabanından
/// okuyor ama canlılık ve transkript sunucuda ve oralara girmek oda
/// sahipliğine takılıyor. Paylaşılan jetonun kimliği `SHARED_USER` ve
/// kullanıcıya ait bir odaya o kimlikle girilemiyor — panel bu yüzden
/// cihazları çevrimdışı, transkripti ulaşılamaz gösteriyordu.
///
/// YALNIZCA paylaşılan jetonla geçerli: o zaten işletmecinin ana anahtarı.
/// Üretilmiş bir kullanıcı jetonuyla da kabul edilseydi, herhangi bir
/// kullanıcı başka bir kullanıcının kimliğine bürünebilirdi.
///
/// `Some(None)` başlığın var ama çözülemez olduğu durum — çağıran bunu
/// reddetmeli.
fn act_as(headers: &HeaderMap) -> Option<Option<i64>> {
    let raw = headers.get(ACT_AS_HEADER)?;
    Some(
        raw.to_str()
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            // Gerçek kullanıcı kimlikleri pozitif; `SHARED_USER` dahil
            // negatif bir değer başlıkla talep edilememeli.
            .filter(|id| *id > 0),
    )
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
}

/// Sabit süreli karşılaştırma: erken çıkan bir eşitlik, yanıt süresinden
/// jetonun kaç karakteri tuttuğunu sızdırır ve jeton harf harf tahmin
/// edilebilir hale gelir.
fn constant_time_eq(expected: &str, got: &str) -> bool {
    let expected = expected.as_bytes();
    let got = got.as_bytes();
    if expected.len() != got.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(got) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn bearer_headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {value}").parse().unwrap(),
        );
        headers
    }

    #[derive(Default)]
    struct MemTokens {
        rows: Mutex<Vec<(String, i64)>>,
    }

    impl TokenStore for MemTokens {
        fn lookup(&self, token_hash: &str) -> BoxFuture<'static, Result<Option<i64>, SyncError>> {
            let found = self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|(h, _)| h == token_hash)
                .map(|(_, id)| *id);
            Box::pin(async move { Ok(found) })
        }
    }

    #[tokio::test]
    async fn paylasilan_jeton_tek_kullanici_kimligi_veriyor() {
        let auth = Auth::new("gizli");
        let id = auth.identify(&bearer_headers("gizli"), None).await;
        assert_eq!(id, Some(Identity { user_id: SHARED_USER }));
        assert!(id.unwrap().is_shared());
        assert_eq!(auth.identify(&HeaderMap::new(), Some("gizli")).await, id);
    }

    fn with_act_as(mut headers: HeaderMap, value: &str) -> HeaderMap {
        headers.insert(ACT_AS_HEADER, value.parse().unwrap());
        headers
    }

    /// Panelin kullanıcı adına konuşabilmesi.
    ///
    /// Bu olmadan panel `SHARED_USER` kimliğiyle gidiyor ve kullanıcıya ait
    /// odalara sahiplik denetimi kapıyı kapatıyordu: cihazlar çevrimdışı,
    /// transkript "sunucuya ulaşılamadı" görünüyordu.
    #[tokio::test]
    async fn paylasilan_jeton_kullanici_adina_konusabiliyor() {
        let auth = Auth::new("gizli");
        let headers = with_act_as(bearer_headers("gizli"), "42");
        assert_eq!(
            auth.identify(&headers, None).await,
            Some(Identity { user_id: 42 })
        );
    }

    /// İZİN YÜKSELTME testi: bu tutmazsa herhangi bir kullanıcı başka
    /// birinin bütün sohbetlerini okuyabilir.
    #[tokio::test]
    async fn uretilmis_jeton_baskasinin_adina_konusamiyor() {
        let tokens = Arc::new(MemTokens::default());
        tokens
            .rows
            .lock()
            .unwrap()
            .push((hash_token("kullanici-jetonu"), 7));
        let auth = Auth::new_with_store(Some("gizli".into()), tokens);

        let headers = with_act_as(bearer_headers("kullanici-jetonu"), "42");
        assert_eq!(
            auth.identify(&headers, None).await,
            Some(Identity { user_id: 7 }),
            "başlık yalnızca paylaşılan jetonla geçerli olmalı"
        );
    }

    #[tokio::test]
    async fn bozuk_act_as_basligi_reddediliyor() {
        let auth = Auth::new("gizli");
        for value in ["abc", "", "-1", "0"] {
            let headers = with_act_as(bearer_headers("gizli"), value);
            assert_eq!(
                auth.identify(&headers, None).await,
                None,
                "çözülemeyen başlık paylaşılan kimliğe DÜŞMEMELİ: {value:?}"
            );
        }
    }

    #[tokio::test]
    async fn yanlis_jeton_kimlik_vermiyor() {
        let auth = Auth::new("gizli");
        assert!(auth.identify(&bearer_headers("baska"), None).await.is_none());
        assert!(auth.identify(&HeaderMap::new(), None).await.is_none());
        // Uzunluk kontrolü olmasaydı kısa bir ön ek kabul edilebilirdi.
        assert!(auth.identify(&HeaderMap::new(), Some("giz")).await.is_none());
    }

    #[tokio::test]
    async fn panel_jetonu_kendi_kullanicisini_veriyor() {
        let store = Arc::new(MemTokens::default());
        store
            .rows
            .lock()
            .unwrap()
            .push((hash_token("panel-jetonu"), 42));

        let auth = Auth::new_with_store(None, store);
        assert_eq!(
            auth.identify(&HeaderMap::new(), Some("panel-jetonu")).await,
            Some(Identity { user_id: 42 })
        );
        assert!(auth.identify(&HeaderMap::new(), Some("baska")).await.is_none());
    }

    /// PAYLAŞILAN TEST VEKTÖRÜ — panelin `ApiToken.hash` testiyle aynı değer
    /// (`apps/panel/tests/functional/tokens.spec.ts`).
    ///
    /// İki taraf jetonu farklı özetlerse hiçbir jeton doğrulanmaz ve hata
    /// yalnızca çalışan sistemde, "jetonum kabul edilmiyor" olarak görünür.
    /// Sabit bir vektör bunu her iki tarafta da derleme zamanında yakalıyor.
    #[test]
    fn ozet_panelle_ayni() {
        assert_eq!(
            hash_token("postillion"),
            "0a32066d31ecf44c0a22ccd8a7c3f9422228893b38c52ac3587fef056d228495"
        );
    }

    /// Ham jeton ASLA saklanmamalı; depoda yalnızca özeti var.
    #[tokio::test]
    async fn depoda_ham_jeton_bulunmuyor() {
        let store = Arc::new(MemTokens::default());
        store.rows.lock().unwrap().push((hash_token("gizli-jeton"), 7));

        let stored = store.rows.lock().unwrap()[0].0.clone();
        assert_ne!(stored, "gizli-jeton");
        assert_eq!(stored.len(), 64, "sha-256 onaltılık gösterimi");
    }

    /// İki kip birlikte çalışabilmeli: geçiş sırasında ikisi de kullanımda.
    #[tokio::test]
    async fn iki_kip_birlikte_calisiyor() {
        let store = Arc::new(MemTokens::default());
        store.rows.lock().unwrap().push((hash_token("panelin"), 9));

        let auth = Auth::new_with_store(Some("paylasilan".into()), store);
        assert_eq!(
            auth.identify(&HeaderMap::new(), Some("paylasilan")).await,
            Some(Identity { user_id: SHARED_USER })
        );
        assert_eq!(
            auth.identify(&HeaderMap::new(), Some("panelin")).await,
            Some(Identity { user_id: 9 })
        );
    }
}
