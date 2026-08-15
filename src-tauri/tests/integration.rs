//! Gerçek `~/.claude` verisine karşı uçtan uca doğrulama.
//!
//! Hesaplar geçici bir `POSTILLION_ROOT` altında oluşturulur, ama paylaşılan
//! öğeler gerçek `~/.claude`'a symlink'lenir. Testin asıl amacı silmenin
//! paylaşılan veriye dokunmadığını kanıtlamak.
//!
//! Tek bir test fonksiyonu: POSTILLION_ROOT süreç geneli bir env var, paralel
//! testler birbirinin ayağını kaydırırdı.

use std::fs;

/// Hesap profilleri, gerçek kimliği takas etmeden doğrulanıyor.
///
/// `switch()` bilerek test edilmiyor: sistem genelindeki kimliği değiştirmek
/// çalışan Claude süreçlerini etkiler ve testin yan etkisi olamaz.
#[test]
fn kimlik_profile_yakalanir_ve_listelenir() {
    let home = dirs::home_dir().expect("ev dizini");
    let config = home.join(".claude.json");

    if !config.exists() {
        eprintln!("~/.claude.json yok, test atlanıyor");
        return;
    }

    let source: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    let Some(identity) = source.get("oauthAccount") else {
        eprintln!("oauthAccount yok, test atlanıyor");
        return;
    };
    let email = identity
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .expect("e-posta");

    let root = std::env::temp_dir().join(format!("postillion-acc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("POSTILLION_ROOT", &root);

    // Listeleme aktif kimliği kendi profiline yakalıyor.
    let accounts = postillion_lib::testing::list_accounts().expect("listeleme");

    let expected_slug = postillion_lib::testing::slugify(email);
    let active = accounts
        .iter()
        .find(|a| a.is_active)
        .expect("etkin hesap bulunmalı");

    assert_eq!(active.slug, expected_slug, "slug e-postadan türetilmeli");
    assert_eq!(active.email.as_deref(), Some(email));
    assert!(active.has_credentials, "jeton saklanmalıydı");
    assert!(!active.label.is_empty(), "etiket boş olmamalı");

    // Jeton 0600 olmalı — kimlik bilgisi.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let creds = root.join(&expected_slug).join(".credentials.json");
        let mode = fs::metadata(&creds).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "jeton 0600 olmalı");
    }

    // Etkin hesap silinemez: kullanıcı kendini dışarıda bırakamamalı.
    let err = postillion_lib::testing::remove_account(&expected_slug);
    assert!(err.is_err(), "etkin hesap silinebildi");

    // Gerçek yapılandırma değişmemiş olmalı.
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(
        after.get("oauthAccount"),
        source.get("oauthAccount"),
        "TEST GERÇEK KİMLİĞİ DEĞİŞTİRDİ"
    );

    let _ = fs::remove_dir_all(&root);
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

/// `/usage` gerçekten çalışıyor mu.
///
/// `#[ignore]`: giriş yapılmış bir hesap ve bir `claude` süreci gerektiriyor
/// (~3 sn). Token harcamıyor — komut yerel. Elle çalıştırmak için:
/// `cargo test --test integration -- --ignored kullanim`
#[test]
#[ignore]
fn kullanim_sorgusu_pencere_dondurur() {
    let usage = postillion_lib::testing::query_usage().expect("sorgu başarılı olmalı");

    eprintln!("{:#?}", usage.windows);

    assert!(
        !usage.windows.is_empty(),
        "hiç limit penceresi ayrıştırılamadı — /usage çıktı biçimi değişmiş olabilir:\n{}",
        usage.detail
    );
    assert!(
        usage.windows.iter().any(|w| w.label.contains("session")),
        "oturum penceresi bekleniyordu"
    );
    assert!(usage.measured_at_ms > 0);
}

/// Yerel komut yoklamalarının listeye sızmadığını diskteki gerçek veriyle
/// doğrular. `claude -p "/usage"` her çağrıda bir transcript bırakıyor.
#[test]
#[ignore]
fn yoklama_oturumlari_listede_yok() {
    let sessions = postillion_lib::testing::scan_sessions().expect("tarama başarılı olmalı");

    let junk: Vec<_> = sessions
        .iter()
        .filter_map(|s| s.title.as_deref())
        .filter(|t| t.starts_with("<local-command") || t.starts_with("<command-"))
        .collect();

    assert!(junk.is_empty(), "yerel komut kayıtları listede: {junk:#?}");

    let synthetic: Vec<_> = sessions
        .iter()
        .filter(|s| s.model.as_deref() == Some("<synthetic>"))
        .map(|s| &s.session_id)
        .collect();

    assert!(synthetic.is_empty(), "sentetik model rozeti: {synthetic:#?}");

    eprintln!("{} oturum, hiçbirinde komut artığı yok", sessions.len());
}
