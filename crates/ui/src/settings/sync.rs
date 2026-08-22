//! Ayarlar → Eşitleme: kendi sunucunuzun adresi ve jetonu.
//!
//! Bu ikisi eskiden yalnızca ortam değişkeninden okunuyordu. Masaüstü
//! kısayolundan açılan bir uygulamada o değişkenler yok, dolayısıyla eşitleme
//! sessizce kapalı kalıyordu — ayarı buraya taşımak bu yüzden.
//!
//! Sayfa diske YAZIYOR ama çalışan motoru değiştirmiyor: uç nokta motor
//! açılışında okunuyor ve profil sınırı da (`login`/`logout`) aynı kurala
//! bağlı. Bu yüzden kaydettikten sonra yeniden başlatma uyarısı gösteriliyor;
//! sessizce yarısı yeni yarısı eski bir duruma düşmektense açıkça söylemek
//! doğru.

use gpui::{Context, Entity, EventEmitter, SharedString, Task, Window, div, prelude::*, px};

use postillion_engine::sync_config::{self, ProbeError, SyncConfig};

use crate::composer::ComposerInput;
use crate::settings::widgets;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub enum SyncPageEvent {
    /// Yapılandırma diske yazıldı.
    Saved,
}

/// Sınama sonucunun kullanıcıya dönüşü.
#[derive(Debug, Clone, PartialEq)]
enum Probe {
    Idle,
    Running,
    Ok,
    Failed(SharedString),
}

pub struct SyncPage {
    /// Veri dizini bilinmiyorsa kaydetmek mümkün değil. Varsayılana düşüp
    /// çalışma dizinine yazmak, jetonu beklenmedik bir yere bırakırdı.
    data_dir: Option<std::path::PathBuf>,
    url: Entity<ComposerInput>,
    token: Entity<ComposerInput>,
    /// Ortamdan geliyorsa panelden değiştirmek işe yaramaz; bunu söylemek
    /// gerekiyor, yoksa kaydeden kullanıcı neden değişmediğini anlayamaz.
    env_override: bool,
    error: Option<SharedString>,
    saved: bool,
    probe: Probe,
    task: Option<Task<()>>,
}

impl EventEmitter<SyncPageEvent> for SyncPage {}

impl SyncPage {
    pub fn new(data_dir: Option<std::path::PathBuf>, cx: &mut Context<Self>) -> Self {
        let stored = data_dir
            .as_deref()
            .map(SyncConfig::load)
            .unwrap_or_default();
        let url = cx.new(|cx| {
            let mut input = ComposerInput::new(
                crate::i18n::t("https://sync.your-domain.example"),
                cx,
            );
            input.set_text(&stored.edge_url, cx);
            input
        });
        let token = cx.new(|cx| {
            let mut input =
                ComposerInput::new(crate::i18n::t("The server's POSTILLION_SERVER_TOKEN"), cx);
            input.set_text(stored.token.as_deref().unwrap_or(""), cx);
            input
        });

        Self {
            data_dir,
            url,
            token,
            env_override: std::env::var("POSTILLION_EDGE_URL")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            error: None,
            saved: false,
            probe: Probe::Idle,
            task: None,
        }
    }

    fn values(&self, cx: &gpui::App) -> (String, String) {
        (
            self.url.read(cx).text().trim().to_string(),
            self.token.read(cx).text().trim().to_string(),
        )
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let (url, token) = self.values(cx);
        self.saved = false;

        // Doğrulama kaydetmeden ÖNCE: `/` ve `@` içeren bir jeton profil
        // yolunu bozuyor ve hata çok sonra, anlamsız bir yerde ortaya çıkıyor.
        if let Some(problem) = sync_config::url_problem(&url) {
            self.error = Some(crate::i18n::t(problem).into());
            cx.notify();
            return;
        }
        if let Some(problem) = sync_config::token_problem(&token) {
            self.error = Some(crate::i18n::t(problem).into());
            cx.notify();
            return;
        }

        let Some(data_dir) = self.data_dir.clone() else {
            self.error = Some(crate::i18n::t("No data directory — cannot save").into());
            cx.notify();
            return;
        };
        let config = SyncConfig {
            edge_url: url,
            token: Some(token),
        };
        match config.save(&data_dir) {
            Ok(()) => {
                self.error = None;
                self.saved = true;
                cx.emit(SyncPageEvent::Saved);
            }
            Err(err) => self.error = Some(format!("{err}").into()),
        }
        cx.notify();
    }

