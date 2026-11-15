use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
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
use rigili_bilibili::api::comment::{CommentReply, CommentSort};
use rigili_bilibili::{Comment, CommentPage, normalize_url};
use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::{
    api_error_message, api_task_error_message, format_count, format_date, message_markdown,
};

use super::{
    COMMENT_AVATAR, COMMENT_MAX_LEN, InfoEvent, InfoPanel, NESTED_COMMENT_AVATAR,
    NESTED_COMMENT_LIMIT,
};

#[derive(Default)]
pub(super) struct ReplyThread {
    replies: Vec<Comment>,
    page: u32,
    total: u64,
    more: bool,
    loading: bool,
    error: Option<String>,
}

#[derive(Clone)]
pub(super) struct ReplyTarget {
    pub(super) root: u64,
    pub(super) parent: u64,
    pub(super) uname: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmoteTarget {
    Comment,
    Reply,
}

impl EmoteTarget {
    fn key(self) -> (&'static str, u64) {
        match self {
            Self::Comment => ("comment-emote", 0),
            Self::Reply => ("comment-emote", 1),
        }
    }
}

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
        self.reply_threads.clear();
        self.reply_target = None;
        self.reply_draft = None;
        self._reply_draft_sub = None;
        self.reply_sending = false;
        self.reply_error = None;
        self.emote_target = None;
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(super) fn load_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments_loading {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let client = match client::get(cx) {
            Ok(client) => client,
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
        self.reply_threads.clear();
        self.reply_target = None;
        self.reply_error = None;
        self.emote_target = None;
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();

        spawn_api(
            cx,
            async move {
                client
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
        let Ok(client) = client::get(cx) else {
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
                client
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

    fn load_next_replies(&mut self, root: u64, cx: &mut Context<Self>) {
        let page = self
            .reply_threads
            .get(&root)
            .map_or(1, |thread| thread.page + 1);
        self.load_replies(root, page, cx);
    }

    fn load_replies(&mut self, root: u64, page: u32, cx: &mut Context<Self>) {
        let Some(details) = self.details.clone() else {
            return;
        };
        let thread = self.reply_threads.entry(root).or_default();
        if thread.loading {
            return;
        }
        if page == 1
            && thread.replies.is_empty()
            && let Some(comment) = self.comments.iter().find(|comment| comment.rpid == root)
        {
            thread.replies = comment
                .replies()
                .iter()
                .take(NESTED_COMMENT_LIMIT)
                .cloned()
                .collect();
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                thread.error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };
        let aid = details.aid;
        thread.loading = true;
        thread.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.comment_replies(aid, root, page).await },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                let Some(thread) = this.reply_threads.get_mut(&root) else {
                    return;
                };
                thread.loading = false;
                match result {
                    Ok(CommentPage {
                        page: info,
                        replies,
                    }) => {
                        let replies = replies.unwrap_or_default();
                        if page > 1 {
                            thread.replies.extend(replies);
                        } else {
                            thread.replies = replies;
                        }
                        thread.page = if info.num == 0 { page } else { info.num };
                        thread.total = info.count.max(info.acount);
                        thread.more = (thread.replies.len() as u64) < thread.total;
                    }
                    Err(error) => thread.error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn collapse_replies(&mut self, root: u64, cx: &mut Context<Self>) {
        self.reply_threads.remove(&root);
        if self
            .reply_target
            .as_ref()
            .is_some_and(|target| target.root == root)
        {
            self.reply_target = None;
            self.reply_error = None;
            self.close_emote_picker(EmoteTarget::Reply);
        }
        cx.notify();
    }

    fn reply_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        if let Some(draft) = &self.reply_draft {
            return draft.clone();
        }
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .submit_on_enter(true)
                .placeholder("回复评论")
        });
        let sub =
            cx.subscribe_in(
                &draft,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if this.reply_error.is_some() {
                            this.reply_error = None;
                            cx.notify();
                        }
                    }
                    InputEvent::PressEnter { shift: false, .. } => this.submit_reply(window, cx),
                    _ => {}
                },
            );
        self.reply_draft = Some(draft.clone());
        self._reply_draft_sub = Some(sub);
        draft
    }

    fn begin_reply(
        &mut self,
        root: u64,
        parent: u64,
        uname: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_target = self
            .reply_target
            .as_ref()
            .is_some_and(|target| target.parent == parent);
        self.reply_target = Some(ReplyTarget {
            root,
            parent,
            uname,
        });
        self.reply_error = None;
        let draft = self.reply_draft(window, cx);
        if !same_target {
            draft.update(cx, |state, cx| state.set_value("", window, cx));
        }
        window.defer(cx, move |window, cx| {
            draft.update(cx, |state, cx| state.focus(window, cx));
        });
        cx.notify();
    }

    fn submit_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reply_sending {
            return;
        }
        let Some(target) = self.reply_target.clone() else {
            return;
        };
        let Some(draft) = self.reply_draft.clone() else {
            return;
        };
        let content = draft.read(cx).value().trim().to_owned();
        if content.is_empty() {
            self.reply_error = Some("请输入回复内容".into());
            cx.notify();
            return;
        }
        if content.chars().count() > COMMENT_MAX_LEN {
            self.reply_error = Some(format!("回复不能超过 {COMMENT_MAX_LEN} 字"));
            cx.notify();
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.reply_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };

        let aid = details.aid;
        let window_handle = window.window_handle();
        let reply = CommentReply {
            root: target.root,
            parent: target.parent,
        };
        self.reply_sending = true;
        self.reply_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.add_comment(aid, &content, Some(reply)).await },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.reply_sending = false;
                match result {
                    Ok(()) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            if let Some(draft) = this.reply_draft.clone() {
                                draft.update(cx, |state, cx| state.set_value("", window, cx));
                            }
                            window.push_notification(Notification::success("回复已发布"), cx);
                        });
                        this.reply_target = None;
                        this.close_emote_picker(EmoteTarget::Reply);
                        if let Some(comment) = this
                            .comments
                            .iter_mut()
                            .find(|comment| comment.rpid == target.root)
                        {
                            comment.rcount = comment.rcount.saturating_add(1);
                        }
                        this.load_replies(target.root, 1, cx);
                    }
                    Err(error) => {
                        this.reply_error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }

    fn load_emotes(&mut self, cx: &mut Context<Self>) {
        if self.emotes_loading {
            return;
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.emotes_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };
        self.emotes_loading = true;
        self.emotes_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.comment_emotes().await },
            move |this, result, cx| {
                this.emotes_loading = false;
                match result {
                    Ok(panel) => {
                        this.emotes = Some(
                            panel
                                .packages
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|package| {
                                    package.emote.as_ref().is_some_and(|emotes| {
                                        emotes.iter().any(|emote| {
                                            !emote.text.is_empty() && !emote.url.is_empty()
                                        })
                                    })
                                })
                                .collect(),
                        );
                        this.emote_package = 0;
                    }
                    Err(error) => this.emotes_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn toggle_emote_picker(&mut self, target: EmoteTarget, cx: &mut Context<Self>) {
        self.emote_target = (self.emote_target != Some(target)).then_some(target);
        if self.emote_target.is_some() && self.emotes.is_none() && !self.emotes_loading {
            self.load_emotes(cx);
        }
        cx.notify();
    }

    fn close_emote_picker(&mut self, target: EmoteTarget) {
        if self.emote_target == Some(target) {
            self.emote_target = None;
        }
    }

    fn insert_emote(
        &mut self,
        target: EmoteTarget,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = match target {
            EmoteTarget::Comment => self.comment_draft(window, cx),
            EmoteTarget::Reply => {
                let Some(draft) = self.reply_draft.clone() else {
                    return;
                };
                draft
            }
        };
        draft.update(cx, |state, cx| {
            state.insert(text, window, cx);
            state.focus(window, cx);
        });
        match target {
            EmoteTarget::Comment => self.comment_error = None,
            EmoteTarget::Reply => self.reply_error = None,
        }
        cx.notify();
    }

    fn render_emote_button(&self, target: EmoteTarget, cx: &mut Context<Self>) -> AnyElement {
        Button::new(target.key())
            .ghost()
            .small()
            .icon(IconName::Sticker)
            .accessibility_label("表情")
            .tooltip("表情")
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_emote_picker(target, cx)))
            .into_any_element()
    }

    fn render_emote_picker(&self, target: EmoteTarget, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let border = theme.border;
        let accent = theme.accent;
        let radius = theme.radius;

        let body: AnyElement = if self.emotes_loading {
            h_flex()
                .py_4()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = self.emotes_error.clone() {
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
            let packages = self.emotes.as_deref().unwrap_or_default();
            if packages.is_empty() {
                div()
                    .py_4()
                    .text_xs()
                    .text_color(muted)
                    .text_center()
                    .child("暂无可用表情")
                    .into_any_element()
            } else {
                let selected = self.emote_package.min(packages.len() - 1);
                let package = &packages[selected];
                let emotes = package.emote.as_deref().unwrap_or_default();

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
                                if emote.text.is_empty() || emote.url.is_empty() {
                                    return None;
                                }
                                let text = emote.text.clone();
                                Some(
                                    div()
                                        .id((
                                            "comment-emote-item",
                                            (selected as u64) * 1000 + index as u64,
                                        ))
                                        .size(px(32.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .hover(move |this| this.bg(accent))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.insert_emote(target, text.clone(), window, cx);
                                        }))
                                        .child(
                                            img(normalize_url(&emote.url))
                                                .size(px(26.))
                                                .object_fit(ObjectFit::Contain),
                                        ),
                                )
                            })),
                    )
                    .child(
                        h_flex()
                            .id("comment-emote-packages")
                            .gap_1()
                            .overflow_x_scroll()
                            .children(packages.iter().enumerate().map(|(index, package)| {
                                let icon = normalize_url(&package.url);
                                div()
                                    .id(("comment-emote-package", index as u64))
                                    .size(px(28.))
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
                                    .child(if icon.is_empty() {
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(package.text.clone())
                                            .into_any_element()
                                    } else {
                                        img(icon)
                                            .size(px(24.))
                                            .object_fit(ObjectFit::Contain)
                                            .into_any_element()
                                    })
                            })),
                    )
                    .into_any_element()
            }
        };

        v_flex()
            .gap_2()
            .p_2()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(theme.popover)
            .child(body)
            .into_any_element()
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
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(self.render_emote_button(EmoteTarget::Comment, cx))
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
            .when(self.emote_target == Some(EmoteTarget::Comment), |this| {
                this.child(self.render_emote_picker(EmoteTarget::Comment, cx))
            })
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
        let client = match client::get(cx) {
            Ok(client) => client,
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
            async move { client.add_comment(aid, &content, None).await },
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
                    .child(self.render_comment(comment, 0, comment.rpid, cx))
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

    fn render_reply_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let (Some(draft), Some(target)) = (self.reply_draft.as_ref(), self.reply_target.as_ref())
        else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        v_flex()
            .gap_2()
            .pt_1()
            .child(Textarea::new(draft).w_full())
            .when_some(self.reply_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .min_w_0()
                            .child(self.render_emote_button(EmoteTarget::Reply, cx))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format!("回复 @{}", target.uname)),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("reply-cancel")
                                    .ghost()
                                    .small()
                                    .label("取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.reply_target = None;
                                        this.reply_error = None;
                                        this.close_emote_picker(EmoteTarget::Reply);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("reply-submit")
                                    .primary()
                                    .small()
                                    .disabled(self.reply_sending)
                                    .label(if self.reply_sending {
                                        "回复中…"
                                    } else {
                                        "回复"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_reply(window, cx)
                                    })),
                            ),
                    ),
            )
            .when(self.emote_target == Some(EmoteTarget::Reply), |this| {
                this.child(self.render_emote_picker(EmoteTarget::Reply, cx))
            })
            .into_any_element()
    }

    fn render_comment(
        &self,
        comment: &Comment,
        depth: usize,
        root: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let link_hover = cx.theme().link_hover;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let member = &comment.member;
        let mid = member.mid;
        let uname: SharedString = member.uname.clone().into();
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
            div()
                .cursor(CursorStyle::IBeam)
                .child(
                    TextView::markdown(
                        ("comment-message", comment.rpid),
                        message_markdown(&comment.content),
                    )
                    .text_sm()
                    .line_height(relative(1.5))
                    .text_color(foreground),
                )
                .into_any_element()
        } else {
            div()
                .text_sm()
                .line_height(relative(1.5))
                .cursor(CursorStyle::IBeam)
                .child(SelectableText::new(
                    ("comment-message", comment.rpid),
                    comment.content.message.clone(),
                ))
                .into_any_element()
        };

        let thread = (depth == 0)
            .then(|| self.reply_threads.get(&comment.rpid))
            .flatten();
        let preview = comment.replies();
        let nested_source: &[Comment] = match thread {
            Some(thread) => &thread.replies,
            None => &preview[..preview.len().min(NESTED_COMMENT_LIMIT)],
        };
        let nested = nested_source
            .iter()
            .map(|reply| self.render_comment(reply, depth + 1, root, cx))
            .collect::<Vec<_>>();

        let thread_control: Option<AnyElement> = if depth != 0 {
            None
        } else if let Some(thread) = thread {
            Some(
                h_flex()
                    .items_center()
                    .gap_2()
                    .when(thread.loading, |this| this.child(Spinner::new().small()))
                    .when_some(thread.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(danger).child(error))
                            .child(
                                Button::new(("comment-replies-retry", comment.rpid))
                                    .small()
                                    .label("重试")
                                    .on_click(cx.listener({
                                        let rpid = comment.rpid;
                                        move |this, _, _, cx| this.load_next_replies(rpid, cx)
                                    })),
                            )
                    })
                    .when(thread.more && !thread.loading, |this| {
                        this.child(
                            Button::new(("comment-replies-more", comment.rpid))
                                .small()
                                .ghost()
                                .label("展开更多回复")
                                .on_click(cx.listener({
                                    let rpid = comment.rpid;
                                    move |this, _, _, cx| this.load_next_replies(rpid, cx)
                                })),
                        )
                    })
                    .child(
                        Button::new(("comment-replies-collapse", comment.rpid))
                            .small()
                            .ghost()
                            .label("收起回复")
                            .on_click(cx.listener({
                                let rpid = comment.rpid;
                                move |this, _, _, cx| this.collapse_replies(rpid, cx)
                            })),
                    )
                    .into_any_element(),
            )
        } else if comment.rcount as usize > nested_source.len() {
            Some(
                Button::new(("comment-replies-expand", comment.rpid))
                    .small()
                    .ghost()
                    .label("展开回复")
                    .on_click(cx.listener({
                        let rpid = comment.rpid;
                        move |this, _, _, cx| this.load_next_replies(rpid, cx)
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };

        let replying = self
            .reply_target
            .as_ref()
            .is_some_and(|target| target.parent == comment.rpid);
        let logged_in = client::get(cx).is_ok_and(|client| client.has_session());

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .id(("comment-avatar", comment.rpid))
                            .when(mid > 0, |this| {
                                this.cursor_pointer().on_click(cx.listener({
                                    let uname = uname.clone();
                                    move |_, _, _, cx| {
                                        cx.emit(InfoEvent::OpenSpace {
                                            mid,
                                            name: uname.to_string(),
                                        });
                                    }
                                }))
                            })
                            .child(
                                Avatar::new()
                                    .src(member.avatar_url())
                                    .name(member.uname.clone())
                                    .size(avatar),
                            ),
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
                                            .id(("comment-author", comment.rpid))
                                            .text_sm()
                                            .text_color(primary)
                                            .when(mid > 0, |this| {
                                                this.cursor_pointer()
                                                    .hover(move |this| this.text_color(link_hover))
                                                    .on_click(cx.listener({
                                                        let uname = uname.clone();
                                                        move |_, _, _, cx| {
                                                            cx.emit(InfoEvent::OpenSpace {
                                                                mid,
                                                                name: uname.to_string(),
                                                            });
                                                        }
                                                    }))
                                            })
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
                                    })
                                    .when(logged_in, |this| {
                                        this.child(
                                            Button::new(("comment-reply-button", comment.rpid))
                                                .text()
                                                .small()
                                                .label("回复")
                                                .on_click(cx.listener({
                                                    let uname = uname.clone();
                                                    let rpid = comment.rpid;
                                                    move |this, _, window, cx| {
                                                        this.begin_reply(
                                                            root,
                                                            rpid,
                                                            uname.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                })),
                                        )
                                    }),
                            )
                            .when(replying, |this| this.child(self.render_reply_composer(cx))),
                    ),
            )
            .when(!nested.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(36.)).children(nested))
            })
            .when_some(thread_control, |this, control| {
                this.child(div().pl(px(36.)).child(control))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::EmoteTarget;
    use crate::info::{InfoPanel, InfoSelections};
    use gpui_kit::prelude::*;
    use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};

    #[gpui_kit::test]
    fn opens_one_emote_picker_at_a_time(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, panel) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(500.), px(300.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|_| InfoPanel::new(selections))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, cx| {
            panel.emotes = Some(Vec::new());

            panel.toggle_emote_picker(EmoteTarget::Comment, cx);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Comment));

            panel.toggle_emote_picker(EmoteTarget::Reply, cx);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Reply));

            panel.toggle_emote_picker(EmoteTarget::Reply, cx);
            assert_eq!(panel.emote_target, None);

            panel.toggle_emote_picker(EmoteTarget::Comment, cx);
            panel.close_emote_picker(EmoteTarget::Reply);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Comment));

            panel.close_emote_picker(EmoteTarget::Comment);
            assert_eq!(panel.emote_target, None);
        });
    }
}
