//! Arayüz dili.
//!
//! Kaynak dizeler İngilizce ve **anahtarın kendisi** — gettext'in yaptığı gibi.
//! Böylece çeviri eklenmemiş bir dize sessizce kaybolmuyor, İngilizce olarak
//! görünmeye devam ediyor; bir katalog boşluğu en kötü ihtimalle çevrilmemiş
//! metin demek, boş bir düğme değil.
//!
//! Dil sistem yerelinden bir kez okunuyor: `tr` ise Türkçe, değilse kaynak
//! metin. Çalışma anında değiştirilebilir bir ayar değil — süreç boyunca sabit
//! olması, çözülmüş metni önbelleğe alan her yerin (markdown run cache gibi)
//! geçersizleşme derdinden kurtulması demek.

use std::sync::OnceLock;

/// Uygulamanın konuştuğu dil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Turkish,
}

/// Yerel ortam değişkenlerinden dil.
///
/// `LC_ALL` > `LC_MESSAGES` > `LANG` — POSIX önceliği. `tr_TR.UTF-8`, `tr` ve
/// `tr_CY` hepsi Türkçe; tanınmayan her şey İngilizce'ye düşüyor.
fn detect() -> Language {
    // Açık tercih her şeyin önünde. Karışık yerel kurulumlar yaygın: KDE'de
    // arayüz dilini İngilizce bırakıp bölgeyi Türkiye yapmak `LANG=en_US` ama
    // `LC_TIME=tr_TR` üretiyor. POSIX'e göre mesaj dili İngilizce'dir ve
    // otomatik algılama bunu doğru yapar — ama kullanıcı yalnızca BU
    // uygulamayı Türkçe isteyebilir ve bunun için sistem yerelini
    // değiştirmek zorunda kalmamalı.
    if let Ok(explicit) = std::env::var("ZERON_LANG")
        && !explicit.is_empty()
    {
        return from_locale(&explicit);
    }

    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        return from_locale(&value);
    }
    Language::English
}

fn from_locale(locale: &str) -> Language {
    // `tr_TR.UTF-8` → `tr`. Yalnızca dil kısmı önemli.
    let tag = locale
        .split(['.', '@'])
        .next()
        .unwrap_or(locale)
        .split(['_', '-'])
        .next()
        .unwrap_or(locale);

    if tag.eq_ignore_ascii_case("tr") {
        Language::Turkish
    } else {
        Language::English
    }
}

fn language() -> Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    *LANGUAGE.get_or_init(detect)
}

/// Kaynak dizeyi kullanıcının diline çevirir.
///
/// Karşılığı olmayan dize olduğu gibi dönüyor.
pub fn t(source: &str) -> &str {
    match language() {
        Language::English => source,
        Language::Turkish => turkish(source).unwrap_or(source),
    }
}

