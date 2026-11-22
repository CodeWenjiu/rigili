use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::CommentReply;
use rigili_bilibili::{is_emote_image, normalize_url};
use rigili_core::client;
use rigili_core::emotes;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_core::{api_error_message, api_task_error_message};

use super::super::{COMMENT_MAX_LEN, InfoPanel};

impl InfoPanel {
    fn load_emotes(&mut self, cx: &mut Context<Self>) {
        if !emotes::start(cx) {
            return;
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                let message = api_error_message(error);
                emotes::fail(cx, message);
                cx.notify();
                return;
            }
        };
        cx.notify();

        spawn_api(
            cx,
            async move { client.comment_emotes().await },
            move |_this, result, cx| {
                match result {
                    Ok(panel) => emotes::succeed(cx, panel),
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        emotes::fail(cx, message);
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn set_emote_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.close_emote_picker();
            cx.notify();
            return;
        }
        self.emote_open = true;
        self.load_emotes(cx);
        cx.notify();
    }

    pub(super) fn close_emote_picker(&mut self) {
        self.emote_open = false;
    }

    fn insert_emote(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(window, cx);
        draft.update(cx, |state, cx| {
            state.insert(text, window, cx);
            state.focus(window, cx);
        });
        self.composer_error = None;
        cx.notify();
    }

    fn render_emote_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.emote_open;
        let weak = cx.entity().downgrade();
        Popover::new("comment-emote-popover")
            .offset(px(6.))
            .open(open)
            .trigger(
                Button::new("comment-emote")
                    .ghost()
                    .small()
                    .icon(IconName::Sticker)
                    .accessibility_label("表情")
                    .tooltip("表情"),
            )
            .on_open_change(move |open, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.set_emote_picker_open(*open, cx);
                });
            })
            .when(open, |this| this.child(self.render_emote_picker(cx)))
            .into_any_element()
    }

    pub(super) fn render_emote_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        let danger = theme.danger;
        let accent = theme.accent;
        let packages = emotes::packages(cx);
        let selected = self.emote_package.min(packages.len().saturating_sub(1));

        let body: AnyElement = if let Some(package) = packages.get(selected) {
            let emotes = package.emote.as_deref().unwrap_or_default();
            let images = rigili_core::image_cache::emotes(cx);

            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(package.text.clone()),
                )
                .child(
                    h_flex()
                        .id("comment-emote-grid")
                        .flex_wrap()
                        .gap_1()
                        .max_h(px(176.))
                        .overflow_y_scroll()
                        .children(emotes.iter().enumerate().filter_map(|(index, emote)| {
                            if emote.text.is_empty() {
                                return None;
                            }
                            let text = emote.text.clone();
                            let icon = if is_emote_image(&emote.url) {
                                img(normalize_url(&emote.url))
                                    .image_cache(&images)
                                    .size(px(26.))
                                    .object_fit(ObjectFit::Contain)
                                    .into_any_element()
                            } else {
                                div()
                                    .px_1()
                                    .text_xs()
                                    .text_color(foreground)
                                    .child(emote.text.clone())
                                    .into_any_element()
                            };
                            Some(
                                div()
                                    .id((
                                        "comment-emote-item",
                                        (selected as u64) * 1000 + index as u64,
                                    ))
                                    .test_support()
                                    .min_h(px(32.))
                                    .min_w(px(32.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .hover(move |this| this.bg(accent))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.insert_emote(text.clone(), window, cx);
                                    }))
                                    .child(icon),
                            )
                        })),
                )
                .child(
                    h_flex()
                        .id("comment-emote-packages")
                        .gap_1()
                        .overflow_x_scroll()
                        .children(packages.iter().enumerate().map(|(index, package)| {
                            let icon = is_emote_image(&package.url)
                                .then(|| normalize_url(&package.url));
                            div()
                                .id(("comment-emote-package", index as u64))
                                .h(px(28.))
                                .min_w(px(28.))
                                .px_1()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .when(index == selected, |this| this.bg(accent))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.emote_package = index;
                                    cx.notify();
                                }))
                                .child(if let Some(icon) = icon {
                                    img(icon)
                                        .image_cache(&images)
                                        .size(px(24.))
                                        .object_fit(ObjectFit::Contain)
                                        .into_any_element()
                                } else {
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(package.text.clone())
                                        .into_any_element()
                                })
                        })),
                )
                .into_any_element()
        } else if emotes::loading(cx) {
            h_flex()
                .py_4()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = emotes::error(cx) {
            v_flex()
                .items_center()
                .gap_2()
                .py_3()
                .child(div().text_xs().text_color(danger).child(error))
                .child(
                    Button::new("emote-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load_emotes(cx))),
                )
                .into_any_element()
        } else {
            div()
                .py_4()
                .text_xs()
                .text_color(muted)
                .text_center()
                .child("暂无可用表情")
                .into_any_element()
        };

        v_flex().w(px(300.)).child(body).into_any_element()
    }

    pub(crate) fn focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(window, cx);
        window.defer(cx, move |window, cx| {
            draft.update(cx, |state, cx| state.focus(window, cx));
        });
        cx.notify();
    }

    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            draft.read(cx).focus_handle(cx).is_focused(window)
        })
    }

    fn composer_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        if let Some(draft) = &self.draft {
            return draft.clone();
        }
        let placeholder = if self.thread.is_some() {
            "回复评论"
        } else {
            "发一条友善的评论"
        };
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .submit_on_enter(true)
                .placeholder(placeholder)
        });
        let sub =
            cx.subscribe_in(
                &draft,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        this.composer_error = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter { shift: false, .. } => this.submit_composer(window, cx),
                    _ => {}
                },
            );
        self.draft = Some(draft.clone());
        self._draft_sub = Some(sub);
        draft
    }

    pub(super) fn render_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let draft = self.composer_draft(window, cx);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let border = cx.theme().border;
        let logged_in = client::get(cx).is_ok_and(|client| client.has_session());

        if !logged_in {
            return div()
                .flex_shrink_0()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(border)
                .text_xs()
                .text_color(muted)
                .child("登录后可发表评论")
                .into_any_element();
        }

        let content = draft.read(cx).value();
        let count = content.chars().count();
        let over = count > COMMENT_MAX_LEN;
        let sending = self.composer_sending;
        let error = self.composer_error.clone();
        let submit_label = if self.thread.is_some() {
            if sending { "回复中…" } else { "回复" }
        } else if sending {
            "发布中…"
        } else {
            "发布"
        };

        v_flex()
            .flex_shrink_0()
            .gap_2()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .id("comment-draft-input")
                    .key_context(rigili_core::TEXT_INPUT_CONTEXT)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(Textarea::new(&draft).w_full()),
            )
            .when_some(error, |this, error| {
                this.child(div().text_xs().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(self.render_emote_button(cx))
                            .when(count > 0, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(if over { danger } else { muted })
                                        .child(format!("{count}/{COMMENT_MAX_LEN}")),
                                )
                            }),
                    )
                    .child(
                        Button::new("comment-submit")
                            .small()
                            .primary()
                            .disabled(sending || content.trim().is_empty() || over)
                            .label(submit_label)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_composer(window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }


    fn submit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_sending {
            return;
        }
        let Some(draft) = self.draft.clone() else {
            return;
        };
        let content = draft.read(cx).value().trim().to_owned();
        let replying = self.thread.as_ref().map(|thread| {
            let root = thread.root.rpid;
            (root, self.reply_target.unwrap_or(root))
        });
        if content.is_empty() {
            self.composer_error = Some(if replying.is_some() {
                "请输入回复内容"
            } else {
                "请输入评论内容"
            }
            .into());
            cx.notify();
            return;
        }
        if content.chars().count() > COMMENT_MAX_LEN {
            self.composer_error = Some(if replying.is_some() {
                format!("回复不能超过 {COMMENT_MAX_LEN} 字")
            } else {
                format!("评论不能超过 {COMMENT_MAX_LEN} 字")
            });
            cx.notify();
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.composer_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };

        let aid = details.aid;
        let window_handle = window.window_handle();
        self.composer_sending = true;
        self.composer_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move {
                let reply = replying.map(|(root, parent)| CommentReply { root, parent });
                client.add_comment(aid, &content, reply).await
            },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.composer_sending = false;
                match result {
                    Ok(()) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            if let Some(draft) = this.draft.clone() {
                                draft.update(cx, |state, cx| state.set_value("", window, cx));
                            }
                            let message = if replying.is_some() {
                                "回复已发布"
                            } else {
                                "评论已发布"
                            };
                            toast::push(window, cx, Notification::success(message));
                        });
                        match replying {
                            Some((root, _)) => {
                                this.reply_target = None;
                                this.close_emote_picker();
                                if let Some(thread) = &mut this.thread
                                    && thread.root.rpid == root
                                {
                                    thread.root.rcount =
                                        thread.root.rcount.saturating_add(1);
                                    thread.total = thread.total.max(thread.root.rcount);
                                }
                                if let Some(comment) = this
                                    .comments
                                    .iter_mut()
                                    .find(|comment| comment.rpid == root)
                                {
                                    comment.rcount = comment.rcount.saturating_add(1);
                                }
                                this.touch_row_of(root);
                                cx.notify();
                            }
                            None => {
                                this.comment_count = this.comment_count.saturating_add(1);
                                this.load_comments(cx);
                            }
                        }
                    }
                    Err(error) => {
                        this.composer_error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }
}
