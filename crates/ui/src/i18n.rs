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
    if let Ok(explicit) = std::env::var("POSTILLION_LANG")
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
        "Log in to Postillion" => "Postillion'a giriş yap",
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

        // ── süreç paneli
        "Processes" => "Süreçler",
        "Background processes" => "Arka plan süreçleri",
        "Nothing running under the agent right now." => "Şu an ajanın altında çalışan bir şey yok.",
        "Stop" => "Durdur",
        "Stopping…" => "Durduruluyor…",

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


        // ── ayarlar
        "Settings" => "Ayarlar",
        "Accounts" => "Hesaplar",
        "Agents" => "Ajanlar",
        "Appearance" => "Görünüm",
        "Theme" => "Tema",
        "Dark" => "Koyu",
        "Light" => "Açık",
        "System" => "Sistem",
        "Always dark, whatever the system is set to." => "Sistem ne olursa olsun daima koyu.",
        "Always light, whatever the system is set to." => "Sistem ne olursa olsun daima açık.",
        "Notifications" => "Bildirimler",
        "Desktop notifications" => "Masaüstü bildirimleri",
        "Sounds" => "Sesler",
        "Chime when a run finishes or an agent asks a question." => {
            "Koşu bittiğinde ya da ajan soru sorduğunda ses çal."
        }
        "Only when in the background" => "Yalnızca arka plandayken",
        "Keyboard shortcuts" => "Klavye kısayolları",
        "Shortcuts" => "Kısayollar",
        "Shortcuts must be unique." => "Kısayollar benzersiz olmalı.",
        "Press keys…" => "Tuşlara basın…",
        "Device name" => "Cihaz adı",
        "Rename device" => "Cihazı yeniden adlandır",
        "Archived" => "Arşivlenmiş",
        "Archived sessions" => "Arşivlenmiş oturumlar",
        "Right-click a session in the sidebar to archive it." => {
            "Arşivlemek için kenar çubuğunda bir oturuma sağ tıklayın."
        }
        "Unarchiving…" => "Arşivden çıkarılıyor…",

        // ── oturum / sohbet
        "New session" => "Yeni oturum",
        "Session title" => "Oturum başlığı",
        "Rename session" => "Oturumu yeniden adlandır",
        "Untitled session" => "Başlıksız oturum",
        "Delete session?" => "Oturum silinsin mi?",
        "Send a message to start a new session." => "Yeni oturum başlatmak için bir mesaj gönderin.",
        // Yer tutuculu: çağıran `{space}`'i proje adıyla değiştiriyor. Anahtar
        // kaynak metnin kendisi olduğu için biçimlendirme ÖNCESİ çevriliyor —
        // sonrasında olsaydı her proje adı ayrı bir anahtar olurdu.
        "Send a message to start a session in {space}." => {
            "{space} projesinde oturum başlatmak için bir mesaj gönderin."
        }
        "Open a blank session canvas to start a new session." => {
            "Yeni oturum için boş bir tuval açın."
        }
        "Do anything…" => "Ne isterseniz…",
        "Working" => "Çalışıyor",
        "Waiting on your input" => "Girdinizi bekliyor",
        "Queued" => "Sırada",
        "Done" => "Bitti",
        "Failed" => "Başarısız",
        "Complete" => "Tamamlandı",
        "Run finished" => "Koşu bitti",
        "Latest turn" => "Son tur",
        "No changes this turn" => "Bu turda değişiklik yok",
        "Loading…" => "Yükleniyor…",
        "Load more" => "Daha fazla yükle",
        "Reconnecting…" => "Yeniden bağlanılıyor…",
        "Type your own answer, or pick an option above" => {
            "Kendi cevabınızı yazın ya da yukarıdan seçin"
        }
        "This agent has no slash commands" => "Bu ajanın slash komutu yok",
        "Couldn't load this agent's commands" => "Bu ajanın komutları yüklenemedi",
        "The session's device is unreachable" => "Oturumun cihazına ulaşılamıyor",
        "Engine not connected" => "Motor bağlı değil",
        "Offline — sends are saved" => "Çevrimdışı — gönderiler saklanıyor",
        "Offline — messages will send when you're back online." => {
            "Çevrimdışı — çevrimiçi olunca mesajlar gönderilecek."
        }
        "Messages will send once the connection recovers." => {
            "Bağlantı düzelince mesajlar gönderilecek."
        }

        // ── proje
        "New session canvas" => "Yeni oturum tuvali",
        "Project name" => "Proje adı",
        "Rename project" => "Projeyi yeniden adlandır",
        "Remove project?" => "Proje kaldırılsın mı?",
        "No project" => "Proje yok",
        "No projects match." => "Eşleşen proje yok.",
        "No projects on this device." => "Bu cihazda proje yok.",
        "No folders here" => "Burada klasör yok",
        "No folders match" => "Eşleşen klasör yok",
        "Workspace name" => "Çalışma alanı adı",
        "Enter a workspace name" => "Bir çalışma alanı adı girin",

        // ── git
        "Branch" => "Dal",
        "Branch changes" => "Dal değişiklikleri",
        "No branch changes" => "Dal değişikliği yok",
        "No branches" => "Dal yok",
        "No matching branches" => "Eşleşen dal yok",
        "Remote branch" => "Uzak dal",
        "Working tree" => "Çalışma ağacı",
        "Worktree" => "Çalışma ağacı",
        "New worktree" => "Yeni çalışma ağacı",
        "Local checkout" => "Yerel çıkış",
        "Current checkout" => "Mevcut çıkış",
        "Current worktree" => "Mevcut çalışma ağacı",
        "No uncommitted changes" => "İşlenmemiş değişiklik yok",
        "Empty commit" => "Boş işleme",
        "New file" => "Yeni dosya",
        "Deleted file" => "Silinmiş dosya",
        "Binary file — contents not shown" => "İkili dosya — içerik gösterilmiyor",
        "Fetch all" => "Hepsini getir",
        "Fetching…" => "Getiriliyor…",
        "History" => "Geçmiş",
        "Merged" => "Birleştirildi",
        "Closed" => "Kapatıldı",
        "Comment" => "Yorum",
        "Request a change…" => "Değişiklik iste…",
        "Diff stream interrupted — retrying" => "Fark akışı kesildi — yeniden deneniyor",
        "Show or hide changes for the current session." => {
            "Bu oturumun değişikliklerini göster ya da gizle."
        }

        // ── arama
        "Search…" => "Ara…",
        "Search branches…" => "Dal ara…",
        "Search devices…" => "Cihaz ara…",
        "Search folders…" => "Klasör ara…",
        "Search models…" => "Model ara…",
        "Search projects…" => "Proje ara…",
        "Search refs…" => "Referans ara…",
        "No matching commands" => "Eşleşen komut yok",
        "No matching files" => "Eşleşen dosya yok",
        "No files available" => "Kullanılabilir dosya yok",
        "File search failed" => "Dosya araması başarısız",

        // ── model
        "No models found" => "Model bulunamadı",
        "No starred models yet — hit a row's star" => {
            "Henüz yıldızlı model yok — bir satırın yıldızına basın"
        }
        "Reasoning" => "Akıl yürütme",
        "Minimal" => "En az",
        "Medium" => "Orta",
        "High" => "Yüksek",
        "Anthropic's coding agent, driven through the Claude Code CLI." => {
            "Anthropic'in kodlama ajanı, Claude Code CLI üzerinden sürülüyor."
        }

        // ── hesap / eşitleme
        "Add Claude account" => "Claude hesabı ekle",
        "Use a different account" => "Farklı bir hesap kullan",
        "Unknown account" => "Bilinmeyen hesap",
        "Credentials unavailable" => "Kimlik bilgisi yok",
        "Usage unavailable" => "Kullanım bilgisi yok",
        "Verifying…" => "Doğrulanıyor…",
        "Login failed" => "Giriş başarısız",
        "Signing out…" => "Çıkış yapılıyor…",
        "Sign out?" => "Çıkış yapılsın mı?",
        "Paste the authorization code" => "Yetkilendirme kodunu yapıştırın",
        "Reopen the authorization page" => "Yetkilendirme sayfasını yeniden aç",
        "Reopen the sign-in page" => "Giriş sayfasını yeniden aç",
        "Press Escape to cancel." => "İptal için Escape'e basın.",
        "Sync is ready" => "Eşitleme hazır",
        "Sync needs a restart" => "Eşitleme yeniden başlatma istiyor",
        "Cancel sync setup" => "Eşitleme kurulumunu iptal et",
        "Canceling sync setup…" => "Eşitleme kurulumu iptal ediliyor…",
        "Switching to your synced workspace…" => "Eşitlenmiş çalışma alanınıza geçiliyor…",
        "Your synced workspace is ready." => "Eşitlenmiş çalışma alanınız hazır.",
        "You're all set" => "Her şey hazır",
        "Unknown device" => "Bilinmeyen cihaz",
        "Manage device names for this workspace." => {
            "Bu çalışma alanının cihaz adlarını yönetin."
        }

        // ── genel eylemler
        "Cancel" => "İptal",
        "Close" => "Kapat",
        "Create" => "Oluştur",
        "Creating…" => "Oluşturuluyor…",
        "Delete" => "Sil",
        "Remove" => "Kaldır",
        "Edit" => "Düzenle",
        "Open" => "Aç",
        "Copy" => "Kopyala",
        "Paste" => "Yapıştır",
        "Undo" => "Geri al",
        "Redo" => "Yinele",
        "Submit" => "Gönder",
        "Continue" => "Devam",
        "Next" => "İleri",
        "Later" => "Sonra",
        "Switch" => "Geç",
        "Switch now" => "Şimdi geç",
        "Switching…" => "Geçiliyor…",
        "Start fresh" => "Sıfırdan başla",
        "Navigate" => "Gezin",
        "Unknown" => "Bilinmiyor",
        "Local" => "Yerel",
        "Attach" => "Ekle",
        "Attached image" => "Eklenen görsel",
        "Couldn't stage the attachment locally." => "Ek yerelde hazırlanamadı.",
        "Couldn't upload the attachment — the device may be offline." => {
            "Ek yüklenemedi — cihaz çevrimdışı olabilir."
        }

        // ── panel / gezinme
        "Toggle left sidebar" => "Sol kenar çubuğunu aç/kapat",
        "Toggle right sidebar" => "Sağ kenar çubuğunu aç/kapat",
        "Toggle terminal" => "Terminali aç/kapat",
        "Show or hide sessions and settings navigation." => {
            "Oturum ve ayar gezinmesini göster ya da gizle."
        }
        "Show or hide the terminal for the current session." => {
            "Bu oturumun terminalini göster ya da gizle."
        }
        "Choose what to show in the right panel." => "Sağ panelde ne görüneceğini seçin.",
        "Update ready — restart to apply" => "Güncelleme hazır — uygulamak için yeniden başlatın",
        "Stopping engine…" => "Motor durduruluyor…",
        "Stop daemon and quit" => "Servisi durdur ve çık",
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
