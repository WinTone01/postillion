use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ev dizini bulunamadı")]
    NoHome,

    #[error("geçersiz hesap ismi: {0}")]
    InvalidName(String),

    #[error("hesap zaten var: {0}")]
    AccountExists(String),

    #[error("hesap bulunamadı: {0}")]
    AccountNotFound(String),

    #[error("'default' hesabı silinemez — paylaşılan verinin kaynağı o")]
    CannotDeleteDefault,

    #[error("{0} için kilit alınamadı; başka bir Claude süreci yazıyor olabilir")]
    LockBusy(String),

    #[error("ajan oturumu bulunamadı: {0}")]
    SessionNotFound(String),

    #[error("io hatası: {0}")]
    Io(#[from] std::io::Error),

    #[error("json hatası: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Tauri komutlarının hata tipi Serialize olmak zorunda; frontend'e düz
// mesaj olarak geçiyoruz.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
