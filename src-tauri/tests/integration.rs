//! Gerçek `~/.claude` verisine karşı uçtan uca doğrulama.
//!
//! Hesaplar geçici bir `POSTILLION_ROOT` altında oluşturulur, ama paylaşılan
//! öğeler gerçek `~/.claude`'a symlink'lenir. Testin asıl amacı silmenin
//! paylaşılan veriye dokunmadığını kanıtlamak.
//!
//! Tek bir test fonksiyonu: POSTILLION_ROOT süreç geneli bir env var, paralel
//! testler birbirinin ayağını kaydırırdı.

use std::fs;
use std::path::PathBuf;

#[test]
fn hesap_yasam_dongusu_paylasilan_veriyi_korur() {
    let home = dirs::home_dir().expect("ev dizini");
    let real_projects = home.join(".claude").join("projects");

    if !real_projects.is_dir() {
        eprintln!("~/.claude/projects yok, test atlanıyor");
        return;
    }

    // Silmenin gerçek veriye dokunmadığını kanıtlamak için önce sayıyoruz.
    let before = fs::read_dir(&real_projects).unwrap().count();
    assert!(before > 0, "test anlamlı olsun diye transcript bekleniyor");

    let root = std::env::temp_dir().join(format!("postillion-it-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("POSTILLION_ROOT", &root);

    // --- oluşturma -------------------------------------------------------
    let account = postillion_lib::testing::create_account("deneme").expect("hesap oluşturulmalı");

    assert_eq!(account.name, "deneme");
    assert!(!account.is_default);
    assert!(!account.logged_in, "yeni hesap kimliksiz doğmalı");
    assert!(
        account.broken_links.is_empty(),
        "symlink'ler kurulmalıydı: {:?}",
        account.broken_links
    );

    let dir: PathBuf = root.join("deneme");

    // --- symlink'ler doğru hedefi gösteriyor mu --------------------------
    let link = dir.join("projects");
    assert!(
        fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
        "projects bir symlink olmalı"
    );
    assert_eq!(
        fs::read_link(&link).unwrap(),
        real_projects,
        "projects gerçek dizini göstermeli"
    );
    // Hedefin gerçekten okunabildiğini de doğrula (kırık link değil).
    assert_eq!(fs::read_dir(&link).unwrap().count(), before);

    // --- tohumlama: proje onayları geldi mi, kimlik geldi mi -------------
    let seeded: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join(".claude.json")).unwrap()).unwrap();

    assert!(
        seeded.get("oauthAccount").is_none(),
        "kimlik yeni hesaba taşınmamalı"
    );
    assert!(seeded.get("userID").is_none(), "userID taşınmamalı");
    assert!(seeded.get("machineID").is_none(), "machineID taşınmamalı");

    // Asıl kazanç: trust dialog'ları ve MCP ayarları korunmalı.
    let source: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    if source.get("projects").is_some() {
        assert!(
            seeded.get("projects").is_some(),
            "proje onayları tohumlanmalıydı"
        );
    }

    // Dosya izni 0600 olmalı — içinde proje ayarları var.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.join(".claude.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "config 0600 olmalı");
    }

    // --- aynı isim ikinci kez oluşturulamaz ------------------------------
    assert!(postillion_lib::testing::create_account("deneme").is_err());

    // --- listeleme: default + yeni hesap ---------------------------------
    let listed = postillion_lib::testing::list_accounts().unwrap();
    assert!(listed.iter().any(|a| a.name == "default" && a.is_default));
    assert!(listed.iter().any(|a| a.name == "deneme"));

    // --- silme: hesap gider, paylaşılan veri kalır -----------------------
    postillion_lib::testing::delete_account("deneme").expect("silinmeli");
    assert!(!dir.exists(), "hesap dizini gitmeliydi");

    let after = fs::read_dir(&real_projects).unwrap().count();
    assert_eq!(
        before, after,
        "SİLME PAYLAŞILAN TRANSCRIPT'LERE DOKUNDU — {before} -> {after}"
    );

    // default hesap asla silinemez
    assert!(postillion_lib::testing::delete_account("default").is_err());

    let _ = fs::remove_dir_all(&root);
}

