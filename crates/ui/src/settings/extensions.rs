//! Ayarlar → Uzantılar: eklentiler, marketplace'ler ve skill'ler.
//!
//! Üçü tek sayfada, çünkü aynı zinciri paylaşıyorlar: bir marketplace eklenti
//! yayınlıyor, eklenti de skill ve MCP sunucusu getiriyor. Ayrı sayfalara
//! bölmek kullanıcıyı "eklentim neden görünmüyor" sorusuyla üç sekme arasında
//! gezdirirdi — cevap çoğu zaman "marketplace eklenmemiş" oluyor.
//!
//! Kurulabilir eklenti listesi kasıtlı olarak **istek üzerine** yükleniyor:
//! ölçüldü, bu makinede 2563 kayıt dönüyor ve sayfayı açar açmaz çekmek hem
//! yavaş hem de çoğu ziyarette gereksiz.

use gpui::{
    AnyElement, Context, Entity, SharedString, Task, Window, div, prelude::*, px,
};
use postillion_harness::claude::catalog_manage::{Marketplace, Plugin, Skill};
use postillion_rpc::methods;

use crate::composer::ComposerInput;
use crate::state::AppState;
use crate::theme::Theme;

/// Sayfanın hangi listesi görünüyor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Installed,
    Available,
    Marketplaces,
    Skills,
    /// MCP sunucuları — ekleme burada, sohbete özel SEÇİM composer'daki
    /// rozette. İkisi farklı iş: burası cihaza sunucu tanıtıyor, rozet o
    /// sunuculardan hangilerinin bu sohbette açık olduğunu seçiyor.
    Mcp,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Installed,
        Tab::Available,
        Tab::Marketplaces,
        Tab::Skills,
        Tab::Mcp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Installed => crate::i18n::t("Installed"),
            Tab::Available => crate::i18n::t("Available"),
            Tab::Marketplaces => crate::i18n::t("Marketplaces"),
            Tab::Skills => crate::i18n::t("Skills"),
            Tab::Mcp => crate::i18n::t("MCP servers"),
        }
    }
}

/// Aramayı uygulayan saf süzgeç.
///
/// Kimlik ve açıklama birlikte taranıyor: kullanıcı çoğu zaman eklentinin tam
/// adını değil ne işe yaradığını hatırlıyor.
pub fn filter_plugins<'a>(plugins: &'a [Plugin], query: &str) -> Vec<&'a Plugin> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return plugins.iter().collect();
    }
    plugins
        .iter()
        .filter(|p| {
            p.id.to_lowercase().contains(&needle)
                || p.description
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
        })
        .collect()
}

pub fn filter_skills<'a>(skills: &'a [Skill], query: &str) -> Vec<&'a Skill> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return skills.iter().collect();
    }
    skills
        .iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&needle)
                || s.description
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
        })
        .collect()
}

/// Kullanıcının yazdığı tek satırdan MCP sunucu tanımı.
///
/// Biçim `ad=hedef`. Hedef `http://` ya da `https://` ile başlıyorsa uzak
/// sunucu (HTTP taşıması), değilse yerelde çalıştırılacak bir komut ve
/// argümanları. Tek satır seçildi çünkü alternatifi beş alanlı bir form ve
/// kullanıcıların çoğu bu iki şekilden birini zaten kopyalayıp yapıştırıyor.
///
/// Döndürülen üçlü: (ad, taşıma, hedef, komut argümanları).
pub fn parse_mcp_entry(input: &str) -> Result<(String, &'static str, String, Vec<String>), String> {
    let (name, rest) = input
        .split_once('=')
        .ok_or_else(|| "biçim: ad=komut ya da ad=https://…".to_string())?;
    let name = name.trim();
    let rest = rest.trim();

    if name.is_empty() {
        return Err("sunucu ismi boş olamaz".into());
    }
    if rest.is_empty() {
        return Err("hedef boş olamaz".into());
    }

    if rest.starts_with("http://") || rest.starts_with("https://") {
        return Ok((name.to_string(), "http", rest.to_string(), Vec::new()));
    }

    let mut parts = rest.split_whitespace();
    let command = parts.next().unwrap_or_default().to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();
    Ok((name.to_string(), "stdio", command, args))
}

