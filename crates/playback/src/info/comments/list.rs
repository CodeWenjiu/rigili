use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Comment, avatar};
use rigili_core::client;
use rigili_core::{format_comment_time, format_count, message_markdown};

use super::super::{COMMENT_AVATAR, InfoEvent, InfoPanel, NESTED_COMMENT_AVATAR};
use super::{CommentAction, PREVIEW_REPLIES, selection_outline};

impl InfoPanel {
    pub(crate) fn render_comments_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.thread.is_some() {
            return self.render_thread_tab(window, cx);
        }
        self.maybe_load_earlier_comments(window, cx);
        let composer = self.render_composer(window, cx);
        let comments_active = self.comments_focus.focused(window);
        v_flex()
            .flex_1()
            .min_h_0()
            .child(composer)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .image_cache(rigili_core::image_cache::avatars(cx))
                    .track_focus(self.comments_focus.handle())
                    .when(comments_active, |this| {
                        this.key_context(crate::shortcuts::COMMENTS)
                    })
                    .child(
                        list(
                            self.comments_list.clone(),
                            cx.processor(Self::render_comment_row),
                        )
                        .p_4()
                        .pb_6()
                        .size_full(),
                    )
                    .child(Scrollbar::vertical(&self.comments_list)),
            )
            .into_any_element()
    }

    fn render_comment_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border = cx.theme().border;
        let primary = cx.theme().primary;
        let radius = cx.theme().radius;
        let Some(comment) = self.comments.get(index) else {
            return self.render_comment_tail(window, cx);
        };
        let selected = self.comments_selection == Some(index);
        let active = self.comments_focus.focused(window);
        div()
            .relative()
            .w_full()
            .pb_4()
            .border_b_1()
            .border_color(border)
            .child(self.render_comment(
                comment,
                false,
                CommentAction::OpenThread,
                comment.rpid,
                cx,
            ))
            .when(selected, |this| {
                this.child(selection_outline(primary, radius, active))
            })
            .into_any_element()
    }

    fn maybe_load_more_comments(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.comments_more || self.comments_more_loading || self.comments_more_error.is_some() {
            return;
        }
        let weak = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.load_more_comments(cx));
        });
    }

    fn render_comment_tail(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        self.maybe_load_more_comments(window, cx);

        if self.comments_loading {
            return h_flex()
                .w_full()
                .py_6()
                .justify_center()
                .child(Spinner::new())
                .into_any_element();
        }
        if let Some(error) = self.comments_error.clone() {
            return v_flex()
                .w_full()
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
                .into_any_element();
        }
        if self.comments.is_empty() {
            return if self.comments_loaded {
                h_flex()
                    .w_full()
                    .py_6()
                    .justify_center()
                    .text_sm()
                    .text_color(muted)
                    .child("暂无评论")
                    .into_any_element()
            } else {
                div().into_any_element()
            };
        }
        if let Some(error) = self.comments_more_error.clone() {
            v_flex()
                .w_full()
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
        } else if self.comments_more || self.comments_more_loading {
            h_flex()
                .w_full()
                .py_2()
                .justify_center()
                .gap_2()
                .child(Spinner::new().small())
                .child(div().text_sm().text_color(muted).child("正在加载…"))
                .into_any_element()
        } else if self.comments_loaded {
            h_flex()
                .w_full()
                .py_2()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("没有更多评论")
                .into_any_element()
        } else {
            div().into_any_element()
        }
    }

    pub(super) fn render_comment(
        &self,
        comment: &Comment,
        nested: bool,
        action: CommentAction,
        root: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let member = &comment.member;
        let mid = member.mid;
        let uname: SharedString = member.uname.clone().into();
        let level = member.level_info.current_level;
        let time = format_comment_time(comment.ctime, &comment.reply_control.time_desc);
        let avatar_size = if nested {
            NESTED_COMMENT_AVATAR
        } else {
            COMMENT_AVATAR
        };
        let has_emotes = comment
            .content
            .emote
            .as_ref()
            .is_some_and(|emotes| !emotes.is_empty());
        let message = if has_emotes {
            div()
                .cursor(CursorStyle::IBeam)
                .image_cache(rigili_core::image_cache::emotes(cx))
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

        let previews = if !nested && action == CommentAction::OpenThread {
            comment
                .replies()
                .iter()
                .take(PREVIEW_REPLIES)
                .map(|reply| {
                    self.render_comment(
                        reply,
                        true,
                        CommentAction::OpenThreadReply,
                        comment.rpid,
                        cx,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let hidden = action == CommentAction::OpenThread
            && has_hidden_replies(comment, previews.len());

        let logged_in = client::get(cx).is_ok_and(|client| client.has_session());
        let liked = comment.liked();
        let like_count = format_count(comment.like);
        let like_target = self.aid().filter(|_| logged_in);
        // Four states of its own: the text button variant repaints its own
        // foreground on hover, which would drop the liked colour.
        let like_color = if liked { primary } else { muted };
        let like_hover = if liked {
            primary.mix_oklab(foreground, 0.25)
        } else {
            foreground
        };
        let open: Option<rigili_core::user::Open> = if mid > 0 {
            let weak = cx.entity().downgrade();
            let uname = uname.clone();
            let handler: rigili_core::user::Open =
                std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |_, cx| {
                        cx.emit(InfoEvent::OpenSpace {
                            mid,
                            name: uname.to_string(),
                        });
                    });
                });
            Some(handler)
        } else {
            None
        };

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(rigili_core::user::avatar(
                        avatar(&member.avatar, 64),
                        member.uname.clone(),
                        avatar_size,
                        comment.rpid,
                        open.clone(),
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                rigili_core::user::Identity::new(member.uname.clone())
                                    .key(comment.rpid)
                                    .name_color(primary)
                                    .level(level)
                                    .detail(div().text_xs().text_color(muted).child(time))
                                    .open(open.clone())
                                    .render(cx),
                            )
                            .child(message)
                            .when(!comment.content.pictures.is_empty(), |this| {
                                this.child(rigili_core::comment_picture::grid(
                                    comment.rpid,
                                    &comment.content.pictures,
                                    cx,
                                ))
                            })
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(match like_target {
                                        Some(aid) => Button::new(("comment-like", comment.rpid))
                                            .text()
                                            .small()
                                            .group("comment-like")
                                            .child(
                                                h_flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .text_color(like_color)
                                                    .group_hover("comment-like", move |this| {
                                                        this.text_color(like_hover)
                                                    })
                                                    .child(
                                                        Icon::new(IconName::ThumbsUp)
                                                            .with_size(px(14.)),
                                                    )
                                                    .child(like_count),
                                            )
                                            .on_click(cx.listener({
                                                let rpid = comment.rpid;
                                                move |this, _, window, cx| {
                                                    this.toggle_comment_like(
                                                        rpid, aid, !liked, window, cx,
                                                    );
                                                }
                                            }))
                                            .into_any_element(),
                                        None => h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::new(IconName::ThumbsUp).with_size(px(14.)))
                                            .child(like_count)
                                            .into_any_element(),
                                    })
                                    .when(logged_in && !hidden, |this| {
                                        let rpid = comment.rpid;
                                        this.child(
                                            Button::new(("comment-reply-button", rpid))
                                                .text()
                                                .small()
                                                .label("回复")
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| match action {
                                                        CommentAction::OpenThread => this
                                                            .open_thread_by_rpid(
                                                                rpid, true, None, window, cx,
                                                            ),
                                                        CommentAction::OpenThreadReply => this
                                                            .open_thread_by_rpid(
                                                                root,
                                                                true,
                                                                Some(rpid),
                                                                window,
                                                                cx,
                                                            ),
                                                        CommentAction::ReplyToRoot => {
                                                            this.reply_target = None;
                                                            this.focus_composer(window, cx);
                                                        }
                                                        CommentAction::ReplyToReply => {
                                                            this.reply_target = Some(rpid);
                                                            this.focus_composer(window, cx);
                                                        }
                                                    },
                                                )),
                                        )
                                    }),
                            ),
                    ),
            )
            .when(!previews.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(36.)).children(previews))
            })
            .when(hidden, |this| {
                let rpid = comment.rpid;
                this.child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .pl(px(36.))
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{} 条回复", format_count(comment.rcount)))
                        .child(
                            Button::new(("comment-replies", rpid))
                                .text()
                                .small()
                                .label("展开回复")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_thread_by_rpid(rpid, false, None, window, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}

pub(super) fn has_hidden_replies(comment: &Comment, shown: usize) -> bool {
    comment.rcount as usize > shown
}