    /// `/health` çağırıp sunucunun cevap verdiğini doğrular.
    ///
    /// Kaydetmeden önce denenebilmesi bilinçli: yanlış bir adresi kaydedip
    /// uygulamayı yeniden başlattıktan sonra öğrenmek çok geç.
    fn probe(&mut self, cx: &mut Context<Self>) {
        let (url, token) = self.values(cx);
        if let Some(problem) = sync_config::url_problem(&url) {
            self.error = Some(crate::i18n::t(problem).into());
            cx.notify();
            return;
        }

        self.probe = Probe::Running;
        self.error = None;
        cx.notify();

        self.task = Some(cx.spawn(async move |page, cx| {
            let result = sync_config::probe(&url, Some(&token)).await;
            let _ = page.update(cx, |page, cx| {
                page.probe = match result {
                    Ok(()) => Probe::Ok,
                    Err(err) => Probe::Failed(describe(err).into()),
                };
                page.task = None;
                cx.notify();
            });
        }));
    }
}

/// Sınama hatasını kullanıcının bir şey yapabileceği bir cümleye çevirir.
fn describe(err: ProbeError) -> String {
    match err {
        ProbeError::Timeout => {
            crate::i18n::t("Timed out — is the address right and reachable?").to_string()
        }
        ProbeError::Connect => {
            crate::i18n::t("Could not connect — check the address and TLS").to_string()
        }
        // Bu ayrımı korumak önemli: sunucuya HİÇ ulaşılmadı, dolayısıyla
        // sunucu ayarlarında aramak boşuna.
        ProbeError::Blocked => crate::i18n::t(
            "Blocked before reaching the server — a proxy is refusing non-browser clients",
        )
        .to_string(),
        ProbeError::Status(code) => format!("{} {code}", crate::i18n::t("Unexpected reply:")),
        ProbeError::Other(message) => message,
    }
}

impl Render for SyncPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);

        widgets::page_column()
            .child(widgets::page_header(&theme, &crate::i18n::t("Sync"), None))
            .child(widgets::page_subtitle(
                &theme,
                crate::i18n::t(
                    "Postillion ships no hosted endpoint. Point it at a server you run yourself.",
                ),
            ))
            .when(self.env_override, |el| {
                el.child(widgets::warning_strip(
                    &theme,
                    crate::i18n::t(
                        "POSTILLION_EDGE_URL is set in the environment and overrides what you save here.",
                    ),
                ))
            })
            .child(
                widgets::section_card(&theme)
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(widgets::field_label(&theme, crate::i18n::t("Server address")))
                    .child(crate::popover::search_input_frame(
                        &theme,
                        self.url.clone().into_any_element(),
                    ))
                    .child(widgets::field_label(&theme, crate::i18n::t("Token")))
                    .child(crate::popover::search_input_frame(
                        &theme,
                        self.token.clone().into_any_element(),
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(crate::i18n::t(
                                "Anyone with this token can read every chat on the server. Chats are not yet end-to-end encrypted.",
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                crate::popover::btn_primary(&theme, &crate::i18n::t("Save"))
                                    .id("sync-save")
                                    .on_click(cx.listener(|page, _, _, cx| page.save(cx))),
                            )
                            .child(
                                crate::popover::btn_ghost(
                                    &theme,
                                    &if self.probe == Probe::Running {
                                        crate::i18n::t("Testing…")
                                    } else {
                                        crate::i18n::t("Test connection")
                                    },
                                    "sync-probe",
                                )
                                .id("sync-probe")
                                .on_click(cx.listener(|page, _, _, cx| page.probe(cx))),
                            ),
                    ),
            )
            .when_some(self.error.clone(), |el, message| {
                el.child(widgets::error_strip(&theme, message))
            })
            .when(self.probe == Probe::Ok, |el| {
                el.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme.text_muted)
                        .child(crate::i18n::t("Server reachable.")),
                )
            })
            .when_some(
                match &self.probe {
                    Probe::Failed(message) => Some(message.clone()),
                    _ => None,
                },
                |el, message| el.child(widgets::error_strip(&theme, message)),
            )
            .when(self.saved, |el| {
                // Kaydetmek çalışan motoru değiştirmiyor; uç nokta açılışta
                // okunuyor. Bunu söylememek, kullanıcının eşitlemenin neden
                // başlamadığını aramasına yol açardı.
                el.child(widgets::warning_strip(
                    &theme,
                    crate::i18n::t("Saved. Restart Postillion for it to take effect."),
                ))
            })
    }
}
