//! Jeton doğrulama.
//!
//! Aşama 1 tek kullanıcılık: ortamdan gelen tek bir paylaşılan jeton. Aşama
//! 3'te yerini gerçek oturumlar alacak; o yüzden doğrulama buraya izole
//! edildi — değişmesi gereken tek yer burası olsun.
//!
//! Jeton İKİ yoldan gelebiliyor ve ikisini de kabul etmek zorundayız:
//! WebSocket `?token=` sorgusuyla (tarayıcı WS API'si başlık koymaya izin
//! vermediği için istemci böyle kurulmuş), HTTP uçları ise
//! `Authorization: Bearer` ile.

use axum::http::HeaderMap;

#[derive(Clone)]
pub struct Auth {
    token: String,
}

impl Auth {
    pub fn from_env() -> anyhow::Result<Self> {
        let token = std::env::var("POSTILLION_SERVER_TOKEN")
            .map_err(|_| anyhow::anyhow!("POSTILLION_SERVER_TOKEN gerekli"))?;
        // Boş bir jeton her isteği geçirirdi; yapılandırma hatasını sessizce
        // açık kapıya çevirmektense burada durmak doğru.
        if token.trim().is_empty() {
            anyhow::bail!("POSTILLION_SERVER_TOKEN boş olamaz");
        }
        Ok(Self { token })
    }

    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    /// `Authorization: Bearer …` ya da `?token=…` doğru mu.
    pub fn permits(&self, headers: &HeaderMap, query_token: Option<&str>) -> bool {
        if let Some(token) = query_token
            && self.matches(token)
        {
            return true;
        }
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|token| self.matches(token.trim()))
    }

    /// Sabit süreli karşılaştırma: erken çıkan bir eşitlik, yanıt süresinden
    /// jetonun kaç karakteri tuttuğunu sızdırır ve jeton harf harf tahmin
    /// edilebilir hale gelir.
    fn matches(&self, candidate: &str) -> bool {
        let expected = self.token.as_bytes();
        let got = candidate.as_bytes();
        if expected.len() != got.len() {
            return false;
        }
        let mut diff = 0u8;
        for (a, b) in expected.iter().zip(got) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {value}").parse().unwrap(),
        );
        headers
    }

    #[test]
    fn dogru_jeton_gecer() {
        let auth = Auth::new("gizli");
        assert!(auth.permits(&bearer("gizli"), None));
        assert!(auth.permits(&HeaderMap::new(), Some("gizli")));
    }

    #[test]
    fn yanlis_jeton_gecmez() {
        let auth = Auth::new("gizli");
        assert!(!auth.permits(&bearer("baska"), None));
        assert!(!auth.permits(&HeaderMap::new(), Some("baska")));
        assert!(!auth.permits(&HeaderMap::new(), None));
    }

    #[test]
    fn on_ek_olan_jeton_gecmez() {
        // Uzunluk kontrolü olmasaydı kısa bir ön ek kabul edilebilirdi.
        let auth = Auth::new("gizli-uzun-jeton");
        assert!(!auth.permits(&HeaderMap::new(), Some("gizli")));
    }

    #[test]
    fn bearer_olmayan_baslik_gecmez() {
        let auth = Auth::new("gizli");
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::AUTHORIZATION, "gizli".parse().unwrap());
        assert!(!auth.permits(&headers, None));
    }
}
