/**
 * İngilizce çeviriler.
 *
 * Anahtar Türkçe kaynak metnin kendisi — gerekçe için `i18n.ts`. Eksik bir
 * anahtar arayüzü bozmuyor, o metin Türkçe kalıyor; `npm run check:i18n`
 * eksikleri ve artık kullanılmayanları listeliyor.
 */
export const EN: Record<string, string> = {
  // --------------------------------------------------------------- genel
  Ayarlar: "Settings",
  "Ayarları aç": "Open settings",
  Hesaplar: "Accounts",
  Oturumlar: "Sessions",
  Oturum: "Session",
  Hafta: "Week",
  "Hafta · Opus": "Week · Opus",
  Kapat: "Close",
  Ekle: "Add",
  Güncelle: "Update",
  Oluştur: "Create",
  Gönder: "Send",
  Durdur: "Stop",
  Zorla: "Force",
  Göster: "Show",
  Yenile: "Refresh",
  Listele: "List",
  Kur: "Install",
  Tamamla: "Finish",
  Başlat: "Start",
  Çal: "Play",
  Dene: "Test",
  Reddet: "Deny",
  Ses: "Sound",
  Bildirim: "Notification",
  Bildirimler: "Notifications",
  "Sonuç yok.": "No results.",
  bilinmiyor: "unknown",
  etkin: "active",
  gizli: "secret",
  kurulu: "installed",
  proje: "project",
  pid: "pid",
  sıfırlanma: "resets",
  ek: "attachment",
  çalışıyor: "running",
  Eylemler: "Actions",

  // ----------------------------------------------------------- kenar çubuğu
  "Aynı sohbet, istediğin hesapla": "One conversation, any account",
  "Henüz hesap yok. Aşağıdan ekleyin.": "No accounts yet. Add one below.",
  "Hesap ekle": "Add account",
  "Hesabı kaldır": "Remove account",
  "etkin hesap": "active account",
  "oturum yok — yeniden giriş gerekiyor": "not signed in — log in again",
  "Terminaldeki claude de artık bu hesabı kullanıyor.":
    "claude in your terminal now uses this account too.",
  "{name} hesabına geçildi": "Switched to {name}",
  "Bir sohbet sürüyor. Hesap değiştirmek paylaşılan kimlik dosyasını değiştirdiği için çalışan oturumu bozar; bitmesini bekleyin.":
    "A chat is running. Switching accounts rewrites the shared credentials file and would break it — wait for it to finish.",
  "Sekmeyi kapat": "Close tab",

  // --------------------------------------------------------------- maskot
  Boşta: "Idle",
  "Claude düşünüyor": "Claude is thinking",
  "Araç çalışıyor": "A tool is running",
  "İzin bekleniyor": "Waiting for permission",
  "Hata var": "Something failed",

  // ------------------------------------------------------------- kullanım
  "En son etkin olduğunda ölçüldü ({when}). Bu hesaba geçince güncellenir.":
    "Measured when this account was last active ({when}). Switching to it refreshes the reading.",
  "{when} ölçüldü": "measured {when}",
  "{when} yenilenir": "renews {when}",
  "hafta {when} yenilenir": "week renews {when}",
  birazdan: "any moment",
  "{n} dk sonra": "in {n}m",
  "{n} sa sonra": "in {n}h",
  "{n} gün sonra": "in {n}d",

  // ---------------------------------------------------------- oturum listesi
  "Oturum ara": "Search sessions",
  "Oturum ara — başlık, proje, dal": "Search sessions — title, project, branch",
  "Yeni oturum": "New session",
  "Yeni oturum başlat": "Start a new session",
  "yeni oturum": "new session",
  "Etkin hesap yok": "No active account",
  "Sol panelden giriş yapın — OAuth akışını Claude yürütür, token bu uygulamadan geçmez.":
    "Sign in from the sidebar — Claude runs the OAuth flow, no token passes through this app.",
  "Eşleşen oturum yok": "No matching sessions",
  "Henüz oturum yok": "No sessions yet",
  "Farklı bir arama deneyin.": "Try a different search.",
  "Oturumlar ~/.claude/projects altından okunur. Yeni bir tane başlatın.":
    "Sessions are read from ~/.claude/projects. Start one.",

  // ------------------------------------------------------------ komut paleti
  "Komut paleti": "Command palette",
  "Oturum, hesap ve eylem arayın": "Search sessions, accounts and actions",
  "Oturum ara ya da komut yazın…": "Search a session or type a command…",

  // --------------------------------------------------------- yeni oturum
  "Claude bu dizinde çalışacak — dosyaları burada arar ve oturum buraya kaydedilir.":
    "Claude will run in this directory — it looks for files here and the session is stored here.",
  "Çalışma dizini": "Working directory",
  "Çalışma dizini seçin": "Choose a working directory",
  "/home/kullanici/Projects/proje": "/home/user/Projects/project",
  "Son kullanılanlar": "Recent",
  "MCP sunucuları": "MCP servers",
  "{n}/{total}": "{n}/{total}",
  Hepsi: "All",
  Hiçbiri: "None",
  "Sohbet başladıktan sonra değiştirilemez.": "This cannot be changed once the chat starts.",
  "Hepsi açık — genel yapılandırma, eklentilerin getirdiği sunucular dahil.":
    "All enabled — your global configuration, including servers from plugins.",
  "Yalnızca seçilenler bu sohbette açık; eklenti sunucuları da kapanır.":
    "Only the selected ones run in this chat; plugin servers are dropped too.",

  // ------------------------------------------------------------ hesap ekleme
  "Tarayıcıda Anthropic hesabınıza giriş yapın, ardından verilen kodu buraya yapıştırın.":
    "Sign in to your Anthropic account in the browser, then paste the code you get here.",
  "Giriş bağlantısı hazırlanıyor…": "Preparing the sign-in link…",
  "1. Tarayıcıda açın": "1. Open in a browser",
  "Tarayıcı kendiliğinden açılmış olabilir. Açılmadıysa bu adresi kullanın.":
    "Your browser may have opened already. If not, use this address.",
  "2. Kodu yapıştırın": "2. Paste the code",
  "giriş sonrası verilen kod": "the code shown after signing in",

  // --------------------------------------------------------------- sohbet
  "Geçmiş yükleniyor…": "Loading history…",
  Hazır: "Ready",
  "Başlatılıyor…": "Starting…",
  "Önceki oturum geri yükleniyor.": "Restoring the previous session.",
  "Bir şey yazarak başlayın.": "Type something to begin.",
  "Claude'a yazın — komutlar için / yazın": "Message Claude — type / for commands",
  "Oturum başlatılıyor…": "Starting the session…",
  Model: "Model",
  Efor: "Effort",
  Mod: "Mode",
  "{n} MCP": "{n} MCP",
  "{n}/{total} MCP": "{n}/{total} MCP",
  "Yönetmek için tıklayın.": "Click to manage.",
  "Sunucu kümesi ancak oturum yeniden kurularak değişir. Sohbet kaybolmuyor — aynı transcript kaldığı yerden devam ediyor.":
    "The server set only changes by restarting the session. The conversation survives — the same transcript picks up where it left off.",
  "Genel yapılandırma kullanılıyor.": "Using your global configuration.",
  "{n} sunucu seçili.": "{n} server(s) selected.",
  "Soluk olanlar eklentilerden geliyor. Tek tek kapatılamıyorlar; bir seçim yaptığınızda hepsi birden kapanır.":
    "The dimmed ones come from plugins. They cannot be toggled individually — making any selection drops all of them.",
  Vazgeç: "Cancel",
  "Uygula ve yeniden başlat": "Apply and restart",
  "Tur bitmesini bekleyin": "Wait for the turn to finish",
  bağlı: "connected",
  bağlanıyor: "connecting",
  "yetki gerekiyor": "needs auth",
  bağlanamadı: "failed",
  "Görüntü ekle": "Attach an image",
  "Ekran görüntüsü al": "Take a screenshot",
  "Eki kaldır": "Remove attachment",
  "{n} ek": "{n} attached",
  "İliştirilen görüntü": "Attached image",
  "{n} ek okunamadı ve gönderilmedi.": "{n} attachment(s) could not be read and were not sent.",

  // ------------------------------------------------------------------ izin
  "çalıştırılsın mı?": "— run it?",
  "İzin ver": "Allow",
  "İzin verildi.": "Allowed.",
  "Reddedildi.": "Denied.",
  "{tool} aracına hep izin ver": "Always allow {tool}",
  "Bir izin bekliyor": "One approval waiting",
  "{n} izin bekliyor": "{n} approvals waiting",
  "{n} isteğe izin ver": "Allow {n} requests",
  "Düzenlemeleri hep onayla": "Always accept edits",
  "İzin sormayı kapat": "Stop asking for permission",
  "Plan moduna geç": "Switch to plan mode",
  "Varsayılan moda dön": "Back to the default mode",

  // ------------------------------------------------------------- izin modları
  "Her şeyi sor": "Ask for everything",
  "Düzenlemeleri onayla": "Accept edits",
  Plan: "Plan",
  Otomatik: "Automatic",
  Sorma: "Don't ask",
  İzinsiz: "No permissions",

  // ---------------------------------------------------------------- sorular
  Cevaplandı: "Answered",
  "Bir sorusu var": "Claude has a question",
  "{n} soru": "{n} questions",
  "Birden fazla seçebilirsiniz": "You can pick more than one",
  "Başka…": "Something else…",
  "Kendi cevabınızı yazın": "Write your own answer",

  // ---------------------------------------------------------------- süreçler
  Süreçler: "Processes",
  "Şu anda alt süreç yok.": "No child processes right now.",
  "Oturum kapalı.": "The session is closed.",
  "{n} sn": "{n}s",
  "{n} dk": "{n}m",
  "{h} sa {m} dk": "{h}h {m}m",

  // ------------------------------------------------------------------- diff
  "Bu yazım tamamlandığı için dosyanın önceki hâli geri getirilemiyor; tüm satırlar yeni olarak gösteriliyor.":
    "This write already completed, so the file's previous contents cannot be recovered; every line is shown as new.",
  "Değişiklik çok büyük; satır eşleştirme atlandı, bloklar olduğu gibi gösteriliyor.":
    "The change is too large; line matching was skipped and the blocks are shown as they are.",

  // ------------------------------------------------------------------ uyarı
  "İzin isteği": "Permission request",
  "Claude bir araç çalıştırmak için onay beklediğinde":
    "When Claude waits for approval to run a tool",
  Soru: "Question",
  "Claude size bir soru sorduğunda": "When Claude asks you a question",
  Tamamlandı: "Finished",
  "Bir tur bittiğinde ve Claude beklemeye geçtiğinde":
    "When a turn ends and Claude goes idle",
  Hata: "Error",
  "Oturumda bir hata oluştuğunda": "When the session hits an error",
  "Claude size bir soru sordu.": "Claude asked you a question.",
  "{tool} çalıştırmak için onay bekliyor.": "Waiting for approval to run {tool}.",
  "Claude işini bitirdi.": "Claude is done.",
  "Oturumda bir hata oluştu.": "The session hit an error.",

  // ----------------------------------------------------------------- ayarlar
  "Ayarlar tüm hesaplar için ortak — tek bir yapılandırma var.":
    "Settings are shared by every account — there is only one configuration.",
  "Model & Efor": "Model & effort",
  "Varsayılan model ve düşünme derinliği": "Default model and thinking depth",
  "Hangi olayda bildirim ve ses": "Which events notify and make a sound",
  MCP: "MCP",
  "Sunucular ve erişim anahtarları": "Servers and access keys",
  Eklentiler: "Plugins",
  "Marketplace ve kurulu eklentiler": "Marketplaces and installed plugins",
  "Skill'ler": "Skills",
  "Sohbette /isim ile çağrılır": "Invoked with /name in a chat",

  "Varsayılan model": "Default model",
  "En yetenekli; karmaşık işler için": "Most capable; for demanding work",
  "Dengeli hız ve yetenek": "Balanced speed and capability",
  "En hızlı; basit işler için": "Fastest; for simple work",
  "Model seçin": "Choose a model",
  "Yeni oturumlarda kullanılır. Açık bir sohbetin modelini başlıktaki seçiciden anında değiştirebilirsiniz.":
    "Used for new sessions. You can change an open chat's model instantly from the picker in its header.",
  "Efor seviyesi": "Effort level",
  "Seviye seçin": "Choose a level",
  "Yeni oturumların başlangıç değeri. Süren bir sohbette başlıktaki efor seçicisi /effort komutunu gönderir.":
    "The starting value for new sessions. In a running chat the effort picker sends the /effort command.",
  "Düşük — hızlı ve ucuz": "Low — fast and cheap",
  "Orta — varsayılan": "Medium — the default",
  Yüksek: "High",
  "Çok yüksek": "Very high",
  "Azami — en yavaş, en derin": "Max — slowest and deepest",

  Genel: "General",
  "Arayüz dili": "Interface language",
  Dil: "Language",
  "Sistem dili": "System language",
  "Varsayılan olarak sistem dilinizi izler. Türkçe dışındaki diller İngilizce'ye düşer.":
    "Follows your system language by default. Anything other than Turkish falls back to English.",

  "Ses düzeyi": "Volume",
  "Ses örneği": "Sound sample",
  "Olay başına uyarılar": "Alerts per event",
  "{label} örneği": "{label} sample",

  "Sunucu ekle": "Add a server",
  "sunucu-adı": "server-name",
  "npx my-mcp-server --flag": "npx my-mcp-server --flag",
  "Erişim anahtarı": "Access key",
  "(isteğe bağlı)": "(optional)",
  "Anahtar doğrudan claude mcp add'e geçer. Bu uygulama onu saklamaz ve listede bir daha göstermez — yalnızca alan adını görürsünüz.":
    "The key is passed straight to claude mcp add. This app does not store it and never shows it again — you only see the field name.",
  "Ek ortam değişkenleri": "More environment variables",
  "Ek başlıklar": "More headers",
  "Yapılandırılmış sunucular ({n})": "Configured servers ({n})",
  "Henüz MCP sunucusu yok.": "No MCP servers yet.",
  "Sunucuyu sil": "Delete server",

  "Marketplace'ler ({n})": "Marketplaces ({n})",
  "kullanıcı/depo, URL ya da yerel yol": "user/repo, a URL or a local path",
  "Kayıtlı marketplace yok.": "No marketplaces registered.",
  "Marketplace'i kaldır": "Remove marketplace",
  "Kurulu eklentiler ({n})": "Installed plugins ({n})",
  "Kurulu eklenti yok.": "No plugins installed.",
  "Devre dışı bırak": "Disable",
  Etkinleştir: "Enable",
  "Eklentiyi kaldır": "Remove plugin",
  "Marketplace'ten kur": "Install from a marketplace",
  "Kurulabilir eklentileri görmek için “Listele”ye basın.":
    "Press “List” to see the plugins you can install.",
  "Eklenti ara…": "Search plugins…",
  "En çok kurulan 20 eklenti gösteriliyor — aramayla daraltın.":
    "Showing the 20 most installed plugins — search to narrow it down.",
  "Eşleşen eklenti yok.": "No matching plugins.",
  "{n} eklenti": "{n} plugins",

  "Yeni skill oluştur": "Create a skill",
  "skill-adi": "skill-name",
  "Ne zaman kullanılacağını anlatan kısa açıklama":
    "A short description of when to use it",
  "{path} altında iskelet oluşturulur ve bir sonraki oturumda yüklenir.":
    "A skeleton is created under {path} and loaded in your next session.",
  "Kendi skill'leriniz ({n})": "Your skills ({n})",
  "Henüz kendi skill'iniz yok.": "You have no skills of your own yet.",
  "Skill'i sil": "Delete skill",
  "Eklentilerden gelenler ({n})": "From plugins ({n})",
  "Eklenti skill'i yok.": "No plugin skills.",

  // ------------------------------------------------------------- hata metni
  "giriş tamamlanamadı": "sign-in did not complete",
  "tercihler okunamadı": "could not read preferences",
  "tercih kaydedilemedi": "could not save the preference",
  "MCP sunucuları okunamadı": "could not read MCP servers",
  "MCP sunucusu eklenemedi": "could not add the MCP server",
  "MCP sunucusu silinemedi": "could not remove the MCP server",
  "eklentiler okunamadı": "could not read plugins",
  "eklenti kurulamadı": "could not install the plugin",
  "eklenti kaldırılamadı": "could not remove the plugin",
  "eklenti durumu değiştirilemedi": "could not change the plugin state",
  "marketplace eklenemedi": "could not add the marketplace",
  "marketplace silinemedi": "could not remove the marketplace",
  "marketplace güncellenemedi": "could not update the marketplace",
  "skill'ler okunamadı": "could not read skills",
  "skill oluşturulamadı": "could not create the skill",
  "skill silinemedi": "could not delete the skill",

  // ------------------------------------------------------------------- bağlam
  "{n}k bağlam": "{n}k context",
  "sıkıştırılıyor…": "compacting…",
  "Her tur bu bağlamın tamamını yeniden okuyor.":
    "Every turn re-reads all of this context.",
  "{n}k aşılınca kendiliğinden sıkışıyor. Şimdi sıkıştırmak için tıklayın.":
    "Compacts on its own past {n}k. Click to compact now.",

  // -------------------------------------------------------------- göreli zaman
  "az önce": "just now",
  "{n} dk önce": "{n}m ago",
  "{n} sa önce": "{n}h ago",
  "{n} gün önce": "{n}d ago",
};
