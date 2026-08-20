//! Katalogdaki her dizenin gerçekten `t()` üzerinden geçtiğini doğrular.
//!
//! Çeviri eklemek iki adım: kataloğa karşılığı yazmak ve çağrı yerini
//! sarmalamak. İkincisi unutulduğunda hiçbir şey bozulmuyor — metin yalnızca
//! Türkçe yerelde İngilizce kalıyor, ki bu gözden kaçması en kolay hata türü.
//! Bu test o sessiz boşluğu gürültülü hale getiriyor.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn ui_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `"kaynak" => "karşılık",` satırlarından kaynak dizeler.
fn catalogue_keys(source: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find("\" =>") else {
            continue;
        };
        let key = &rest[..end];
        if !key.is_empty() {
            keys.insert(key.to_string());
        }
    }
    keys
}

#[test]
fn katalogdaki_dizeler_sarmalanmis() {
    let src = ui_src();
    let catalogue = std::fs::read_to_string(src.join("i18n.rs")).expect("i18n.rs okunabilmeli");
    let keys = catalogue_keys(&catalogue);
    assert!(
        keys.len() > 50,
        "katalog beklenenden küçük ({}); ayrıştırma bozulmuş olabilir",
        keys.len()
    );

    let mut files = Vec::new();
    rust_files(&src, &mut files);

    let mut misses: Vec<String> = Vec::new();
    for path in files {
        if path.file_name().is_some_and(|n| n == "i18n.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for key in &keys {
            // Sarmalanmamış hâl: doğrudan tırnak içinde, `t(` olmadan.
            let bare = format!("SharedString::from(\"{key}\")");
            if text.contains(&bare) {
                misses.push(format!(
                    "{}: {key:?}",
                    path.strip_prefix(&src).unwrap_or(&path).display()
                ));
            }
        }
    }

    assert!(
        misses.is_empty(),
        "katalogda karşılığı olan ama t() ile sarmalanmamış dizeler:\n  {}",
        misses.join("\n  ")
    );
}
