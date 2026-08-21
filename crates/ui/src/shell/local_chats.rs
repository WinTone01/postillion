//! Yerel Claude Code oturumları: kenar çubuğundaki giriş satırı ve içe
//! aktarma paleti.
//!
//! Postillion 1 oturum listesi olarak doğrudan `~/.claude/projects` altındaki
//! transcript'leri gösteriyordu. Burada aynı algılama var ama araya bir adım
//! giriyor: motor (`ListLocalChats`) makinedeki oturumları tarıyor, kullanıcı
//! birini seçiyor ve `AdoptLocalChat` onu bir Postillion sohbetine çeviriyor —
//! geçmiş dokümana yazılıyor, sonraki tur `--resume` ile aynı konuşmayı
//! sürdürüyor.
//!
//! Palet, proje ekleme paletiyle aynı iskelet: üstte arama çubuğu, altta
//! klavyeyle gezilen satırlar, en altta tuş ipuçları. Zaten içe aktarılmış
//! satırlar listede kalıyor ama "İçe aktarıldı" etiketiyle; tıklanınca yeni
//! bir kopya açmak yerine mevcut sohbete gidiyor.

use super::*;
use gpui::FocusHandle;
use serde::Deserialize;

/// `ListLocalChats` satırı (motorun `LocalChat`'i, düzleştirilmiş).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LocalChatRow {
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub modified_ms: u64,
    /// Bu oturumu daha önce benimsemiş sohbet.
    #[serde(default)]
    pub chat_id: Option<String>,
}

impl LocalChatRow {
    /// Aramada eşleşen metin: başlık + klasör (ikisiyle de aranıyor).
    fn haystack(&self) -> String {
        let title = self.title.clone().unwrap_or_default();
        match &self.cwd {
            Some(cwd) => format!("{title} {cwd}"),
            None => title,
        }
    }

    fn display_title(&self) -> SharedString {
        match self.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(title) => title.to_string().into(),
            None => crate::i18n::t("Untitled session").to_string().into(),
        }
    }

    /// Klasörün son iki segmenti — tam yol satıra sığmıyor, kök tek başına
    /// ayırt etmiyor ("src" her projede var).
    fn folder(&self) -> Option<SharedString> {
        let cwd = self.cwd.as_deref()?;
        let mut parts = cwd.rsplit('/').filter(|s| !s.is_empty());
        let last = parts.next()?;
        Some(match parts.next() {
            Some(parent) => format!("{parent}/{last}").into(),
            None => last.to_string().into(),
        })
    }
}

/// Açık palet.
pub(super) struct LocalChatsPalette {
    search: Entity<ComposerInput>,
    rows: Loadable<Vec<LocalChatRow>>,
    /// Klavye vurgusu, süzülmüş satırlar içinde.
    active: usize,
    /// Kartın odağı — arama girdisi odaktayken kare düzeyi tuşları buraya
    /// düşürüyor (çalışan her paletin yapısı).
    focus: FocusHandle,
    list_scroll: gpui::ScrollHandle,
    focus_pending: bool,
    /// İçe aktarılmayı bekleyen oturum (satır bu sırada kilitli).
    busy: Option<String>,
    error: Option<SharedString>,
    load_task: Option<Task<()>>,
    adopt_task: Option<Task<()>>,
    _search_events: Subscription,
}

/// `claude-opus-5-20260101` → `opus-5`. Sağlayıcı öneki ve tarih kuyruğu
/// satırda yer kaplamaktan başka bir şey yapmıyor.
fn short_model_name(model: &str) -> String {
    let trimmed = model.strip_prefix("claude-").unwrap_or(model);
    let mut parts: Vec<&str> = trimmed.split('-').collect();
    // Sondaki 8 haneli sürüm tarihi (varsa) düşüyor.
    if parts
        .last()
        .is_some_and(|p| p.len() == 8 && p.chars().all(|c| c.is_ascii_digit()))
    {
        parts.pop();
    }
    parts.join("-")
}

