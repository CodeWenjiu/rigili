use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::FavFolder;

use super::MineView;
use rigili_core::backend;
use rigili_core::runtime::spawn_api;

use rigili_core::api_task_error_message;

pub(super) struct SortFoldersDialog {
    folders: Vec<FavFolder>,
    default_id: Option<u64>,
    submitting: bool,
    error: Option<String>,
    mine: Entity<MineView>,
    window: AnyWindowHandle,
}

impl SortFoldersDialog {
    pub(super) fn new(
        window: &mut Window,
        mine: Entity<MineView>,
        folders: Vec<FavFolder>,
        default_id: Option<u64>,
    ) -> Self {
        Self {
            folders,
            default_id,
            submitting: false,
            error: None,
            mine,
            window: window.window_handle(),
        }
    }

    fn move_up(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 {
            return;
        }
        self.folders.swap(index, index - 1);
        cx.notify();
    }

    fn move_down(&mut self, index: usize, cx: &mut Context<Self>) {
        if index + 1 >= self.folders.len() {
            return;
        }
        self.folders.swap(index, index + 1);
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        let mut ids = self.default_id.into_iter().collect::<Vec<_>>();
        ids.extend(self.folders.iter().map(|folder| folder.id));
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };

        let mine = self.mine.clone();
        let window = self.window;
        self.submitting = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.sort_folders(&ids).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    mine.update(cx, |mine, cx| mine.folders_changed(cx));
                    let _ = window.update(cx, |_, window, cx| window.close_dialog(cx));
                }
                Err(error) => {
                    this.submitting = false;
                    this.error = Some(api_task_error_message(error, cx));
                    cx.notify();
                }
            },
        );
    }
}

impl Render for SortFoldersDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let hover = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;
        let count = self.folders.len();

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("使用箭头调整收藏夹顺序"),
            )
            .child(
                div()
                    .id("sort-folder-list")
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .gap_1()
                            .children(self.folders.iter().enumerate().map(|(index, folder)| {
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_1()
                                    .rounded(radius)
                                    .hover(move |this| this.bg(hover))
                                    .child(
                                        div()
                                            .w(px(20.))
                                            .text_xs()
                                            .text_color(muted)
                                            .child((index + 1).to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_sm()
                                            .child(folder.title.clone()),
                                    )
                                    .child(
                                        Button::new(("sort-folder-up", index))
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::ChevronUp)
                                            .accessibility_label("上移")
                                            .disabled(index == 0)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_up(index, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(("sort-folder-down", index))
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::ChevronDown)
                                            .accessibility_label("下移")
                                            .disabled(index + 1 >= count)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_down(index, cx);
                                            })),
                                    )
                            })),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("sort-folder-cancel")
                            .small()
                            .label("取消")
                            .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                    )
                    .child(
                        Button::new("sort-folder-save")
                            .small()
                            .primary()
                            .disabled(self.submitting)
                            .label(if self.submitting {
                                "保存中…"
                            } else {
                                "保存"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
    }
}
