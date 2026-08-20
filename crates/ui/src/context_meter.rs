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
pub use zeron_proto::{
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

/// Ölçer satırı — footer etiketleriyle aynı ölçüler.
///
/// Renk pencereye göre: 1M'lik bir modelde 300k sakin, 200k'lık bir modelde
/// aynı sayı çoktan kritik. Sabit bir eşik ikisini de yanlış boyardı.
pub fn context_meter(tokens: u64, window: u64, theme: &Theme) -> Div {
    let level = usage_level(context_fraction(tokens, window));
    let color = usage_color(level, theme);

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::accounts::UsageLevel;
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
