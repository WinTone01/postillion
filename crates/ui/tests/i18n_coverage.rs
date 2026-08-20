//! Katalogdaki her dizenin gerçekten `t()` üzerinden geçtiğini doğrular.
//!
//! Çeviri eklemek iki adım: kataloğa karşılığı yazmak ve çağrı yerini
//! sarmalamak. İkincisi unutulduğunda hiçbir şey bozulmuyor — metin yalnızca
//! Türkçe yerelde İngilizce kalıyor, ki bu gözden kaçması en kolay hata türü.
//! Bu test o sessiz boşluğu gürültülü hale getiriyor.
//!
//! Tarama çağrı BİÇİMİNE bakmıyor, çünkü metin `SharedString::from(…)`,
//! `.child(…)`, `label: …`, `menu_heading(theme, …)` ve bir `if` kolunda çıplak
//! değişmez olarak da geçebiliyor; ilk sürüm yalnızca birini kontrol ediyordu
//! ve elli kadar yeri gözden kaçırmıştı.
//!
//! İki şey kasıtlı olarak dışarıda:
//!
//! - **Yorumlar.** Doküman yorumları arayüz metnini sık sık alıntılıyor
//!   ("Settings" başlığı gibi); onları sarmalamak anlamsız.
//! - **Test modülleri.** Test verisi kullanıcıya gösterilmiyor ve sarmalamak
//!   testleri yerele bağımlı yapardı — Türkçe bir makinede kırılırlardı.

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

        let mut in_tests = false;
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim();

            // Test modülünden dosya sonuna kadar her şey test verisi sayılıyor;
            // `#[cfg(test)]` bu dosyalarda daima sondaki modülü açıyor.
            if trimmed.starts_with("#[cfg(test)]") {
                in_tests = true;
            }
            if in_tests || trimmed.starts_with("//") {
                continue;
            }

            // Sarmalama satır sonuna taşabiliyor: uzun bir dize
            // `crate::i18n::t(` ile açılıp bir sonraki satırda yazılıyor.
            // Önceki satırın `t(` ile bitmesi de geçerli bir sarmalama.
            let wrapped_above = number
                .checked_sub(1)
                .and_then(|prev| text.lines().nth(prev))
                .is_some_and(|prev| prev.trim_end().ends_with("t("));

            for key in &keys {
                let quoted = format!("\"{key}\"");
                if line.contains(&quoted)
                    && !line.contains(&format!("t({quoted})"))
                    && !wrapped_above
                {
                    misses.push(format!(
                        "{}:{}  {}",
                        path.strip_prefix(&src).unwrap_or(&path).display(),
                        number + 1,
                        trimmed
                    ));
                }
            }
        }
    }

    assert!(
        misses.is_empty(),
        "katalogda karşılığı olan ama t() ile sarmalanmamış {} yer:\n  {}",
        misses.len(),
        misses.join("\n  ")
    );
}
