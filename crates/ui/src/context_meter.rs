//! Turun ödediği bağlamı gösteren ölçer ve otomatik sıkıştırma eşiği.
//!
//! Neden var: maliyetin asıl sürücüsü bağlamın her turda yeniden okunması.
//! 600k'lık bir sohbette yazılan her mesaj 600k token'lık bir okuma demek ve
//! sayı görünmediği sürece konuşma oraya sessizce tırmanıyor. Ölçüldü —
//! yalnızca arayüzden sürülen oturumlar hiç sıkışmadan 788k'ya çıkmış, tur
//! başına maliyetleri sıkışan oturumların ~2,5 katıydı.
//!
//! # Eşik neden sabit değil
//!
//! İlk hâli 200k'da sıkıştırıyordu ve bu yanlıştı: uzun bağlamlı modeller 1M'e
//! kadar taşıyor, dolayısıyla 200k'da sıkıştırmak kullanıcının parasını
//! ödediği bağlamın dörtte üçünü daha kullanmadan atmak demekti. Eşik artık
//! **modelin gerçek penceresinin oranı**: 200k'lık bir modelde 170k, 1M'lik
//! bir modelde 850k.
//!
//! Oran yüksek (`AUTO_COMPACT_FRACTION`) çünkü sıkıştırmanın kendi bedeli var
//! — bağlamı bir kez okuyup özet yazıyor — ve özet daima ayrıntı kaybediyor.
//! Amaç tasarruf için erken sıkıştırmak değil, duvara toslamadan önce yer
//! açmak. Claude Code'un kendi otomatik sıkıştırması da pencereye yakın
//! tetikleniyor; sürpriz olmaması için aynı hizada duruyoruz.
//!
//! Renk basamakları hesap ölçerleriyle ortak ([`crate::settings::accounts`]):
//! aynı üç seviye, aynı tema token'ları — iki ölçer aynı dili konuşuyor.

use gpui::{Div, SharedString, div, prelude::*, px};
pub use postillion_proto::{
    AUTO_COMPACT_FRACTION, DEFAULT_CONTEXT_WINDOW, LONG_CONTEXT_WINDOW, compact_threshold,
    context_window,
};

use crate::settings::accounts::{usage_color, usage_level};
use crate::theme::Theme;

/// Ölçümün pencereye oranı; 1.0'da doyuyor.
pub fn context_fraction(tokens: u64, window: u64) -> f32 {
    if window == 0 {
        return 0.0;
    }
    (tokens as f32 / window as f32).min(1.0)
}

/// `142_000` → `"142k"`.
///
/// Bin altı ham gösteriliyor; "0k" hiçbir şey söylemezdi.
pub fn format_context(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    format!("{}k", tokens.div_ceil(1_000).max(1))
}

/// Doluluk yüzdesi — `"27%"`.
///
/// Sayının kendisi tek başına bir şey söylemiyor: 232k, 200k'lık bir modelde
/// duvarın ötesi, 1M'likte dörtte biri bile değil. Oran zaten hesaplanıyordu
/// ama hiç gösterilmiyordu.
///
/// Sıfırın üstündeki her ölçüm en az `%1` yazıyor: `%0` "ölçüm yok" gibi
/// okunur, oysa bir şey ölçülmüş durumda.
pub fn format_fill(tokens: u64, window: u64) -> String {
    let percent = (context_fraction(tokens, window) * 100.0).round() as u32;
    if tokens > 0 && percent == 0 {
        return "%1".into();
    }
    format!("%{percent}")
}