/// `2563` → `2.5k`. Kurulum sayısı rozetinde yer kazanıyor.
pub fn short_count(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    format!("{:.1}k", count as f64 / 1_000.0)
}

#[derive(Default)]
struct Lists {
    installed: Vec<Plugin>,
    available: Vec<Plugin>,
    marketplaces: Vec<Marketplace>,
    skills: Vec<Skill>,
    mcp: Vec<String>,
}

pub struct ExtensionsPage {
    state: Entity<AppState>,
    tab: Tab,
    lists: Lists,
    /// Liste araması. Kurulabilir sekmede zorunlu: bu makinede 2563 kayıt
    /// dönüyor ve arama olmadan liste kullanılamaz.
    search: Entity<ComposerInput>,
    /// Marketplace kaynağı ya da yeni skill adı — sekmeye göre.
    new_entry: Entity<ComposerInput>,
    error: Option<SharedString>,
    /// Yükleme sürüyor mu — sekme başına değil, tek bayrak: aynı anda tek
    /// istek atılıyor.
    loading: bool,
    /// İşlem yolda olan kimlik; satırı kilitler.
    busy: Option<String>,
    /// Kurulabilir liste bir kez çekildi mi (istek üzerine yükleme).
    available_loaded: bool,
    task: Option<Task<()>>,
}

