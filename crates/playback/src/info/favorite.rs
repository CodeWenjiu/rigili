use std::collections::BTreeSet;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_bilibili::{Client, FavFolder};
use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::{api_error_message, api_task_error_message};

use super::InfoPanel;

const LIST_MAX_HEIGHT: Pixels = px(320.);

pub(super) struct FavoriteDialog {
    aid: u64,
    panel: Entity<InfoPanel>,
    window: AnyWindowHandle,
    folders: Vec<FavFolder>,
    selected: BTreeSet<u64>,
    original: BTreeSet<u64>,
    pending_selection: Option<u64>,
    input: Entity<InputState>,
    show_create: bool,
    private: bool,
    loading: bool,
    loaded: bool,
    saving: bool,
    creating: bool,
    error: Option<String>,
    _sub: Subscription,
}

impl FavoriteDialog {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        aid: u64,
        panel: Entity<InfoPanel>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("收藏夹名称"));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.create_folder(cx);
            }
        });

        let mut dialog = Self {
            aid,
            panel,
            window: window.window_handle(),
            folders: Vec::new(),
            selected: BTreeSet::new(),
            original: BTreeSet::new(),
            pending_selection: None,
            input,
            show_create: false,
            private: false,
            loading: true,
            loaded: false,
            saving: false,
            creating: false,
            error: None,
            _sub: sub,
        };
        dialog.load(cx);
        dialog
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.require_client(cx) else {
            self.loading = false;
            return;
        };
        let aid = self.aid;
        self.loading = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move {
                let nav = client.nav().await?;
                client.fav_folders_with_state(nav.mid, aid).await
            },
            move |this, result, cx| {
                this.loading = false;
                match result {
                    Ok(folders) => {
                        this.folders = folders
                            .into_items()
                            .into_iter()
                            .filter(|folder| folder.is_valid())
                            .collect();
                        let original = this
                            .folders
                            .iter()
                            .filter(|folder| folder.is_favorited())
                            .map(|folder| folder.id)
                            .collect::<BTreeSet<_>>();
                        let mut selected = original.clone();
                        if let Some(id) = this.pending_selection.take() {
                            selected.insert(id);
                        }
                        this.original = original;
                        this.selected = selected;
                        this.loaded = true;
                    }
                    Err(error) => this.error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn begin_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_create = true;
        self.error = None;
        let input = self.input.clone();
        window.defer(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
        });
        cx.notify();
    }

    fn create_folder(&mut self, cx: &mut Context<Self>) {
        if self.creating {
            return;
        }
        let title = self.input.read(cx).value().trim().to_owned();
        if title.is_empty() {
            self.error = Some("请输入收藏夹名称".into());
            cx.notify();
            return;
        }
        let Some(client) = self.require_client(cx) else {
            return;
        };

        let private = self.private;
        self.creating = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.create_folder(&title, private).await },
            move |this, result, cx| {
                this.creating = false;
                match result {
                    Ok(created) => {
                        this.show_create = false;
                        this.private = false;
                        this.clear_input(cx);
                        this.pending_selection = Some(created.id);
                        this.load(cx);
                    }
                    Err(error) => {
                        this.error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || !self.loaded {
            return;
        }
        let added = self
            .selected
            .difference(&self.original)
            .copied()
            .collect::<Vec<_>>();
        let removed = self
            .original
            .difference(&self.selected)
            .copied()
            .collect::<Vec<_>>();
        if added.is_empty() && removed.is_empty() {
            self.close(cx);
            return;
        }
        let Some(client) = self.require_client(cx) else {
            return;
        };

        let aid = self.aid;
        let favorited = !self.selected.is_empty();
        let panel = self.panel.clone();
        let window = self.window;
        self.saving = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move {
                for folder in added {
                    client.set_favorite(aid, folder, true).await?;
                }
                for folder in removed {
                    client.set_favorite(aid, folder, false).await?;
                }
                Ok(favorited)
            },
            move |this, result, cx| {
                this.saving = false;
                match result {
                    Ok(favorited) => {
                        panel.update(cx, |panel, cx| panel.set_favorited(favorited, cx));
                        let _ = window.update(cx, |_, window, cx| window.close_dialog(cx));
                    }
                    Err(error) => {
                        this.error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }

    fn close(&self, cx: &mut Context<Self>) {
        let window = self.window;
        let _ = window.update(cx, |_, window, cx| window.close_dialog(cx));
    }

    fn clear_input(&self, cx: &mut Context<Self>) {
        let input = self.input.clone();
        let window = self.window;
        let _ = window.update(cx, |_, window, cx| {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        });
    }

    fn require_client(&mut self, cx: &mut Context<Self>) -> Option<Arc<Client>> {
        match client::get(cx) {
            Ok(client) => Some(client),
            Err(error) => {
                self.error = Some(api_error_message(error));
                self.loading = false;
                cx.notify();
                None
            }
        }
    }

    fn toggle(&mut self, id: u64, checked: bool, cx: &mut Context<Self>) {
        if checked {
            self.selected.insert(id);
        } else {
            self.selected.remove(&id);
        }
        cx.notify();
    }

    fn folder_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let radius = cx.theme().radius;

        v_flex()
            .gap_1()
            .children(self.folders.iter().map(|folder| {
                let id = folder.id;
                let title: SharedString = folder.title.clone().into();
                let meta: SharedString = format!(
                    "{} · {}个内容",
                    if folder.is_private() {
                        "私密"
                    } else {
                        "公开"
                    },
                    folder.media_count
                )
                .into();

                Checkbox::new(("favorite-folder", id))
                    .small()
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded(radius)
                    .hover(move |this| this.bg(accent))
                    .label(title)
                    .checked(self.selected.contains(&id))
                    .disabled(self.saving)
                    .on_change(cx.listener(move |this, checked, _, cx| {
                        this.toggle(id, *checked, cx);
                    }))
                    .child(div().text_xs().text_color(muted).child(meta))
            }))
            .into_any_element()
    }

    fn create_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let radius = cx.theme().radius;

        v_flex()
            .gap_2()
            .p_2()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .child(Input::new(&self.input).small())
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Checkbox::new("favorite-new-private")
                            .small()
                            .label("私密收藏夹")
                            .checked(self.private)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                this.private = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("favorite-new-cancel")
                                    .ghost()
                                    .small()
                                    .label("取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_create = false;
                                        this.private = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("favorite-new-submit")
                                    .primary()
                                    .small()
                                    .disabled(self.creating)
                                    .label(if self.creating {
                                        "创建中…"
                                    } else {
                                        "创建"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| this.create_folder(cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for FavoriteDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        let body: AnyElement = if self.loading {
            v_flex()
                .items_center()
                .gap_2()
                .py_6()
                .child(Spinner::new())
                .child(div().text_sm().text_color(muted).child("正在加载收藏夹…"))
                .into_any_element()
        } else if !self.loaded {
            v_flex()
                .items_center()
                .gap_2()
                .py_6()
                .child(div().text_sm().text_color(muted).child("收藏夹加载失败"))
                .child(
                    Button::new("favorite-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                )
                .into_any_element()
        } else if self.folders.is_empty() {
            div()
                .py_6()
                .text_sm()
                .text_color(muted)
                .text_center()
                .child("还没有收藏夹，可在下方新建")
                .into_any_element()
        } else {
            div()
                .id("favorite-folder-list")
                .max_h(LIST_MAX_HEIGHT)
                .overflow_y_scroll()
                .child(self.folder_rows(cx))
                .into_any_element()
        };

        v_flex()
            .gap_3()
            .child(body)
            .when(self.show_create && !self.loading, |this| {
                this.child(self.create_form(cx))
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("favorite-new")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .label("新建收藏夹")
                            .disabled(self.loading || self.saving || self.creating)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_create(window, cx);
                            })),
                    )
                    .child(
                        Button::new("favorite-save")
                            .primary()
                            .small()
                            .disabled(self.loading || self.saving || !self.loaded)
                            .label(if self.saving {
                                "保存中…"
                            } else {
                                "保存"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
    }
}