impl Shell {
    pub(super) fn open_local_chats(&mut self, cx: &mut Context<Self>) {
        // "PaletteSearch" bağlamı: ↑↓/⏎ metin imlecini oynatmak yerine karenin
        // tuş işleyicisine çıkıyor.
        let search = cx.new(|cx| {
            ComposerInput::with_context(crate::i18n::t("Search local sessions…"), "PaletteSearch", cx)
        });
        let search_events = cx.subscribe(&search, |this: &mut Shell, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                if let Some(palette) = this.local_chats.as_mut() {
                    palette.active = 0;
                }
                cx.notify();
            }
        });
        self.local_chats = Some(LocalChatsPalette {
            search,
            rows: Loadable::Idle,
            active: 0,
            focus: cx.focus_handle(),
            list_scroll: gpui::ScrollHandle::new(),
            focus_pending: true,
            busy: None,
            error: None,
            load_task: None,
            adopt_task: None,
            _search_events: search_events,
        });
        self.load_local_chats(cx);
        cx.notify();
    }

    /// Tarama. Bloklayan iş motorda; burada tek bir çağrı var.
    fn load_local_chats(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let Some(palette) = self.local_chats.as_mut() else {
            return;
        };
        palette.rows = Loadable::Loading;
        palette.load_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(methods::LIST_LOCAL_CHATS, serde_json::json!({}))
                .await;
            this.update(cx, |shell, cx| {
                if let Some(palette) = shell.local_chats.as_mut() {
                    palette.rows = match result {
                        Ok(value) => match serde_json::from_value::<Vec<LocalChatRow>>(value) {
                            Ok(rows) => Loadable::Ready(rows),
                            Err(err) => Loadable::Error(err.to_string()),
                        },
                        Err(err) => Loadable::Error(err.to_string()),
                    };
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Arama sorgusuna göre süzülmüş satırlar (önek eşleşmeleri önce).
    fn local_chats_filtered(&self, cx: &App) -> Vec<LocalChatRow> {
        let Some(palette) = self.local_chats.as_ref() else {
            return Vec::new();
        };
        let Some(rows) = palette.rows.ready() else {
            return Vec::new();
        };
        let query = palette.search.read(cx).text().to_string();
        if query.trim().is_empty() {
            return rows.clone();
        }
        let haystacks: Vec<String> = rows.iter().map(LocalChatRow::haystack).collect();
        let names: Vec<&str> = haystacks.iter().map(String::as_str).collect();
        popover::filter_indices(&query, &names)
            .into_iter()
            .map(|ix| rows[ix].clone())
            .collect()
    }

    /// Vurgulu satırı aç.
    fn local_chats_open_active(&mut self, cx: &mut Context<Self>) {
        let rows = self.local_chats_filtered(cx);
        let Some(palette) = self.local_chats.as_ref() else {
            return;
        };
        let Some(row) = rows.get(palette.active).cloned() else {
            return;
        };
        self.open_local_chat_row(row, cx);
    }

    /// Bir satırı aç: zaten içe aktarılmışsa o sohbete git, değilse benimse.
    pub(super) fn open_local_chat_row(&mut self, row: LocalChatRow, cx: &mut Context<Self>) {
        if let Some(chat_id) = row.chat_id.clone() {
            self.local_chats = None;
            self.open_chat(chat_id, cx);
            return;
        }
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let Some(palette) = self.local_chats.as_mut() else {
            return;
        };
        if palette.busy.is_some() {
            return; // tek uçuş: iki kez benimseme yok
        }
        palette.busy = Some(row.session_id.clone());
        palette.error = None;
        let session_id = row.session_id.clone();
        palette.adopt_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    methods::ADOPT_LOCAL_CHAT,
                    serde_json::json!({ "sessionId": session_id }),
                )
                .await;
            this.update(cx, |shell, cx| {
                match result
                    .map_err(|err| err.to_string())
                    .and_then(|value| {
                        value
                            .get("chatId")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                            .ok_or_else(|| "AdoptLocalChat returned no chatId".to_string())
                    }) {
                    Ok(chat_id) => {
                        shell.local_chats = None;
                        // Kayıt satırı eşitlenene kadar seçim boşa düşmesin:
                        // `open_chat` seçimi yazıyor, liste geldiğinde satır
                        // zaten seçili oluyor.
                        shell.open_chat(chat_id, cx);
                    }
                    Err(message) => {
                        if let Some(palette) = shell.local_chats.as_mut() {
                            palette.busy = None;
                            palette.error = Some(message.into());
                        }
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
        cx.notify();
    }

    fn local_chats_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        let key = popover::classify_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.control,
        );
        match key {
            popover::MenuKey::Escape => {
                self.local_chats = None;
                cx.notify();
            }
            popover::MenuKey::Up | popover::MenuKey::Down => {
                let count = self.local_chats_filtered(cx).len();
                let delta = if key == popover::MenuKey::Up { -1 } else { 1 };
                if let Some(palette) = self.local_chats.as_mut() {
                    palette.active =
                        popover::menu_step(Some(palette.active), count, delta).unwrap_or(0);
                    palette.list_scroll.scroll_to_item(palette.active);
                    cx.notify();
                }
            }
            popover::MenuKey::Enter | popover::MenuKey::ModEnter => {
                self.local_chats_open_active(cx)
            }
            _ => {}
        }
    }

    /// Kenar çubuğunun altındaki giriş satırı. Her zaman görünür: tarama
    /// pahalı olduğu için sayıyı önden çekmiyoruz, palet açılınca tarıyoruz.
    pub(super) fn render_local_chats_entry(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("local-chats-entry")
            .mt(px(12.0))
            .mx(px(2.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(12.0))
            .text_color(theme.text_muted.opacity(0.7))
            .hover(|s| s.bg(theme.element_hover).text_color(theme.text))
            .on_click(cx.listener(|this, _, _, cx| this.open_local_chats(cx)))
            .child(
                icon(icons::CLAUDE_MARK)
                    .size(px(13.0))
                    .flex_none()
                    .text_color(theme.text_muted.opacity(0.7)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(crate::i18n::t("Sessions on this machine"))),
            )
            .child(
                icon(icons::ALT_ARROW_RIGHT)
                    .size(px(12.0))
                    .flex_none()
                    .text_color(theme.text_muted.opacity(0.45)),
            )
            .into_any_element()
    }

    /// Palet kartı: arama çubuğu · oturum listesi · tuş ipuçları.
    pub(super) fn render_local_chats_overlay(
        &mut self,
        viewport: gpui::Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        {
            let palette = self.local_chats.as_mut()?;
            if std::mem::take(&mut palette.focus_pending) {
                let handle = palette.search.focus_handle(cx);
                window.focus(&handle, cx);
            }
        }
        let (search, loading, load_error, active, focus, list_scroll, busy, error) = {
            let palette = self.local_chats.as_ref()?;
            (
                palette.search.clone(),
                matches!(palette.rows, Loadable::Loading | Loadable::Idle),
                palette.rows.error().map(str::to_string),
                palette.active,
                palette.focus.clone(),
                palette.list_scroll.clone(),
                palette.busy.clone(),
                palette.error.clone(),
            )
        };
        let rows = self.local_chats_filtered(cx);
        let query_empty = search.read(cx).is_empty();
        let hairline = crate::theme::hairline(0.06);
        let card_radius = 14.0;
        let band = popover::band();
        let now = Utc::now();

        let key_chip = |theme: &Theme| {
            div()
                .h(px(22.0))
                .px(px(6.0))
                .rounded(px(5.0))
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .bg(crate::theme::ink(0.05))
                .text_size(px(11.0))
                .font_family(theme.font_mono.clone())
                .text_color(theme.text_muted.opacity(0.7))
        };

        let input_row = div()
            .h(px(46.0))
            .flex_none()
            .rounded_t(px(card_radius))
            .pl(px(12.0))
            .pr(px(10.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .bg(band)
            .border_b_1()
            .border_color(hairline)
            .child(
                icon(icons::MAGNIFER)
                    .size(px(15.0))
                    .flex_none()
                    .text_color(theme.text_muted.opacity(0.7)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(14.0))
                    .child(search.clone().into_any_element()),
            )
            .child(
                key_chip(&theme)
                    .id("local-chats-esc")
                    .cursor_pointer()
                    .hover(|s| s.bg(crate::theme::ink(0.09)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.local_chats = None;
                        cx.notify();
                    }))
                    .child(SharedString::from(crate::i18n::t("esc"))),
            );

        let list: AnyElement = if let Some(message) = load_error {
            div()
                .px(px(14.0))
                .py(px(16.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme.danger)
                        .child(SharedString::from(message)),
                )
                .child(
                    popover::btn_ghost(&theme, crate::i18n::t("Retry"), "local-chats-retry")
                        .id("local-chats-retry")
                        .on_click(cx.listener(|this, _, _, cx| this.load_local_chats(cx))),
                )
                .into_any_element()
        } else if loading {
            div()
                .px(px(14.0))
                .py(px(16.0))
                .text_size(px(12.5))
                .text_color(theme.text_faint)
                .child(SharedString::from(crate::i18n::t("Scanning…")))
                .into_any_element()
        } else if rows.is_empty() {
            div()
                .px(px(14.0))
                .py(px(16.0))
                .text_size(px(12.5))
                .text_color(theme.text_faint)
                .child(SharedString::from(if query_empty {
                    crate::i18n::t("No Claude Code sessions found on this machine")
                } else {
                    crate::i18n::t("No sessions match")
                }))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .py(px(6.0))
                .child(
                    div()
                        .id("local-chats-list")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&list_scroll)
                        .px(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .children(rows.into_iter().enumerate().map(|(ix, row)| {
                            let title = row.display_title();
                            let folder = row.folder();
                            let branch = row.git_branch.clone();
                            // Modelin kısa adı: `claude-opus-5` → `opus-5`
                            // (v1'in rozetiyle aynı bilgi, satıra sığan hâli).
                            let model = row
                                .model
                                .as_deref()
                                .map(short_model_name)
                                .filter(|m| !m.is_empty());
                            let imported = row.chat_id.is_some();
                            let adopting = busy.as_deref() == Some(row.session_id.as_str());
                            let when: SharedString = chrono::DateTime::<Utc>::from_timestamp_millis(
                                row.modified_ms as i64,
                            )
                            .map(|at| format_time_ago(at, now))
                            .unwrap_or_default()
                            .into();
                            let pick = row.clone();
                            popover::menu_row_nav(
                                &theme,
                                false,
                                ix == active,
                                format!("local-chat-{ix}"),
                            )
                            .when(ix == active, |el| {
                                el.shadow(crate::theme::card_selected_shadows())
                            })
                            .h(px(44.0))
                            .id(("local-chat", ix))
                            .when(busy.is_some() && !adopting, |el| el.opacity(0.5))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_local_chat_row(pick.clone(), cx);
                            }))
                            .child(
                                icon(icons::CLAUDE_MARK)
                                    .size(px(15.0))
                                    .flex_none()
                                    .text_color(theme.text_muted.opacity(0.8)),
                            )
                            // İki satır: başlık, altında klasör · dal · zaman.
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(div().min_w_0().truncate().child(title))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(6.0))
                                            .min_w_0()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted.opacity(0.55))
                                            .when_some(folder, |el, folder| {
                                                el.child(
                                                    div().min_w_0().truncate().child(folder),
                                                )
                                            })
                                            .when_some(branch, |el, branch| {
                                                el.child(
                                                    icon(icons::GIT_BRANCH)
                                                        .size(px(11.0))
                                                        .flex_none()
                                                        .text_color(
                                                            theme.text_muted.opacity(0.45),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .child(SharedString::from(branch)),
                                                )
                                            })
                                            .child(div().flex_none().child(when))
                                            .when_some(model, |el, model| {
                                                el.child(
                                                    div()
                                                        .flex_none()
                                                        .child(SharedString::from(model)),
                                                )
                                            }),
                                    ),
                            )
                            // Sağ uç: içe aktarılmışsa etiket, benimseme
                            // sürüyorsa ilerleme sözcüğü.
                            .when(adopting, |el| {
                                el.child(
                                    div()
                                        .flex_none()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted.opacity(0.7))
                                        .child(SharedString::from(crate::i18n::t("Importing…"))),
                                )
                            })
                            .when(imported && !adopting, |el| {
                                el.child(
                                    div()
                                        .flex_none()
                                        .text_size(px(10.0))
                                        .px(px(5.0))
                                        .py(px(1.0))
                                        .rounded(px(4.0))
                                        .bg(crate::theme::ink(0.05))
                                        .text_color(theme.text_muted.opacity(0.6))
                                        .child(SharedString::from(crate::i18n::t("Imported"))),
                                )
                            })
                        })),
                )
                .into_any_element()
        };

        let footer = div()
            .flex_none()
            .rounded_b(px(card_radius))
            .bg(band)
            .border_t_1()
            .border_color(hairline)
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(popover::key_hint_pair(
                &theme,
                icons::ARROW_UP,
                icons::ARROW_DOWN,
                crate::i18n::t("Navigate"),
            ))
            .child(popover::key_hint(
                &theme,
                icons::RETURN,
                crate::i18n::t("Import"),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(match &error {
                        Some(_) => theme.danger,
                        None => theme.text_muted.opacity(0.5),
                    })
                    .child(match error {
                        Some(message) => message,
                        None => SharedString::from(crate::i18n::t(
                            "The transcript stays where it is; the session continues here.",
                        )),
                    }),
            );

        let card = div()
            .id("local-chats-palette")
            .w(px(620.0))
            .rounded(px(card_radius))
            .border_1()
            .border_color(crate::theme::hairline(0.10))
            .bg(if theme.is_frost() {
                theme.glass_overlay()
            } else {
                theme.surface_overlay
            })
            .shadow_lg()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_color(theme.text)
            .track_focus(&focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                this.local_chats_key(event, cx)
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.local_chats = None;
                cx.notify();
            }))
            .child(input_row)
            .child(div().h(px(330.0)).flex().flex_col().child(list))
            .child(footer)
            .into_any_element();

        Some(popover::modal_glass(
            "local-chats-dialog",
            viewport,
            card,
            card_radius,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: Option<&str>, cwd: Option<&str>) -> LocalChatRow {
        LocalChatRow {
            session_id: "s".into(),
            cwd: cwd.map(str::to_string),
            git_branch: None,
            title: title.map(str::to_string),
            model: None,
            modified_ms: 0,
            chat_id: None,
        }
    }

    #[test]
    fn klasor_son_iki_segmenti_gosterir() {
        assert_eq!(
            row(None, Some("/home/ali/Projects/postillion")).folder(),
            Some("Projects/postillion".into())
        );
        assert_eq!(row(None, Some("/tmp")).folder(), Some("tmp".into()));
        assert_eq!(row(None, None).folder(), None);
    }

    /// Arama hem başlıkta hem klasörde eşleşmeli — kullanıcı oturumu çoğu
    /// zaman projesiyle hatırlıyor.
    #[test]
    fn arama_metni_baslik_ve_klasoru_kapsar() {
        let haystack = row(Some("Fix the flaky test"), Some("/home/ali/postillion")).haystack();
        assert!(haystack.contains("flaky"));
        assert!(haystack.contains("postillion"));
    }

    #[test]
    fn model_adi_kisaltiliyor() {
        assert_eq!(short_model_name("claude-opus-5-20260101"), "opus-5");
        assert_eq!(short_model_name("claude-sonnet-5"), "sonnet-5");
        assert_eq!(short_model_name("gpt-5"), "gpt-5");
    }

    /// Motorun düzleştirilmiş satırı olduğu gibi çözülebilmeli.
    #[test]
    fn rpc_satiri_cozulur() {
        let value = serde_json::json!({
            "sessionId": "abc",
            "path": "/home/ali/.claude/projects/-home-ali/abc.jsonl",
            "cwd": "/home/ali",
            "gitBranch": "main",
            "title": "Bir şeyler",
            "model": "claude-opus-5",
            "hasConversation": true,
            "sizeBytes": 1024,
            "modifiedMs": 1_700_000_000_000u64,
            "chatId": "chat-1",
        });
        let row: LocalChatRow = serde_json::from_value(value).expect("parse");
        assert_eq!(row.session_id, "abc");
        assert_eq!(row.chat_id.as_deref(), Some("chat-1"));
        assert_eq!(row.display_title(), SharedString::from("Bir şeyler"));
    }
}
