use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::MineView;
use rigili_core::backend;
use rigili_core::runtime::spawn_api;

use crate::error::api_task_error_message;

pub(super) struct CreateFolderDialog {
    input: Entity<InputState>,
    private: bool,
    submitting: bool,
    error: Option<String>,
    mine: Entity<MineView>,
    window: AnyWindowHandle,
    focus_once: bool,
    _sub: Subscription,
}

impl CreateFolderDialog {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>, mine: Entity<MineView>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("收藏夹名称"));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.submit(cx);
            }
        });

        Self {
            input,
            private: false,
            submitting: false,
            error: None,
            mine,
            window: window.window_handle(),
            focus_once: true,
            _sub: sub,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.submitting {
            return;
        }
        let title = self.input.read(cx).value().trim().to_owned();
        if title.is_empty() {
            self.error = Some("请输入收藏夹名称".into());
            cx.notify();
            return;
        }
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };

        let private = self.private;
        let mine = self.mine.clone();
        let window = self.window;
        self.submitting = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.create_folder(&title, private).await },
            move |this, result, cx| match result {
                Ok(_) => {
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

impl Render for CreateFolderDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;

        if self.focus_once {
            self.focus_once = false;
            let input = self.input.clone();
            window.defer(cx, move |window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
            });
        }

        v_flex()
            .gap_4()
            .child(Input::new(&self.input).large())
            .child(
                Checkbox::new("create-folder-private")
                    .label("私密收藏夹")
                    .checked(self.private)
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.private = *checked;
                        cx.notify();
                    })),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("create-folder-cancel")
                            .small()
                            .label("取消")
                            .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                    )
                    .child(
                        Button::new("create-folder-submit")
                            .small()
                            .primary()
                            .disabled(self.submitting)
                            .label(if self.submitting {
                                "创建中…"
                            } else {
                                "创建"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