/// İngilizce kaynak → Türkçe.
///
/// `match` kullanılıyor: derleyici bunu bir arama tablosuna çeviriyor ve
/// çalışma anında hiçbir tahsis yapılmıyor — bu fonksiyon her karede, her
/// etiket için çağrılıyor.
fn turkish(source: &str) -> Option<&'static str> {
    Some(match source {
        // ── kimlik / hesap
        "Add account" => "Hesap ekle",
        "Log in" => "Giriş yap",
        "Log in to Zeron" => "Zeron'a giriş yap",
        "Not signed in" => "Giriş yapılmadı",
        "Sign out" => "Çıkış yap",
        "Signed out" => "Çıkış yapıldı",
        "Waiting for the browser…" => "Tarayıcı bekleniyor…",
        "Adding…" => "Ekleniyor…",

        // ── proje / çalışma alanı
        "Add a project to get started" => "Başlamak için bir proje ekleyin",
        "All projects" => "Tüm projeler",
        "New project…" => "Yeni proje…",
        "No project selected" => "Proje seçilmedi",
        "No repository selected" => "Depo seçilmedi",
        "Create your workspace" => "Çalışma alanınızı oluşturun",
        "Add a project" => "Proje ekle",
        "A project is a folder on one of your devices." => {
            "Proje, cihazlarınızdan birindeki bir klasördür."
        }
        "Local only" => "Yalnızca yerel",
        "Sync ready after restart" => "Eşitleme yeniden başlatınca hazır",
        "Stored on this device" => "Bu cihazda saklanıyor",
        "Home" => "Ana dizin",
        "Locations" => "Konumlar",

        // ── oturum / sohbet
        "No sessions yet" => "Henüz oturum yok",
        "Nothing archived" => "Arşivde bir şey yok",
        "Loading history…" => "Geçmiş yükleniyor…",
        "Sending…" => "Gönderiliyor…",
        "Not delivered — click to retry" => "İletilmedi — yeniden denemek için tıklayın",
        "Run failed" => "Koşu başarısız",
        "No turn recorded yet — send a message first" => {
            "Henüz kayıtlı tur yok — önce bir mesaj gönderin"
        }
        "Scroll to bottom" => "En alta in",
        "Drop images to attach" => "Eklemek için görselleri bırakın",
        "Question" => "Soru",
        "Archive" => "Arşivle",
        "Unarchive" => "Arşivden çıkar",
        "Rename" => "Yeniden adlandır",
        "Rename…" => "Yeniden adlandır…",
        "Delete…" => "Sil…",
        "Remove…" => "Kaldır…",

        // ── git
        "Select ref" => "Referans seç",
        "No ref" => "Referans yok",
        "No refs found." => "Referans bulunamadı.",
        "No commits found" => "İşleme bulunamadı",
        "Commit" => "İşleme",
        "Author" => "Yazar",
        "Date" => "Tarih",
        "Git" => "Git",
        "Add pull request badges" => "Pull request rozetleri ekle",
        "Preparing diff…" => "Fark hazırlanıyor…",
        "Partial snapshot" => "Kısmi anlık görüntü",
        "BIN" => "İKİLİ",
        "SHA" => "SHA",

        // ── MCP / bağlam
        "MCP: all" => "MCP: hepsi",
        "MCP: none" => "MCP: hiçbiri",
        "No MCP servers configured." => "Tanımlı MCP sunucusu yok.",
        "Each connected server's tool definitions ride every turn." => {
            "Bağlı her sunucunun araç tanımları her turda taşınıyor."
        }
        "Context window" => "Bağlam penceresi",
        "Context Window" => "Bağlam Penceresi",

        // ── model / özellikler
        "Traits" => "Özellikler",
        "Default" => "Varsayılan",
        "Normal" => "Normal",
        "Fast" => "Hızlı",
        "Flagship" => "Amiral gemisi",
        "Speed" => "Hız",
        "Standard" => "Standart",
        "Opus" => "Opus",
        "Select one or more options." => "Bir ya da daha fazla seçenek seçin.",
        "Restore defaults" => "Varsayılanlara dön",
        "Reset" => "Sıfırla",

        // ── cihaz / eşitleme
        "Devices" => "Cihazlar",
        "This device" => "Bu cihaz",
        "No devices registered" => "Kayıtlı cihaz yok",
        "No devices match." => "Eşleşen cihaz yok.",
        "Enable sync" => "Eşitlemeyi aç",
        "Finish sync setup" => "Eşitleme kurulumunu bitir",
        "Sync setup in progress" => "Eşitleme kurulumu sürüyor",

        // ── terminal
        "Terminal" => "Terminal",
        "Select a chat to open a terminal" => "Terminal açmak için bir sohbet seçin",
        "Open a surface" => "Bir yüzey aç",

        // ── genel
        "Settings" => "Ayarlar",
        "Back" => "Geri",
        "Retry" => "Yeniden dene",
        "Click to retry" => "Yeniden denemek için tıklayın",
        "Refresh" => "Yenile",
        "Error" => "Hata",
        "Copied" => "Kopyalandı",
        "Enter" => "Enter",
        "esc" => "esc",
        "switching…" => "geçiliyor…",
        "You" => "Siz",

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yerel_etiketi_dile_cevriliyor() {
        // Ülke, kodlama ve değiştirici ekleri dilin kendisini gölgelememeli.
        assert_eq!(from_locale("tr"), Language::Turkish);
        assert_eq!(from_locale("tr_TR"), Language::Turkish);
        assert_eq!(from_locale("tr_TR.UTF-8"), Language::Turkish);
        assert_eq!(from_locale("tr_CY.UTF-8@euro"), Language::Turkish);
        assert_eq!(from_locale("TR"), Language::Turkish);

        // Tanınmayan her şey kaynağa düşüyor.
        assert_eq!(from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(from_locale("de_DE"), Language::English);
        assert_eq!(from_locale("C"), Language::English);
        assert_eq!(from_locale(""), Language::English);
        // `tr` ile başlayan başka bir dil Türkçe sanılmamalı.
        assert_eq!(from_locale("trv"), Language::English);
    }

    #[test]
    fn cevirisi_olmayan_dize_kaynak_olarak_kaliyor() {
        // Katalog boşluğu boş bir düğme değil, çevrilmemiş metin üretmeli.
        assert!(turkish("Bu dizenin karşılığı yok").is_none());
        assert_eq!(turkish("Settings"), Some("Ayarlar"));
    }

    #[test]
    fn katalogda_bos_karsilik_yok() {
        // Boş bir çeviri arayüzde kaybolan bir etiket demek.
        for source in [
            "Add account",
            "Settings",
            "MCP: all",
            "No MCP servers configured.",
            "Retry",
        ] {
            let translated = turkish(source).expect("karşılık bekleniyordu");
            assert!(!translated.trim().is_empty(), "boş çeviri: {source}");
        }
    }
}
