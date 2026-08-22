//! Panelin renkleri uygulamanın temasından ÜRETİLİYOR.
//!
//! Web paneli masaüstü uygulamasıyla aynı görünmeli. Renkleri elle kopyalamak
//! bunu ilk gün sağlar ve sonra sessizce sapar: `theme.rs`'te bir ton
//! değiştiğinde CSS'i kimse güncellemez ve iki yüzey birbirinden ayrılır.
//!
//! Bu test CSS'i temadan üretip diskteki dosyayla karşılaştırıyor. Tema
//! değişirse test düşüyor ve nasıl güncelleneceğini söylüyor:
//!
//! ```sh
//! POSTILLION_UPDATE_PANEL_THEME=1 cargo test -p postillion-ui --test panel_theme
//! ```

use std::path::PathBuf;

use gpui::Hsla;
use postillion_ui::theme::Theme;

/// gpui `Hsla` zaten HSL tutuyor (hepsi 0..1), yani CSS'e dönüşüm birebir —
/// ara bir renk uzayı yok, dolayısıyla yuvarlama sapması da yok.
fn css(color: Hsla) -> String {
    let h = (color.h * 360.0).round();
    let s = (color.s * 100.0).round();
    let l = (color.l * 100.0).round();
    if color.a >= 1.0 {
        format!("hsl({h} {s}% {l}%)")
    } else {
        // Alfa üç haneye yuvarlanıyor: daha fazlası CSS'te fark yaratmıyor
        // ama dosyayı gereksiz yere oynatıyor.
        format!("hsl({h} {s}% {l}% / {:.3})", color.a)
    }
}

fn generate() -> String {
    let t = Theme::dark();

    // Panelin ihtiyaç duyduğu belirteçler. Temanın tamamı değil: kullanılmayan
    // bir belirteci taşımak, panelde karşılığı olmayan bir şeyi güncel tutmaya
    // çalışmak olurdu.
    let tokens: Vec<(&str, Hsla)> = vec![
        ("bg", t.bg),
        ("surface", t.surface),
        ("surface-card", t.surface_card),
        ("surface-raised", t.surface_raised),
        ("element-hover", t.element_hover),
        ("border", t.border),
        ("border-strong", t.border_strong),
        ("text", t.text),
        ("text-muted", t.text_muted),
        ("text-faint", t.text_faint),
        ("accent", t.accent),
        ("accent-strong", t.accent_strong),
        ("on-accent", t.on_accent),
        ("danger", t.danger),
        ("danger-muted", t.danger_muted),
        ("warning", t.warning),
        ("success", t.success),
        ("input-bg", t.input_bg),
    ];

    let mut out = String::new();
    out.push_str("/* ÜRETİLMİŞ DOSYA — elle düzenlemeyin.\n");
    out.push_str(" *\n");
    out.push_str(" * Kaynak: crates/ui/src/theme.rs (Theme::dark)\n");
    out.push_str(" * Üreten: crates/ui/tests/panel_theme.rs\n");
    out.push_str(" *\n");
    out.push_str(" * Güncellemek için:\n");
    out.push_str(" *   POSTILLION_UPDATE_PANEL_THEME=1 \\\n");
    out.push_str(" *     cargo test -p postillion-ui --test panel_theme\n");
    out.push_str(" */\n\n");
    out.push_str(":root {\n");
    for (name, color) in tokens {
        out.push_str(&format!("  --{name}: {};\n", css(color)));
    }
    out.push_str("}\n");
    out
}

fn target() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("depo kökü")
        .join("apps/panel/resources/css/theme.css")
}

#[test]
fn panel_temasi_uygulamayla_ayni() {
    let expected = generate();
    let path = target();

    if std::env::var("POSTILLION_UPDATE_PANEL_THEME").is_ok() {
        std::fs::create_dir_all(path.parent().expect("dizin")).expect("dizin oluşturulmalı");
        std::fs::write(&path, &expected).expect("yazılmalı");
        return;
    }

    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        actual,
        expected,
        "\npanel teması uygulamanınkinden sapmış ({}).\n\
         Güncellemek için:\n  \
         POSTILLION_UPDATE_PANEL_THEME=1 cargo test -p postillion-ui --test panel_theme\n",
        path.display()
    );
}