/// Ölçer satırı — footer etiketleriyle aynı ölçüler.
///
/// Renk pencereye göre: 1M'lik bir modelde 300k sakin, 200k'lık bir modelde
/// aynı sayı çoktan kritik. Sabit bir eşik ikisini de yanlış boyardı.
///
/// Sayının yanında doluluk çubuğu ve yüzdesi var. Çıplak bir "232k" pencerenin
/// neresinde olunduğunu söylemiyordu ve asıl sorulan bu.
pub fn context_meter(tokens: u64, window: u64, theme: &Theme) -> Div {
    let fraction = context_fraction(tokens, window);
    let level = usage_level(fraction);
    let color = usage_color(level, theme);

    // Çubuk sabit genişlikte: değişken genişlik, sayı büyüdükçe satırı
    // oynatır ve göz her turda yeniden yer arardı.
    const BAR_WIDTH: f32 = 28.0;
    let bar = div()
        .w(px(BAR_WIDTH))
        .h(px(3.0))
        .rounded(px(2.0))
        .bg(color.opacity(0.18))
        .child(
            div()
                // Sıfırdan büyük her doluluk görünür kalmalı; 1 piksel altına
                // düşen bir dolgu hiç çizilmemiş gibi görünürdü.
                .w(px((BAR_WIDTH * fraction).max(if fraction > 0.0 { 2.0 } else { 0.0 })))
                .h(px(3.0))
                .rounded(px(2.0))
                .bg(color),
        );

    div()
        .h(px(20.0))
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color)
        .child(
            crate::icons::icon(crate::icons::HARD_DRIVE)
                .size(px(12.0))
                .text_color(color),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(SharedString::from(format_context(tokens))),
        )
        .child(bar)
        .child(
            div()
                .flex_none()
                .text_color(color.opacity(0.75))
                .child(SharedString::from(format_fill(tokens, window))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::accounts::UsageLevel;
    /// Yüzde, pencereye göre okunmalı: aynı sayı iki modelde farklı anlam.
    #[test]
    fn doluluk_pencereye_gore_yaziliyor() {
        assert_eq!(format_fill(100_000, DEFAULT_CONTEXT_WINDOW), "%50");
        assert_eq!(format_fill(100_000, LONG_CONTEXT_WINDOW), "%10");
        assert_eq!(format_fill(0, DEFAULT_CONTEXT_WINDOW), "%0");
    }

    /// Ölçülmüş ama çok küçük bir bağlam `%0` yazmamalı: o "ölçüm yok" gibi
    /// okunur, oysa bir şey ölçülmüş durumda.
    #[test]
    fn kucuk_ama_var_olan_olcum_sifir_yazmiyor() {
        assert_eq!(format_fill(200, LONG_CONTEXT_WINDOW), "%1");
    }

    /// Pencereyi aşan bir ölçüm %100'de doyuyor — çubuk taşmamalı.
    #[test]
    fn pencereyi_asan_olcum_doyuyor() {
        assert_eq!(format_fill(5_000_000, DEFAULT_CONTEXT_WINDOW), "%100");
        assert_eq!(context_fraction(5_000_000, DEFAULT_CONTEXT_WINDOW), 1.0);
    }

    #[test]
    fn olcum_kisaltilarak_yazilir() {
        // Bin altı ham: "0k" hiçbir şey söylemez.
        assert_eq!(format_context(0), "0");
        assert_eq!(format_context(999), "999");
        // Yukarı yuvarlanıyor ki 1000 token "1k" olsun, "1k"dan az görünmesin.
        assert_eq!(format_context(1_000), "1k");
        assert_eq!(format_context(1_001), "2k");
        assert_eq!(format_context(142_000), "142k");
        assert_eq!(format_context(788_000), "788k");
    }

    #[test]
    fn ayni_sayi_farkli_pencerede_farkli_okunuyor() {
        // 300k, 1M'lik modelde sakin; 200k'lık modelde çoktan tavanda.
        assert_eq!(
            usage_level(context_fraction(300_000, LONG_CONTEXT_WINDOW)),
            UsageLevel::Normal
        );
        assert_eq!(
            usage_level(context_fraction(300_000, DEFAULT_CONTEXT_WINDOW)),
            UsageLevel::Critical
        );

        assert_eq!(
            usage_level(context_fraction(170_000, DEFAULT_CONTEXT_WINDOW)),
            UsageLevel::Warn,
            "sıkıştırma eşiğinde uyarı rengi bekleniyor"
        );
        // Sıfır pencere savunması: bölme yok, sakin.
        assert_eq!(context_fraction(1_000, 0), 0.0);
    }
}
