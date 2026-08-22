//! Dockerfile'ın kopyaladığı crate'ler sunucunun ihtiyaç duyduklarını kapsıyor mu.
//!
//! Konteyner imajı çalışma alanını KIRPIYOR: gpui'ye bağlı üyeler çıkarılıyor,
//! çünkü gpui bir git bağımlılığı ve sunucunun onunla işi yok. Bunun bedeli,
//! `crates/server`'a eklenen yeni bir bağımlılığın `cargo build --workspace`
//! yeşilken konteyner derlemesini sessizce bozması.
//!
//! Tam olarak bu oldu: cihaz rölesi `postillion-rpc`'ye bağlandı, yerel
//! derleme geçti, dağıtım `failed to read /src/crates/rpc/Cargo.toml` ile
//! düştü. Hiçbir CI imajı derlemediği için hata ancak sunucuda göründü.
//!
//! Bu test imaj derlemeden aynı şeyi yakalıyor: sunucunun yol bağımlılıklarını
//! kapanışıyla çıkarıp Dockerfile'ın `COPY`'leriyle ve kırpma listesiyle
//! karşılaştırıyor.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("depo kökü")
        .to_path_buf()
}

/// Bir crate'in `postillion-*` yol bağımlılıkları.
///
/// İsimlendirme kuralına dayanıyor: `postillion-x` → `crates/x`. Çalışma
/// alanının tamamı bu kurala uyuyor ve uymayan bir üye eklenirse burada
/// görünmez kalmaz — kapanış eksik çıkar ve test düşer.
fn local_deps(root: &Path, crate_dir: &str) -> BTreeSet<String> {
    let manifest = root.join("crates").join(crate_dir).join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("{} okunamadı: {e}", manifest.display()));

    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            // `#` ile başlayan satırlar yorum; `dev-dependencies` de sayılıyor
            // çünkü Dockerfile testleri derlemese de cargo manifesti çözüyor.
            if line.starts_with('#') {
                return None;
            }
            let name = line.strip_prefix("postillion-")?;
            let name = name.split(['.', ' ', '=']).next()?;
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

/// `postillion-server`'ın yol bağımlılıklarının geçişli kapanışı.
fn closure(root: &Path) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = vec!["server".to_string()];
    while let Some(current) = queue.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        for dep in local_deps(root, &current) {
            if !seen.contains(&dep) {
                queue.push(dep);
            }
        }
    }
    seen
}

fn dockerfile(root: &Path) -> String {
    std::fs::read_to_string(root.join("deploy/Dockerfile")).expect("Dockerfile okunmalı")
}

#[test]
fn dockerfile_sunucunun_butun_bagimliliklarini_kopyaliyor() {
    let root = repo_root();
    let needed = closure(&root);
    let text = dockerfile(&root);

    let copied: BTreeSet<String> = text
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("COPY crates/")?;
            rest.split_whitespace().next().map(str::to_string)
        })
        .collect();

    let missing: Vec<&String> = needed.difference(&copied).collect();
    assert!(
        missing.is_empty(),
        "Dockerfile bu crate'leri kopyalamıyor: {missing:?}\n\
         Sunucunun ihtiyacı: {needed:?}\nKopyalananlar: {copied:?}\n\
         Konteyner derlemesi 'failed to read Cargo.toml' ile düşecek.",
    );
}

#[test]
fn kirpma_sunucunun_bagimliliklarini_elemiyor() {
    let root = repo_root();
    let needed = closure(&root);
    let text = dockerfile(&root);

    // `awk` kırpması üyeleri bir alternasyondan eliyor:
    //   /"(crates\/(harness|engine|update|ui|syntax)|apps\/postillion)"/
    let trimmed: BTreeSet<String> = text
        .split_once("crates\\/(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(list, _)| list.split('|').map(str::to_string).collect())
        .expect("kırpma listesi Dockerfile'da bulunmalı");

    let dropped: Vec<&String> = needed.intersection(&trimmed).collect();
    assert!(
        dropped.is_empty(),
        "kırpma sunucunun ihtiyaç duyduğu crate'leri eliyor: {dropped:?}\n\
         Çalışma alanı üyeliğinden çıkarılan bir crate, kopyalanmış olsa bile \
         cargo tarafından çözülemez.",
    );
}

/// Kırpmanın hedefi ıskalamadığını doğrulayan `grep`'ler duruyor mu.
///
/// Sessizce boşalan bir üye listesi çok daha sonra, anlamsız bir bağlantı
/// hatası olarak ortaya çıkardı.
#[test]
fn kirpma_kendini_dogruluyor() {
    let text = dockerfile(&repo_root());
    assert!(
        text.contains(r#"grep -q '"crates/server"'"#),
        "kırpma sonrası sunucu üyeliği doğrulanmalı"
    );
}
