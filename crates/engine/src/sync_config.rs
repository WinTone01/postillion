//! Eşitleme sunucusunun adresi ve jetonu — kalıcı yapılandırma.
//!
//! Bunlar önceden yalnızca ortam değişkeninden okunuyordu. Bir masaüstü
//! uygulamasında bu, kullanıcının uygulamayı her seferinde doğru ortamdan
//! başlatmasını şart koşuyor: masaüstü kısayolundan açıldığında değişkenler
//! yok ve eşitleme sessizce kapalı kalıyor.
//!
//! Jeton `ui-settings.json`'a KOYULMUYOR. O dosya pencere genişliği türünden
//! tercihlerin yeri ve olağan izinlerle yazılıyor; bu jeton ise sunucudaki
//! bütün sohbetlere erişim demek. `session.json` ile aynı muameleyi görüyor:
//! ayrı dosya, 0600.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "sync.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SyncConfig {
    /// `https://sync.alanadiniz.com` — boş dize "eşitleme kapalı" demek.
    pub edge_url: String,
    /// Sunucunun `POSTILLION_SERVER_TOKEN` değeriyle aynı olmalı.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

impl SyncConfig {
    /// Diskten okur. Dosya yoksa ya da bozuksa boş yapılandırma.
    ///
    /// Bozuk bir dosyada hata vermek uygulamanın hiç açılmamasına yol açardı;
    /// eşitlemesiz açılıp ayarın panelden düzeltilebilmesi daha iyi.
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read(path_in(data_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::auth::write_private(&path_in(data_dir), &bytes)
    }

    /// Eşitlemenin açık sayılması için adres gerekli.
    pub fn is_configured(&self) -> bool {
        !self.edge_url.trim().is_empty()
    }
}

/// Ortam değişkeni mi kayıtlı dosya mı — ortam KAZANIYOR.
///
/// Sıra bilinçli: betikler, testler ve CI tek seferlik bir uç noktaya
/// yönlendirmek için değişkeni kullanıyor ve bunun kalıcı ayarı geçici olarak
/// geçersiz kılabilmesi gerekiyor. Tersi olsaydı, kaydedilmiş bir adres
/// betiğin verdiğini sessizce yok sayardı.
pub fn resolve(data_dir: &Path) -> SyncConfig {
    let stored = SyncConfig::load(data_dir);
    let env_url = std::env::var("POSTILLION_EDGE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let env_token = std::env::var("POSTILLION_EDGE_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty());

    SyncConfig {
        edge_url: env_url.unwrap_or(stored.edge_url),
        // Jeton ayrı çözülüyor: yalnızca adresi değişkenle geçersiz kılıp
        // jetonu kayıttan kullanmak geçerli bir kullanım (aynı hesap, farklı
        // uç nokta).
        token: env_token.or(stored.token),
    }
}

/// Jetonun istemci tarafında güvenle kullanılıp kullanılamayacağı.
///
/// Bu jeton yalnızca bir bearer değil: kimlik doğrulaması yokken kullanıcı
/// kimliği olarak da kullanılıyor ve veri dizini altında bir yol parçasına
/// dönüşüyor (`orgs/{org}/{user}/`). `/` o yolu böler, `@` ise kimliği
/// kırpar — kod `@`'den öncesini alıyor. İkisi de sessizce yanlış profile
/// yazılmakla sonuçlanır, o yüzden kaydetmeden ÖNCE reddediliyor.
pub fn token_problem(token: &str) -> Option<&'static str> {
    if token.trim().is_empty() {
        return Some("Token cannot be empty");
    }
    if token.contains('/') {
        return Some("Token cannot contain '/': it splits the profile path");
    }
    if token.contains('@') {
        return Some("Token cannot contain '@': it truncates the identity");
    }
    if token.chars().any(char::is_whitespace) {
        return Some("Token cannot contain whitespace");
    }
    None
}

/// Adresin kullanılabilir olup olmadığı.
pub fn url_problem(url: &str) -> Option<&'static str> {
    let url = url.trim();
    if url.is_empty() {
        return Some("Address cannot be empty");
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Some("Address must start with https://");
    }
    // Düz HTTP'de jeton ve bütün sohbet trafiği açık gidiyor. Yerel geliştirme
    // için gerekli olabildiğinden yasaklamıyoruz, ama uzak bir konakta bu
    // neredeyse her zaman hata.
    None
}

/// Bağlantı sınamasının sonucu.
///
/// Ham HTTP hatası yerine tiplenmiş: metni kullanıcıya gösteren katman
/// arayüz ve çeviriler orada. Ayrıca "engellendi" ile "ulaşılamadı" ayrımı
/// sorunun nerede aranacağını belirlediği için kaybolmamalı.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// Süre doldu — adres yanlış ya da konak erişilemiyor.
    Timeout,
    /// TCP/TLS kurulamadı.
    Connect,
    /// Araya giren bir vekil reddetti (ör. Cloudflare bot koruması). Sunucuya
    /// hiç ulaşılmadı, dolayısıyla sunucu ayarlarında aramak boşuna.
    Blocked,
    /// Sunucu cevapladı ama beklenmeyen bir durumla.
    Status(u16),
    Other(String),
}

