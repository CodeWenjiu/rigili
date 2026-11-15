use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::CommentSort;
use rigili_bilibili::{Comment, CommentPage};
use rigili_core::backend;
use rigili_core::runtime::spawn_api;
use rigili_core::{
    api_error_message, api_task_error_message, format_count, format_date, message_markdown,
};

use super::{
    COMMENT_AVATAR, COMMENT_MAX_LEN, InfoPanel, NESTED_COMMENT_AVATAR, NESTED_COMMENT_LIMIT,
};

impl InfoPanel {
    pub(super) fn reset_comments(&mut self) {
        self.comments.clear();
        self.comments_loading = false;
        self.comments_loaded = false;
        self.comments_more = false;
        self.comments_page = 1;
        self.comments_total = 0;
        self.comments_error = None;
        self.comments_more_loading = false;
        self.comments_more_error = None;
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(super) fn load_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments_loading {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.comments_loading = false;
                self.comments_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };
        let aid = details.aid;
        self.comments_loading = true;
        self.comments_loaded = false;
        self.comments_error = None;
        self.comments_more_error = None;
        self.comments.clear();
        self.comments_more = false;
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();

        spawn_api(
            cx,
            async move {
                backend
                    .comments(1, &aid.to_string(), 1, CommentSort::Hot)
                    .await
            },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.comments_loading = false;
                match result {
                    Ok(page) => {
                        let CommentPage {
                            page: info,
                            replies,
                        } = page;
                        this.comments = replies.unwrap_or_default();
                        this.comments_page = if info.num == 0 { 1 } else { info.num };
                        this.comments_total = info.count.max(info.acount);
                        this.comments_more = !this.comments.is_empty()
                            && (this.comments.len() as u64) < this.comments_total;
                        this.comments_loaded = true;
                    }
                    Err(error) => this.comments_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn load_more_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments_loading || self.comments_more_loading || !self.comments_more {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let Ok(backend) = backend::get(cx) else {
            return;
        };
        let aid = details.aid;
        let requested = self.comments_page + 1;
        self.comments_more_loading = true;
        self.comments_more_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move {
                backend
                    .comments(1, &aid.to_string(), requested, CommentSort::Hot)
                    .await
            },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.comments_more_loading = false;
                match result {
                    Ok(page) => {
                        let CommentPage {
                            page: info,
                            replies,
                        } = page;
                        this.comments.extend(replies.unwrap_or_default());
                        this.comments_page = if info.num == 0 { requested } else { info.num };
                        this.comments_total = this.comments_total.max(info.count.max(info.acount));
                        this.comments_more = this.comments_more
                            && (this.comments.len() as u64) < this.comments_total;
                    }
                    Err(error) => {
                        this.comments_more_error = Some(api_task_error_message(error, cx))
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn render_comments_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = self.comment_draft(window, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_comment_composer(&draft, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("player-comments-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.comments_scroll)
                            .child(self.render_comments(cx)),
                    )
                    .child(Scrollbar::vertical(&self.comments_scroll)),
            )
            .into_any_element()
    }

    fn comment_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        if let Some(draft) = &self.draft {
            return draft.clone();
        }
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .submit_on_enter(true)
                .placeholder("发一条友善的评论")
        });
        let sub =
            cx.subscribe_in(
                &draft,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if this.comment_error.is_some() {
                            this.comment_error = None;
                        }
                        cx.notify();
                    }
                    InputEvent::PressEnter { shift: false, .. } => this.submit_comment(window, cx),
                    _ => {}
                },
            );
        self.draft = Some(draft.clone());
        self._draft_sub = Some(sub);
        draft
    }

    fn render_comment_composer(
        &self,
        draft: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let border = cx.theme().border;
        let logged_in = backend::get(cx).is_ok_and(|backend| backend.has_session());

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

        v_flex()
            .flex_shrink_0()
            .gap_2()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(border)
            .child(Textarea::new(draft).w_full())
            .when_some(self.comment_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(if over { danger } else { muted })
                            .child(if count == 0 {
                                "Enter 发布，Shift+Enter 换行".to_owned()
                            } else {
                                format!("{count}/{COMMENT_MAX_LEN}")
                            }),
                    )
                    .child(
                        Button::new("comment-submit")
                            .small()
                            .primary()
                            .disabled(self.comment_sending || content.trim().is_empty() || over)
                            .label(if self.comment_sending {
                                "发布中…"
                            } else {
                                "发布"
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit_comment(window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn submit_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.comment_sending {
            return;
        }
        let Some(draft) = self.draft.clone() else {
            return;
        };
        let content = draft.read(cx).value().trim().to_owned();
        if content.is_empty() {
            self.comment_error = Some("请输入评论内容".into());
            cx.notify();
            return;
        }
        if content.chars().count() > COMMENT_MAX_LEN {
            self.comment_error = Some(format!("评论不能超过 {COMMENT_MAX_LEN} 字"));
            cx.notify();
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.comment_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };

        let aid = details.aid;
        let window_handle = window.window_handle();
        self.comment_sending = true;
        self.comment_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { backend.add_comment(aid, &content).await },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.comment_sending = false;
                match result {
                    Ok(()) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            draft.update(cx, |state, cx| state.set_value("", window, cx));
                            window.push_notification(Notification::success("评论已发布"), cx);
                        });
                        this.comment_count = this.comment_count.saturating_add(1);
                        this.load_comments(cx);
                    }
                    Err(error) => {
                        this.comment_error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }

    fn render_comments(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let border = cx.theme().border;

        let initial: AnyElement = if self.comments_loading {
            h_flex()
                .py_6()
                .justify_center()
                .child(Spinner::new())
                .into_any_element()
        } else if let Some(error) = self.comments_error.clone() {
            v_flex()
                .items_center()
                .gap_2()
                .py_6()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load_comments(cx))),
                )
                .into_any_element()
        } else if self.comments_loaded && self.comments.is_empty() {
            h_flex()
                .py_6()
                .justify_center()
                .text_sm()
                .text_color(muted)
                .child("暂无评论")
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let rows = self
            .comments
            .iter()
            .map(|comment| {
                div()
                    .border_b_1()
                    .border_color(border)
                    .pb_3()
                    .child(self.render_comment(comment, 0, cx))
            })
            .collect::<Vec<_>>();

        let footer: AnyElement = if self.comments_more_loading {
            h_flex()
                .py_2()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = self.comments_more_error.clone() {
            v_flex()
                .items_center()
                .gap_2()
                .py_2()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-more-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_comments(cx))),
                )
                .into_any_element()
        } else if self.comments_more {
            h_flex()
                .justify_center()
                .child(
                    Button::new("comment-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_comments(cx))),
                )
                .into_any_element()
        } else if self.comments_loaded && !self.comments.is_empty() {
            h_flex()
                .py_2()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("没有更多评论")
                .into_any_element()
        } else {
            div().into_any_element()
        };

        v_flex()
            .gap_4()
            .p_4()
            .pb_6()
            .child(initial)
            .children(rows)
            .child(footer)
            .into_any_element()
    }

    fn render_comment(&self, comment: &Comment, depth: usize, cx: &App) -> AnyElement {
        let primary = cx.theme().primary;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let member = &comment.member;
        let level = member.level_info.current_level;
        let time = if comment.reply_control.time_desc.is_empty() {
            format_date(comment.ctime)
        } else {
            comment.reply_control.time_desc.clone()
        };
        let avatar = if depth == 0 {
            COMMENT_AVATAR
        } else {
            NESTED_COMMENT_AVATAR
        };
        let has_emotes = comment
            .content
            .emote
            .as_ref()
            .is_some_and(|emotes| !emotes.is_empty());
        let message = if has_emotes {
            TextView::markdown(
                ("comment-message", comment.rpid),
                message_markdown(&comment.content),
            )
            .text_sm()
            .line_height(relative(1.5))
            .text_color(foreground)
            .selectable(false)
            .into_any_element()
        } else {
            div()
                .text_sm()
                .line_height(relative(1.5))
                .child(comment.content.message.clone())
                .into_any_element()
        };
        let nested = comment
            .replies()
            .iter()
            .take(NESTED_COMMENT_LIMIT)
            .map(|reply| self.render_comment(reply, depth + 1, cx))
            .collect::<Vec<_>>();

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(
                        Avatar::new()
                            .src(member.avatar_url())
                            .name(member.uname.clone())
                            .size(avatar),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(primary)
                                            .child(member.uname.clone()),
                                    )
                                    .when(level > 0, |this| {
                                        this.child(
                                            div()
                                                .px_1()
                                                .rounded(px(3.))
                                                .bg(primary.opacity(0.12))
                                                .text_color(primary)
                                                .text_xs()
                                                .child(format!("LV{level}")),
                                        )
                                    })
                                    .child(div().text_xs().text_color(muted).child(time)),
                            )
                            .child(message)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::new(IconName::ThumbsUp).with_size(px(14.)))
                                            .child(format_count(comment.like)),
                                    )
                                    .when(comment.rcount > 0, |this| {
                                        this.child(format!(
                                            "{} 条回复",
                                            format_count(comment.rcount)
                                        ))
                                    }),
                            ),
                    ),
            )
            .when(!nested.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(36.)).children(nested))
            })
            .into_any_element()
    }
}
