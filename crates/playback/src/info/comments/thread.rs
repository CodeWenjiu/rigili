use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Comment, CommentPage};
use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::{api_error_message, api_task_error_message, format_count};

use super::super::InfoPanel;
use super::{CommentAction, selection_outline};

pub(crate) struct CommentThread {
    pub(super) root: Comment,
    pub(super) replies: Vec<Comment>,
    pub(super) page: u32,
    pub(super) total: u64,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
}

impl CommentThread {
    pub(super) fn new(root: Comment) -> Self {
        let total = root.rcount;
        Self {
            root,
            replies: Vec::new(),
            page: 0,
            total,
            loading: false,
            error: None,
        }
    }

    pub(super) fn more(&self) -> bool {
        (self.replies.len() as u64) < self.total
    }

    pub(super) fn tail_row(&self) -> usize {
        1 + self.replies.len()
    }

    pub(super) fn row_count(&self) -> usize {
        self.tail_row() + 1
    }
}

impl InfoPanel {
    pub(super) fn load_thread_page(&mut self, page: u32, cx: &mut Context<Self>) {
        let Some(details) = self.details.clone() else {
            return;
        };
        let Some(thread) = &self.thread else {
            return;
        };
        if thread.loading {
            return;
        }
        let root = thread.root.rpid;
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                if let Some(thread) = &mut self.thread {
                    thread.error = Some(api_error_message(error));
                }
                self.touch_thread_tail();
                cx.notify();
                return;
            }
        };
        if let Some(thread) = &mut self.thread {
            thread.loading = true;
            thread.error = None;
        }
        self.touch_thread_tail();
        cx.notify();

        let aid = details.aid;
        spawn_api(
            cx,
            async move { client.comment_replies(aid, root, page).await },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                let Some(thread) = this.thread.as_mut() else {
                    return;
                };
                if thread.root.rpid != root {
                    return;
                }
                thread.loading = false;
                let mut reset = None;
                let mut splice = None;
                match result {
                    Ok(CommentPage {
                        page: info,
                        replies,
                    }) => {
                        let replies = replies.unwrap_or_default();
                        let added = replies.len();
                        if page <= 1 {
                            thread.replies = replies;
                            thread.page = if info.num == 0 { page } else { info.num };
                            thread.total = if added == 0 {
                                0
                            } else {
                                info.count.max(info.acount).max(thread.replies.len() as u64)
                            };
                            thread.root.rcount = thread.total;
                            reset = Some(thread.row_count());
                        } else {
                            let previous = thread.replies.len();
                            thread.replies.extend(replies);
                            thread.page = if info.num == 0 { page } else { info.num };
                            thread.total = if added == 0 {
                                thread.replies.len() as u64
                            } else {
                                info.count.max(info.acount).max(thread.replies.len() as u64)
                            };
                            thread.root.rcount = thread.total;
                            splice = Some((1 + previous, added));
                        }
                    }
                    Err(error) => thread.error = Some(api_task_error_message(error, cx)),
                }
                let total = thread.total;
                if let Some(count) = reset {
                    this.thread_list.reset(count);
                    this.clamp_thread_selection();
                }
                if let Some((at, added)) = splice {
                    this.thread_list.splice(at..at, added);
                }
                if let Some(comment) = this
                    .comments
                    .iter_mut()
                    .find(|comment| comment.rpid == root)
                {
                    comment.rcount = total;
                }
                this.touch_row_of(root);
                this.touch_thread_tail();
                cx.notify();
            },
        );
    }

    fn retry_thread(&mut self, cx: &mut Context<Self>) {
        let page = self.thread.as_ref().map_or(1, |thread| {
            if thread.replies.is_empty() {
                1
            } else {
                thread.page + 1
            }
        });
        self.load_thread_page(page, cx);
    }

    fn maybe_load_more_replies(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = &self.thread else {
            return;
        };
        if thread.loading || thread.error.is_some() || !thread.more() {
            return;
        }
        let page = thread.page + 1;
        let weak = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.load_thread_page(page, cx));
        });
    }

    pub(super) fn render_thread_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let composer = self.render_composer(window, cx);
        let header = self.render_thread_header(cx);
        let comments_active = self.comments_focus.focused(window);
        v_flex()
            .flex_1()
            .min_h_0()
            .child(composer)
            .child(header)
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
                            self.thread_list.clone(),
                            cx.processor(Self::render_thread_row),
                        )
                        .p_4()
                        .pb_6()
                        .size_full(),
                    )
                    .child(Scrollbar::vertical(&self.thread_list)),
            )
            .into_any_element()
    }

    fn render_thread_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let total = self.thread.as_ref().map_or(0, |thread| thread.total);
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .child(
                Button::new("comment-thread-back")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronLeft)
                    .accessibility_label("返回评论")
                    .tooltip("返回评论")
                    .on_click(cx.listener(|this, _, window, cx| this.close_thread(window, cx))),
            )
            .child(div().text_sm().child("全部回复"))
            .when(total > 0, |this| {
                this.child(div().text_xs().text_color(muted).child(format_count(total)))
            })
            .into_any_element()
    }

    fn render_thread_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let radius = cx.theme().radius;
        let selected = self.thread_selection == Some(index);
        let active = self.comments_focus.focused(window);
        let Some(thread) = &self.thread else {
            return div().into_any_element();
        };
        if index == 0 {
            let border = cx.theme().border;
            return div()
                .relative()
                .w_full()
                .pb_4()
                .border_b_1()
                .border_color(border)
                .child(self.render_comment(
                    &thread.root,
                    false,
                    CommentAction::ReplyToRoot,
                    thread.root.rpid,
                    cx,
                ))
                .when(selected, |this| {
                    this.child(selection_outline(primary, radius, active))
                })
                .into_any_element();
        }
        if let Some(reply) = thread.replies.get(index - 1) {
            return div()
                .relative()
                .w_full()
                .pb_2()
                .child(self.render_comment(
                    reply,
                    true,
                    CommentAction::ReplyToReply,
                    thread.root.rpid,
                    cx,
                ))
                .when(selected, |this| {
                    this.child(selection_outline(primary, radius, active))
                })
                .into_any_element();
        }
        self.render_thread_tail(window, cx)
    }

    fn render_thread_tail(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        self.maybe_load_more_replies(window, cx);
        let Some(thread) = &self.thread else {
            return div().into_any_element();
        };
        if thread.loading && thread.replies.is_empty() {
            return h_flex()
                .w_full()
                .py_6()
                .justify_center()
                .child(Spinner::new())
                .into_any_element();
        }
        if let Some(error) = thread.error.clone() {
            return v_flex()
                .w_full()
                .items_center()
                .gap_2()
                .py_4()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-replies-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_thread(cx))),
                )
                .into_any_element();
        }
        if thread.replies.is_empty() && !thread.loading {
            return h_flex()
                .w_full()
                .py_4()
                .justify_center()
                .text_sm()
                .text_color(muted)
                .child(if thread.total == 0 {
                    "还没有回复"
                } else {
                    "没有更多回复"
                })
                .into_any_element();
        }
        if thread.more() {
            h_flex()
                .w_full()
                .py_2()
                .justify_center()
                .gap_2()
                .child(Spinner::new().small())
                .child(div().text_sm().text_color(muted).child("正在加载…"))
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .py_2()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("没有更多回复")
                .into_any_element()
        }
    }
}