impl ExtensionsPage {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| ComposerInput::new(crate::i18n::t("Search…"), cx));
        let new_entry = cx.new(|cx| {
            ComposerInput::new(crate::i18n::t("Marketplace source (git URL or path)"), cx)
        });
        let mut page = Self {
            state,
            tab: Tab::Installed,
            lists: Lists::default(),
            search,
            new_entry,
            error: None,
            loading: false,
            busy: None,
            available_loaded: false,
            task: None,
        };
        page.reload(cx);
        page
    }

    fn query(&self, cx: &gpui::App) -> String {
        self.search.read(cx).text().trim().to_string()
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.search.update(cx, |input, cx| input.set_text("", cx));
        self.new_entry.update(cx, |input, cx| {
            input.set_text("", cx);
            input.set_placeholder(
                match tab {
                    Tab::Skills => crate::i18n::t("New skill name"),
                    Tab::Mcp => crate::i18n::t("name=command arg…  or  name=https://url"),
                    _ => crate::i18n::t("Marketplace source (git URL or path)"),
                },
                cx,
            );
        });
        // Kurulabilir liste büyük; ilk gerçek ziyarete kadar çekilmiyor.
        // Diğer sekmeler boşsa (ilk ziyaret ya da bir işlemden sonra
        // temizlendiyse) hemen çekiliyor.
        let needs_load = match tab {
            Tab::Available => !self.available_loaded,
            Tab::Installed => self.lists.installed.is_empty(),
            Tab::Marketplaces => self.lists.marketplaces.is_empty(),
            Tab::Skills => self.lists.skills.is_empty(),
            Tab::Mcp => self.lists.mcp.is_empty(),
        };
        if needs_load {
            self.reload(cx);
        }
        cx.notify();
    }

    /// Görünen sekmenin listesini tazeler.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let tab = self.tab;
        self.loading = true;
        self.error = None;
        cx.notify();

        self.task = Some(cx.spawn(async move |this, cx| {
            let method = match tab {
                Tab::Installed => methods::LIST_PLUGINS,
                Tab::Available => methods::LIST_AVAILABLE_PLUGINS,
                Tab::Marketplaces => methods::LIST_MARKETPLACES,
                Tab::Skills => methods::LIST_SKILLS,
                Tab::Mcp => methods::LIST_MCP_SERVERS,
            };
            let result = engine
                .client()
                .call(method, serde_json::json!({}))
                .await;

            this.update(cx, |page, cx| {
                page.loading = false;
                match result {
                    Ok(value) => {
                        match tab {
                            Tab::Installed => {
                                page.lists.installed =
                                    serde_json::from_value(value).unwrap_or_default();
                            }
                            Tab::Available => {
                                page.lists.available =
                                    serde_json::from_value(value).unwrap_or_default();
                                page.available_loaded = true;
                            }
                            Tab::Marketplaces => {
                                page.lists.marketplaces =
                                    serde_json::from_value(value).unwrap_or_default();
                            }
                            Tab::Skills => {
                                page.lists.skills =
                                    serde_json::from_value(value).unwrap_or_default();
                            }
                            Tab::Mcp => {
                                page.lists.mcp =
                                    serde_json::from_value(value).unwrap_or_default();
                            }
                        }
                        page.error = None;
                    }
                    Err(err) => page.error = Some(err.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Kimlik alan bir eylem çalıştırıp listeyi tazeler.
    fn act(&mut self, method: &'static str, id: String, enabled: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        self.busy = Some(id.clone());
        self.error = None;
        cx.notify();

        self.task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    method,
                    serde_json::json!({ "id": id, "enabled": enabled }),
                )
                .await;
            this.update(cx, |page, cx| {
                page.busy = None;
                match result {
                    Ok(_) => {
                        // Kurmak/kaldırmak İKİ listeyi birden değiştiriyor:
                        // kurulan eklenti "Kurulu"ya giriyor, "Kurulabilir"den
                        // çıkıyor. Yalnızca görünen sekmeyi tazelemek, kurulan
                        // eklentinin ancak uygulama yeniden açılınca listede
                        // görünmesine yol açıyordu.
                        page.available_loaded = false;
                        page.lists.installed.clear();
                        page.reload(cx);
                    }
                    Err(err) => {
                        page.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
    }

    /// Marketplace ekler ya da skill oluşturur — sekmeye göre.
    fn submit_new_entry(&mut self, cx: &mut Context<Self>) {
        let value = self.new_entry.read(cx).text().trim().to_string();
        if value.is_empty() {
            return;
        }
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let tab = self.tab;
        self.busy = Some(value.clone());
        self.error = None;
        cx.notify();

        self.task = Some(cx.spawn(async move |this, cx| {
            let (method, params) = match tab {
                Tab::Skills => (
                    methods::SKILL_CREATE,
                    serde_json::json!({ "name": value }),
                ),
                Tab::Mcp => match parse_mcp_entry(&value) {
                    Ok((name, transport, target, command_args)) => (
                        methods::ADD_MCP_SERVER,
                        serde_json::json!({
                            "name": name,
                            "transport": transport,
                            "target": target,
                            "commandArgs": command_args,
                        }),
                    ),
                    Err(message) => {
                        // Biçim hatası ağa gitmeden burada bitiyor.
                        this.update(cx, |page, cx| {
                            page.busy = None;
                            page.error = Some(message.into());
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                },
                _ => (
                    methods::MARKETPLACE_ADD,
                    serde_json::json!({ "id": value }),
                ),
            };
            let result = engine.client().call(method, params).await;
            this.update(cx, |page, cx| {
                page.busy = None;
                match result {
                    Ok(_) => {
                        page.new_entry.update(cx, |input, cx| input.set_text("", cx));
                        // Marketplace eklemek kurulabilir listeyi de değiştiriyor.
                        page.available_loaded = false;
                        page.reload(cx);
                    }
                    Err(err) => {
                        page.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
    }

    fn render_tabs(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let active = self.tab;
        let mut row = div()
            .flex()
            .flex_row()
            .gap(px(4.0))
            .mb(px(12.0));

        for (index, tab) in Tab::ALL.into_iter().enumerate() {
            let selected = tab == active;
            row = row.child(
                div()
                    .id(("ext-tab", index))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .when(selected, |el| {
                        el.bg(crate::theme::card_selected_bg())
                            .text_color(theme.text)
                    })
                    .when(!selected, |el| el.text_color(theme.text_muted))
                    .child(SharedString::from(tab.label()))
                    .on_click(cx.listener(move |page, _, _, cx| page.select_tab(tab, cx))),
            );
        }
        row.into_any_element()
    }

    fn row_shell(theme: &Theme) -> gpui::Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(theme.border.opacity(0.5))
    }

    fn render_body(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let query = self.query(cx);
        if self.loading {
            return crate::popover::skeleton_rows("ext-skeleton", theme, 5, cx.entity_id(), cx);
        }
        if let Some(error) = &self.error {
            return div()
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(theme.danger)
                .child(error.clone())
                .into_any_element();
        }

        let busy = self.busy.clone();
        // `size_full` + `overflow_y_scroll`: arşiv sayfasının deseni. Yalnızca
        // `overflow_y_scroll` vermek yetmiyor — kutu içeriği kadar uzuyor ve
        // taşma hiç oluşmadığı için kaydırma da olmuyor.
        let mut list = div()
            .id("ext-list")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll();

        match self.tab {
            Tab::Installed | Tab::Available => {
                let installed = self.tab == Tab::Installed;
                let source = if installed {
                    &self.lists.installed
                } else {
                    &self.lists.available
                };
                let rows = filter_plugins(source, &query);
                if rows.is_empty() {
                    return empty_note(theme, crate::i18n::t("Nothing here yet."));
                }
                for (index, plugin) in rows.into_iter().enumerate() {
                    let id = plugin.id.clone();
                    let working = busy.as_deref() == Some(id.as_str());
                    let enabled = plugin.enabled.unwrap_or(true);

                    let mut meta = div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(11.0))
                        .text_color(theme.text_faint);
                    if let Some(version) = &plugin.version {
                        meta = meta.child(SharedString::from(version.clone()));
                    }
                    if let Some(count) = plugin.install_count {
                        meta = meta.child(SharedString::from(short_count(count)));
                    }
                    if let Some(market) = &plugin.marketplace {
                        meta = meta.child(SharedString::from(market.clone()));
                    }
                    if !plugin.mcp_server_names.is_empty() {
                        meta = meta.child(SharedString::from(format!(
                            "MCP: {}",
                            plugin.mcp_server_names.join(", ")
                        )));
                    }

                    let action_id = id.clone();
                    let toggle_id = id.clone();
                    list = list.child(
                        Self::row_shell(theme)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(theme.text)
                                            .truncate()
                                            .child(SharedString::from(id.clone())),
                                    )
                                    .when_some(plugin.description.clone(), |el, d| {
                                        el.child(
                                            div()
                                                .text_size(px(11.5))
                                                .text_color(theme.text_muted)
                                                .truncate()
                                                .child(SharedString::from(d)),
                                        )
                                    })
                                    .child(meta),
                            )
                            // Kurulu eklentide aç/kapa; kurulabilir olanda kur.
                            .when(installed, |el| {
                                el.child(
                                    crate::popover::btn_ghost(
                                        theme,
                                        if enabled {
                                            crate::i18n::t("Disable")
                                        } else {
                                            crate::i18n::t("Enable")
                                        },
                                        format!("ext-toggle-{index}"),
                                    )
                                    .id(("ext-toggle", index))
                                    .on_click(cx.listener(move |page, _, _, cx| {
                                        page.act(
                                            methods::PLUGIN_SET_ENABLED,
                                            toggle_id.clone(),
                                            !enabled,
                                            cx,
                                        );
                                    })),
                                )
                            })
                            .child(
                                crate::popover::btn_ghost(
                                    theme,
                                    if working {
                                        crate::i18n::t("Working…")
                                    } else if installed {
                                        crate::i18n::t("Uninstall")
                                    } else {
                                        crate::i18n::t("Install")
                                    },
                                    format!("ext-action-{index}"),
                                )
                                .id(("ext-action", index))
                                .on_click(cx.listener(move |page, _, _, cx| {
                                    let method = if installed {
                                        methods::PLUGIN_UNINSTALL
                                    } else {
                                        methods::PLUGIN_INSTALL
                                    };
                                    page.act(method, action_id.clone(), false, cx);
                                })),
                            ),
                    );
                }
            }
            Tab::Marketplaces => {
                if self.lists.marketplaces.is_empty() {
                    return empty_note(theme, crate::i18n::t("No marketplaces added."));
                }
                for (index, market) in self.lists.marketplaces.clone().into_iter().enumerate() {
                    let name = market.name.clone();
                    list = list.child(
                        Self::row_shell(theme)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(theme.text)
                                            .child(SharedString::from(name.clone())),
                                    )
                                    .when_some(market.source.clone(), |el, source| {
                                        el.child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_faint)
                                                .truncate()
                                                .child(SharedString::from(source)),
                                        )
                                    }),
                            )
                            .child(
                                crate::popover::btn_ghost(
                                    theme,
                                    crate::i18n::t("Remove"),
                                    format!("market-remove-{index}"),
                                )
                                .id(("market-remove", index))
                                .on_click(cx.listener(move |page, _, _, cx| {
                                    page.act(
                                        methods::MARKETPLACE_REMOVE,
                                        name.clone(),
                                        false,
                                        cx,
                                    );
                                })),
                            ),
                    );
                }
            }
            Tab::Mcp => {
                if self.lists.mcp.is_empty() {
                    return empty_note(theme, crate::i18n::t("No MCP servers configured."));
                }
                for (index, name) in self.lists.mcp.clone().into_iter().enumerate() {
                    let for_remove = name.clone();
                    list = list.child(
                        Self::row_shell(theme)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.0))
                                    .text_color(theme.text)
                                    .truncate()
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(
                                crate::popover::btn_ghost(
                                    theme,
                                    crate::i18n::t("Remove"),
                                    format!("mcp-remove-{index}"),
                                )
                                .id(("mcp-remove", index))
                                .on_click(cx.listener(move |page, _, _, cx| {
                                    page.act(
                                        methods::REMOVE_MCP_SERVER,
                                        for_remove.clone(),
                                        false,
                                        cx,
                                    );
                                })),
                            ),
                    );
                }
            }
            Tab::Skills => {
                let rows = filter_skills(&self.lists.skills, &query);
                if rows.is_empty() {
                    return empty_note(theme, crate::i18n::t("No skills installed."));
                }
                for (index, skill) in rows.into_iter().enumerate() {
                    let name = skill.name.clone();
                    // Eklentiden gelen skill'ler eklentiyle birlikte gidiyor;
                    // tek tek silmek yapılandırmayı tutarsız bırakırdı.
                    let removable = skill.source == "user";
                    list = list.child(
                        Self::row_shell(theme)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(theme.text)
                                            .child(SharedString::from(name.clone())),
                                    )
                                    .when_some(skill.description.clone(), |el, d| {
                                        el.child(
                                            div()
                                                .text_size(px(11.5))
                                                .text_color(theme.text_muted)
                                                .truncate()
                                                .child(SharedString::from(d)),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_faint)
                                            .child(SharedString::from(skill.source.clone())),
                                    ),
                            )
                            .when(removable, |el| {
                                el.child(
                                    crate::popover::btn_ghost(
                                        theme,
                                        crate::i18n::t("Delete"),
                                        format!("skill-delete-{index}"),
                                    )
                                    .id(("skill-delete", index))
                                    .on_click(cx.listener(move |page, _, _, cx| {
                                        page.act(methods::SKILL_DELETE, name.clone(), false, cx);
                                    })),
                                )
                            }),
                    );
                }
            }
        }

        list.into_any_element()
    }
}

fn empty_note(theme: &Theme, text: &str) -> AnyElement {
    div()
        .p(px(12.0))
        .text_size(px(12.0))
        .text_color(theme.text_faint)
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

impl Render for ExtensionsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let tabs = self.render_tabs(&theme, cx);
        let tab = self.tab;
        let body = self.render_body(&theme, cx);

        // Arama yalnızca listelerde anlamlı; marketplace listesi kısa ve
        // zaten tamamı ekrana sığıyor.
        let search = matches!(tab, Tab::Installed | Tab::Available | Tab::Skills).then(|| {
            crate::popover::search_input_frame(
                &theme,
                self.search.clone().into_any_element(),
            )
        });

        // Marketplace eklemek ve skill oluşturmak aynı satırı paylaşıyor:
        // ikisi de tek bir metin alıp bir düğmeyle çalışıyor.
        let adder = matches!(tab, Tab::Marketplaces | Tab::Skills | Tab::Mcp).then(|| {
            let busy = self.busy.is_some();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .mb(px(8.0))
                .child(
                    div().flex_1().min_w_0().child(
                        crate::popover::search_input_frame(
                            &theme,
                            self.new_entry.clone().into_any_element(),
                        ),
                    ),
                )
                .child(
                    crate::popover::btn_primary(
                        &theme,
                        if busy {
                            crate::i18n::t("Working…")
                        } else if tab == Tab::Skills {
                            crate::i18n::t("Create")
                        } else if tab == Tab::Mcp {
                            crate::i18n::t("Add server")
                        } else {
                            crate::i18n::t("Add")
                        },
                    )
                    .id("ext-add")
                    .on_click(cx.listener(|page, _, _, cx| page.submit_new_entry(cx))),
                )
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .child(tabs)
            .when_some(search, |el, search| el.child(search))
            .when_some(adder, |el, adder| el.child(adder))
            // `min_h_0` olmadan liste içeriği kadar uzuyor ve kaydırma hiç
            // devreye girmiyordu — kullanıcı raporu: "aşağı kaydırılmıyor".
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(id: &str, description: Option<&str>) -> Plugin {
        Plugin {
            id: id.into(),
            version: None,
            scope: None,
            enabled: None,
            description: description.map(str::to_string),
            install_path: None,
            marketplace: None,
            install_count: None,
            mcp_server_names: Vec::new(),
        }
    }

    #[test]
    fn arama_kimlik_ve_aciklamayi_birlikte_tariyor() {
        let plugins = vec![
            plugin("context7@official", Some("Up-to-date library docs")),
            plugin("figbridge@design", Some("Figma köprüsü")),
        ];

        // Kullanıcı çoğu zaman adı değil ne işe yaradığını hatırlıyor.
        let hits = filter_plugins(&plugins, "docs");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "context7@official");

        // Kimlikten de bulunmalı ve arama büyük/küçük harf duyarsız.
        assert_eq!(filter_plugins(&plugins, "FIGBRIDGE").len(), 1);

        // Boş sorgu her şeyi geçiriyor; boşluk da boş sayılıyor.
        assert_eq!(filter_plugins(&plugins, "").len(), 2);
        assert_eq!(filter_plugins(&plugins, "   ").len(), 2);

        // Eşleşme yoksa boş.
        assert!(filter_plugins(&plugins, "yok").is_empty());
    }

    #[test]
    fn skill_aramasi_ayni_kurallarla_calisiyor() {
        let skills = vec![
            Skill {
                name: "dataviz".into(),
                description: Some("Grafik çizer".into()),
                source: "user".into(),
                path: "/x".into(),
            },
            Skill {
                name: "pdf".into(),
                description: None,
                source: "anthropic-skills@inline".into(),
                path: "/y".into(),
            },
        ];
        assert_eq!(filter_skills(&skills, "grafik")[0].name, "dataviz");
        assert_eq!(filter_skills(&skills, "PDF")[0].name, "pdf");
        // Açıklaması olmayan kayıt aramada panik etmemeli.
        assert!(filter_skills(&skills, "yok").is_empty());
    }

    #[test]
    fn mcp_girdisi_uzak_ve_yerel_olarak_ayristiriliyor() {
        // Uzak: http(s) ile başlayan hedef.
        let (name, transport, target, args) =
            parse_mcp_entry("ctx7=https://mcp.example/sse").unwrap();
        assert_eq!(name, "ctx7");
        assert_eq!(transport, "http");
        assert_eq!(target, "https://mcp.example/sse");
        assert!(args.is_empty());

        // Yerel: komut ve argümanları.
        let (name, transport, target, args) =
            parse_mcp_entry("local=npx -y some-server").unwrap();
        assert_eq!(name, "local");
        assert_eq!(transport, "stdio");
        assert_eq!(target, "npx");
        assert_eq!(args, vec!["-y", "some-server"]);

        // Boşluklar kırpılıyor.
        let (name, _, target, _) = parse_mcp_entry("  pad  =  ./run.sh  ").unwrap();
        assert_eq!(name, "pad");
        assert_eq!(target, "./run.sh");
    }

    #[test]
    fn bozuk_mcp_girdisi_aga_gitmeden_reddediliyor() {
        // Eşittir yoksa ne ad ne hedef belli.
        assert!(parse_mcp_entry("sadece-isim").is_err());
        assert!(parse_mcp_entry("=komut").is_err());
        assert!(parse_mcp_entry("ad=").is_err());
        assert!(parse_mcp_entry("ad=   ").is_err());
    }

    #[test]
    fn kurulum_sayisi_kisaltiliyor() {
        assert_eq!(short_count(0), "0");
        assert_eq!(short_count(999), "999");
        assert_eq!(short_count(1_000), "1.0k");
        assert_eq!(short_count(2_563), "2.6k");
    }
}