/// Regresyon: default hesabın `.claude.json`'ı `~/.claude/` içinde değil,
/// ev kökünde (`~/.claude.json`). Yanlış yerden okununca hesap "giriş
/// yapılmamış" görünüyordu.
#[test]
fn default_hesap_kimligi_ev_kokunden_okunur() {
    let home = dirs::home_dir().expect("ev dizini");

    let config = home.join(".claude.json");
    let creds = home.join(".claude").join(".credentials.json");

    if !config.exists() || !creds.exists() {
        eprintln!("bu makinede default hesap kurulu değil, test atlanıyor");
        return;
    }

    let source: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    if source.get("oauthAccount").is_none() {
        eprintln!("oauthAccount yok, test atlanıyor");
        return;
    }

    // `~/.claude/.claude.json` OLMAMALI.
    //
    // Bu dosya ancak biri `CLAUDE_CONFIG_DIR=~/.claude` vererek Claude'u
    // çalıştırdığında oluşur ve o an Claude gerçek `~/.claude.json` yerine
    // bomboş bir gölge config kullanmaya başlar (proje onayları yok, trust
    // dialog'u yeniden çıkar). Default hesap için o değişken hiç set
    // edilmemeli — bu assert tam olarak o regresyonu yakalar.
    assert!(
        !home.join(".claude").join(".claude.json").exists(),
        "gölge config oluşmuş: default hesap için CLAUDE_CONFIG_DIR set edilmiş olmalı"
    );

    let accounts = postillion_lib::testing::list_accounts().unwrap();
    let default = accounts
        .iter()
        .find(|a| a.is_default)
        .expect("default hesap listelenmeli");

    assert!(
        default.logged_in,
        "default hesap giriş yapılmış görünmeliydi"
    );
    assert!(
        default.email.is_some(),
        "default hesabın e-postası okunmalıydı"
    );
}

/// `claude --resume` geçmişi stdout'a basmıyor (boş stdin ile sıfır satır
/// çıktı ölçüldü), bu yüzden sohbet geçmişi diskten okunuyor. Bu test o
/// okumanın gerçek transcript'lerde işe yaradığını doğrular.
#[test]
fn gecmis_diskten_okunabiliyor() {
    let sessions = postillion_lib::testing::scan_sessions().expect("tarama");
    if sessions.is_empty() {
        eprintln!("oturum yok, test atlanıyor");
        return;
    }

    // En büyük oturum en zorlu vaka: hem kullanıcı hem asistan hem araç kaydı.
    let biggest = sessions
        .iter()
        .max_by_key(|s| s.size_bytes)
        .expect("en az bir oturum");

    let records = postillion_lib::testing::read_transcript(&biggest.path, 600).expect("okuma");

    assert!(
        !records.is_empty(),
        "geçmiş boş döndü — arayüzde sohbet görünmezdi"
    );

    let mut users = 0;
    let mut assistants = 0;

    for record in &records {
        let kind = record.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "user" => users += 1,
            "assistant" => assistants += 1,
            other => panic!("sohbet dışı kayıt sızmış: {other}"),
        }

        // Alt-ajan yan dalları ana sohbete karışmamalı.
        assert_ne!(
            record.get("isSidechain").and_then(|v| v.as_bool()),
            Some(true),
            "alt-ajan kaydı sızmış"
        );

        // Sisteme enjekte edilmiş kayıtlar da öyle; kullanıcı onları yazmadı.
        assert_ne!(
            record.get("isMeta").and_then(|v| v.as_bool()),
            Some(true),
            "isMeta kaydı sızmış"
        );

        // Reducer `message` alanına bakıyor; yoksa mesaj sessizce kaybolur.
        assert!(record.get("message").is_some(), "message alanı eksik");
    }

    assert!(users > 0, "hiç kullanıcı mesajı yok");
    assert!(assistants > 0, "hiç asistan mesajı yok");

    // Arayüzün gerçekten metin gösterebilmesi için en az bir düz metin
    // kullanıcı mesajı bulunmalı (reducer bunu böyle tanıyor).
    let plain_text_user = records.iter().any(|r| {
        r.get("type").and_then(|t| t.as_str()) == Some("user")
            && r.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .is_some_and(|s| !s.trim().is_empty())
    });
    assert!(
        plain_text_user,
        "düz metin kullanıcı mesajı yok — geçmiş boş görünürdü"
    );

    eprintln!(
        "{} kayıt ({} kullanıcı, {} asistan) — {}",
        records.len(),
        users,
        assistants,
        biggest.title.as_deref().unwrap_or("(başlıksız)")
    );
}

#[test]
fn gercek_transcriptler_taranabiliyor() {
    let home = dirs::home_dir().expect("ev dizini");
    if !home.join(".claude").join("projects").is_dir() {
        eprintln!("~/.claude/projects yok, test atlanıyor");
        return;
    }

    let start = std::time::Instant::now();
    let sessions = postillion_lib::testing::scan_sessions().expect("tarama başarılı olmalı");
    let elapsed = start.elapsed();

    eprintln!("{} oturum, {:?}", sessions.len(), elapsed);

    assert!(!sessions.is_empty(), "en az bir oturum bulunmalıydı");

    // Sıralama: en yeni ilk.
    for pair in sessions.windows(2) {
        assert!(
            pair[0].modified_ms >= pair[1].modified_ms,
            "oturumlar tarihe göre sıralı olmalı"
        );
    }

    // 412 MB'ı baş+son örneklemesiyle okuyoruz; makul sürede bitmeli.
    assert!(
        elapsed.as_secs() < 20,
        "tarama çok yavaş: {elapsed:?}"
    );

    let with_title = sessions.iter().filter(|s| s.title.is_some()).count();
    assert!(
        with_title > 0,
        "hiçbir oturumdan başlık çıkarılamadı — ayrıştırma bozuk"
    );
    eprintln!("{with_title}/{} oturumda başlık var", sessions.len());
}
