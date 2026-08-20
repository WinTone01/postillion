//! Ajanın arka planda çalıştırdıklarını gösteren panel.
//!
//! Claude'un `Bash` aracı her komutu kendi alt süreci olarak açıyor. Uzun süren
//! ya da takılan bir komut arayüzde yalnızca "Çalışıyor" olarak görünüyordu:
//! neyin çalıştığı da, onu durdurmanın bir yolu da yoktu — tek çare turun
//! tamamını kesmekti, ki bu ajanın o ana kadarki işini de atıyor.
//!
//! Liste yoklanarak tazeleniyor. Süreçler bir olay yayınlamıyor; `/proc`'u
//! okumaktan başka yol yok ve ölçüldü, tarama ~8 ms sürüyor — panel açıkken
//! saniyede bir yoklamak ucuz, kapalıyken hiç yoklanmıyor.

use std::time::Duration;

use gpui::{AnyElement, Context, Entity, SharedString, Task, div, prelude::*, px};
use postillion_engine::processes::Proc;
use postillion_rpc::methods;

use crate::state::AppState;
use crate::theme::Theme;

/// Panel açıkken iki tarama arası.
///
/// Saniye altına inmenin anlamı yok: gösterilen tek değişken alan geçen süre
/// ve o zaten saniye çözünürlüğünde.
const POLL: Duration = Duration::from_secs(1);

pub struct ProcessPanel {
    state: Entity<AppState>,
    chat_id: String,
    rows: Vec<Proc>,
    /// Durdurma isteği yolda olan pid; butonu iki kez basmaya karşı kilitler.
    killing: Option<u32>,
    error: Option<SharedString>,
    _poll: Option<Task<()>>,
}

impl ProcessPanel {
    pub fn new(state: Entity<AppState>, chat_id: String, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            state,
            chat_id,
            rows: Vec::new(),
            killing: None,
            error: None,
            _poll: None,
        };
        panel.start_polling(cx);
        panel
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        let chat_id = self.chat_id.clone();
        self._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let engine = this
                    .update(cx, |panel, cx| panel.state.read(cx).engine().cloned())
                    .ok()
                    .flatten();

                if let Some(engine) = engine {
                    let result = engine
                        .client()
                        .call(
                            methods::LIST_PROCESSES,
                            serde_json::json!({ "chatId": chat_id }),
                        )
                        .await;
                    let updated = this.update(cx, |panel, cx| {
                        match result {
                            Ok(value) => match serde_json::from_value::<Vec<Proc>>(value) {
                                Ok(rows) => {
                                    // Liste değişmediyse yeniden çizmiyoruz:
                                    // saniyede bir render, seçim ve kaydırma
                                    // konumunu boşuna sarsardı.
                                    if rows != panel.rows {
                                        panel.rows = rows;
                                        cx.notify();
                                    }
                                    panel.error = None;
                                }
                                Err(err) => panel.error = Some(err.to_string().into()),
                            },
                            Err(err) => panel.error = Some(err.to_string().into()),
                        }
                        true
                    });
                    if updated.is_err() {
                        return; // panel kapandı
                    }
                }

                cx.background_executor().timer(POLL).await;
            }
        }));
    }

    fn kill(&mut self, pid: u32, force: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        self.killing = Some(pid);
        cx.notify();

        let chat_id = self.chat_id.clone();
        cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    methods::KILL_PROCESS,
                    serde_json::json!({ "chatId": chat_id, "pid": pid, "force": force }),
                )
                .await;
            this.update(cx, |panel, cx| {
                panel.killing = None;
                if let Err(err) = result {
                    panel.error = Some(err.to_string().into());
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// `93` → `1dk 33sn`. Saf.
pub fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}sn");
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{minutes}dk {}sn", secs % 60);
    }
    format!("{}sa {}dk", minutes / 60, minutes % 60)
}

/// Uzun komut satırlarını kısaltır.
///
/// Baş kırpılıyor değil son: komutun ayırt edici kısmı (program ve ilk
/// argümanlar) başta, uzun yollar sonda oluyor.
pub fn short_command(command: &str, max: usize) -> String {
    let trimmed = command.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

impl Render for ProcessPanel {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(SharedString::from(crate::i18n::t("Background processes"))),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_faint)
                    .child(SharedString::from(self.rows.len().to_string())),
            );

        let body: AnyElement = if let Some(error) = &self.error {
            div()
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(theme.danger)
                .child(error.clone())
                .into_any_element()
        } else if self.rows.is_empty() {
            div()
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child(SharedString::from(crate::i18n::t(
                    "Nothing running under the agent right now.",
                )))
                .into_any_element()
        } else {
            let killing = self.killing;
            let mut list = div()
                .id("process-list")
                .flex()
                .flex_col()
                .overflow_y_scroll();

            for proc in self.rows.clone() {
                let pid = proc.pid;
                let busy = killing == Some(pid);
                list = list.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_b_1()
                        .border_color(theme.border.opacity(0.5))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(SharedString::from(short_command(&proc.command, 120))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(10.0))
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child(SharedString::from(format!("pid {pid}")))
                                .child(SharedString::from(format_elapsed(proc.elapsed_secs)))
                                .child(div().flex_1())
                                .child(
                                    crate::popover::btn_ghost(
                                        &theme,
                                        if busy {
                                            crate::i18n::t("Stopping…")
                                        } else {
                                            crate::i18n::t("Stop")
                                        },
                                        format!("kill-{pid}"),
                                    )
                                    .id(("kill", pid as usize))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        // Önce nazik SIGTERM; kabuk ağaçları
                                        // buna uyuyor ve komut kendi
                                        // temizliğini yapabiliyor.
                                        this.kill(pid, false, cx);
                                    })),
                                ),
                        ),
                );
            }
            list.into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface_card)
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gecen_sure_okunur_yazilir() {
        assert_eq!(format_elapsed(0), "0sn");
        assert_eq!(format_elapsed(59), "59sn");
        assert_eq!(format_elapsed(60), "1dk 0sn");
        assert_eq!(format_elapsed(93), "1dk 33sn");
        assert_eq!(format_elapsed(3_599), "59dk 59sn");
        assert_eq!(format_elapsed(3_600), "1sa 0dk");
        assert_eq!(format_elapsed(7_380), "2sa 3dk");
    }

    #[test]
    fn uzun_komut_sondan_kirpiliyor() {
        // Ayırt edici kısım başta: program adı ve ilk argümanlar korunmalı.
        assert_eq!(short_command("cargo test", 20), "cargo test");
        assert_eq!(short_command("  cargo test  ", 20), "cargo test");
        assert_eq!(short_command("cargo test --workspace", 10), "cargo tes…");
        // Çok baytlı karakterler bayt değil KARAKTER sayılmalı, yoksa
        // kırpma bir karakterin ortasına düşüp paniklerdi.
        assert_eq!(short_command("çalıştır çok uzun", 5), "çalı…");
    }
}