/// Sınamanın süresi. Yanlış bir adreste bağlantı dakikalarca asılı
/// kalabiliyor; panelde bekleyen kullanıcı o kadar beklememeli.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// `{url}/health` çağırıp sunucunun cevap verdiğini doğrular.
///
/// Kaydetmeden önce çağrılabilmesi bilinçli: yanlış bir adresi kaydedip
/// uygulamayı yeniden başlattıktan sonra öğrenmek çok geç.
pub async fn probe(url: &str, token: Option<&str>) -> Result<(), ProbeError> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| ProbeError::Other(e.to_string()))?;

    let mut request = client.get(format!("{}/health", url.trim_end_matches('/')));
    // `/health` jeton istemiyor ama yine de gönderiliyor: vekil kurallarının
    // jetonlu isteklere farklı davrandığı kurulumlarda sınamanın gerçek
    // istekten farklı bir yol izlemesi, sınamayı işe yaramaz kılardı.
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        request = request.bearer_auth(token);
    }

    let response = request.send().await.map_err(|err| {
        if err.is_timeout() {
            ProbeError::Timeout
        } else if err.is_connect() {
            ProbeError::Connect
        } else {
            ProbeError::Other(err.to_string())
        }
    })?;

    match response.status().as_u16() {
        200 => Ok(()),
        403 => Err(ProbeError::Blocked),
        code => Err(ProbeError::Status(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kaydedilen_yapilandirma_geri_okunuyor() {
        let dir = tempfile::tempdir().unwrap();
        let config = SyncConfig {
            edge_url: "https://sync.example".into(),
            token: Some("abc123".into()),
        };
        config.save(dir.path()).unwrap();
        assert_eq!(SyncConfig::load(dir.path()), config);
    }

    #[test]
    fn jeton_dosyasi_sahibine_okunur() {
        // Bu dosya sunucudaki bütün sohbetlere erişim taşıyor; ortak bir
        // makinede olağan izinlerle yazılması onu diğer kullanıcılara açardı.
        let dir = tempfile::tempdir().unwrap();
        SyncConfig {
            edge_url: "https://sync.example".into(),
            token: Some("gizli".into()),
        }
        .save(dir.path())
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(FILE_NAME))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "sync.json yalnızca sahibine okunmalı");
        }
    }

    #[test]
    fn bozuk_dosya_uygulamayi_kirmiyor() {
        // Eşitlemesiz açılıp ayarın panelden düzeltilebilmesi, hiç açılmamaktan
        // iyi.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), b"{ bu json degil").unwrap();
        assert_eq!(SyncConfig::load(dir.path()), SyncConfig::default());
    }

    #[test]
    fn eksik_dosya_bos_yapilandirma() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!SyncConfig::load(dir.path()).is_configured());
    }

    #[test]
    fn yol_bozan_jetonlar_reddediliyor() {
        // `/` profil yolunu böler, `@` kimliği kırpar — ikisi de sessizce
        // yanlış profile yazmakla sonuçlanır.
        assert!(token_problem("a/b").is_some());
        assert!(token_problem("kullanici@org").is_some());
        assert!(token_problem("  ").is_some());
        assert!(token_problem("bosluk var").is_some());
        assert!(token_problem("a1b2c3d4e5").is_none());
    }

    /// Ortam değişkeni kayıtlı değeri geçersiz kılıyor mu.
    ///
    /// Tek testte toplandı: `set_var` süreç geneli ve testler paralel
    /// koştuğu için iki ayrı test birbirinin ortamını bozardı.
    #[test]
    fn ortam_kaydi_geciyor() {
        let dir = tempfile::tempdir().unwrap();
        SyncConfig {
            edge_url: "https://kayitli.example".into(),
            token: Some("kayitli-jeton".into()),
        }
        .save(dir.path())
        .unwrap();

        // SAFETY: süreç geneli ortam; bu test tek başına dokunuyor ve
        // sonunda temizliyor.
        unsafe {
            std::env::set_var("POSTILLION_EDGE_URL", "https://ortam.example");
            std::env::remove_var("POSTILLION_EDGE_TOKEN");
        }
        let resolved = resolve(dir.path());
        assert_eq!(resolved.edge_url, "https://ortam.example", "ortam kazanmalı");
        // Yalnızca adresi geçersiz kılıp jetonu kayıttan kullanmak geçerli:
        // aynı hesap, farklı uç nokta.
        assert_eq!(resolved.token.as_deref(), Some("kayitli-jeton"));

        unsafe {
            std::env::remove_var("POSTILLION_EDGE_URL");
        }
        assert_eq!(resolve(dir.path()).edge_url, "https://kayitli.example");
    }

    #[test]
    fn sema_gerektiriyor() {
        assert!(url_problem("sync.example").is_some());
        assert!(url_problem("").is_some());
        assert!(url_problem("https://sync.example").is_none());
        assert!(url_problem("http://127.0.0.1:8787").is_none());
    }
}
