use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::{CommentReply, CommentSort};
use rigili_bilibili::{Comment, CommentPage, avatar, is_emote_image, normalize_url};
use rigili_core::client;
use rigili_core::emotes;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_core::{
    api_error_message, api_task_error_message, format_comment_time, format_count, message_markdown,
};

use super::{COMMENT_AVATAR, COMMENT_MAX_LEN, InfoEvent, InfoPanel, NESTED_COMMENT_AVATAR};

const COMMENT_RETAIN_PAGES: usize = 12;
const PREVIEW_REPLIES: usize = 2;

/// The replies of one comment, shown in their own view.
///
/// The reply endpoint returns the whole chain under a root as one flat page of
/// comments, so this needs no nested list: every reply, including a reply to a
/// reply, is a row of the same list.
pub(super) struct CommentThread {
    root: Comment,
    replies: Vec<Comment>,
    page: u32,
    total: u64,
    loading: bool,
    error: Option<String>,
}

impl CommentThread {
    fn new(root: Comment) -> Self {
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

    fn more(&self) -> bool {
        (self.replies.len() as u64) < self.total
    }

    fn tail_row(&self) -> usize {
        1 + self.replies.len()
    }

    fn row_count(&self) -> usize {
        self.tail_row() + 1
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentAction {
    OpenThread,
    OpenThreadReply,
    ReplyToRoot,
    ReplyToReply,
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
        self.comments_pages.clear();
        self.comments_first_page = 1;
        self.comments_earlier_loading = false;
        self.comments_earlier_error = None;
        self.thread = None;
        self.reply_target = None;
        self.draft = None;
        self._draft_sub = None;
        self.composer_sending = false;
        self.composer_error = None;
        self.emote_open = false;
        self.comments_list.reset(1);
        self.thread_list.reset(1);
    }

    fn touch_comment_tail(&self) {
        let index = self.comments.len();
        if index < self.comments_list.item_count() {
            self.comments_list.remeasure_items(index..index + 1);
        }
    }

    fn touch_thread_tail(&self) {
        let Some(thread) = &self.thread else {
            return;
        };
        let index = thread.tail_row();
        if index < self.thread_list.item_count() {
            self.thread_list.remeasure_items(index..index + 1);
        }
    }

    fn touch_row_of(&self, rpid: u64) {
        if let Some(row) = self.comments.iter().position(|comment| {
            comment.rpid == rpid
                || comment
                    .replies()
                    .iter()
                    .any(|reply| reply.rpid == rpid)
        }) && row < self.comments_list.item_count()
        {
            self.comments_list.remeasure_items(row..row + 1);
        }
        let Some(thread) = &self.thread else {
            return;
        };
        let row = if thread.root.rpid == rpid {
            Some(0)
        } else {
            thread
                .replies
                .iter()
                .position(|reply| reply.rpid == rpid)
                .map(|index| index + 1)
        };
        if let Some(row) = row
            && row < self.thread_list.item_count()
        {
            self.thread_list.remeasure_items(row..row + 1);
        }
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
                self.touch_comment_tail();
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
        self.comments_pages.clear();
        self.comments_first_page = 1;
        self.comments_earlier_loading = false;
        self.comments_earlier_error = None;
        self.thread = None;
        self.reply_target = None;
        self.composer_error = None;
        self.emote_open = false;
        self.comments_list.reset(1);
        self.thread_list.reset(1);
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
                        if !this.comments.is_empty() {
                            this.comments_pages.push_back(this.comments.len());
                        }
                        this.comments_total = info.count.max(info.acount);
                        this.comments_more = !this.comments.is_empty()
                            && (this.comments.len() as u64) < this.comments_total;
                        this.comments_loaded = true;
                    }
                    Err(error) => this.comments_error = Some(api_task_error_message(error, cx)),
                }
                this.comments_list.reset(this.comments.len() + 1);
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
        self.touch_comment_tail();
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
                        let added = replies.unwrap_or_default();
                        let count = added.len();
                        let previous = this.comments.len();
                        this.comments.extend(added);
                        if count > 0 {
                            this.comments_list.splice(previous..previous, count);
                            this.comments_pages.push_back(count);
                            this.trim_front_pages();
                        }
                        this.comments_page = if info.num == 0 { requested } else { info.num };
                        this.comments_total = this.comments_total.max(info.count.max(info.acount));
                        this.comments_more = this.comments_more
                            && count > 0
                            && (this.comments.len() as u64) < this.comments_total;
                    }
                    Err(error) => {
                        this.comments_more_error = Some(api_task_error_message(error, cx))
                    }
                }
                this.touch_comment_tail();
                cx.notify();
            },
        );
    }

    fn trim_front_pages(&mut self) {
        let (pages_dropped, dropped) = drain_front_pages(&mut self.comments_pages);
        let dropped = dropped.min(self.comments.len());
        if dropped == 0 {
            return;
        }
        let top = self.comments_list.logical_scroll_top();
        self.comments.drain(0..dropped);
        self.comments_first_page = self.comments_first_page.saturating_add(pages_dropped);
        self.comments_list.splice(0..dropped, 0);
        self.comments_list.scroll_to(ListOffset {
            item_ix: top.item_ix.saturating_sub(dropped),
            offset_in_item: top.offset_in_item,
        });
    }

    fn trim_back_pages(&mut self) {
        let (pages_dropped, dropped) = drain_back_pages(&mut self.comments_pages);
        let dropped = dropped.min(self.comments.len());
        if dropped == 0 {
            return;
        }
        let start = self.comments.len() - dropped;
        self.comments.truncate(start);
        self.comments_list.splice(start..start + dropped, 0);
        self.comments_page = self.comments_page.saturating_sub(pages_dropped);
        self.comments_more = true;
    }

    fn load_earlier_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments_loading || self.comments_earlier_loading || self.comments_first_page <= 1 {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let Ok(client) = client::get(cx) else {
            return;
        };
        let aid = details.aid;
        let requested = self.comments_first_page - 1;
        self.comments_earlier_loading = true;
        self.comments_earlier_error = None;
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
                this.comments_earlier_loading = false;
                match result {
                    Ok(page) => {
                        let CommentPage {
                            page: info,
                            replies,
                        } = page;
                        let added = replies.unwrap_or_default();
                        let count = added.len();
                        this.comments_total = this.comments_total.max(info.count.max(info.acount));
                        if count > 0 {
                            let top = this.comments_list.logical_scroll_top();
                            this.comments.splice(0..0, added);
                            this.comments_pages.push_front(count);
                            this.comments_first_page = requested;
                            this.comments_list.splice(0..0, count);
                            this.comments_list.scroll_to(ListOffset {
                                item_ix: top.item_ix + count,
                                offset_in_item: top.offset_in_item,
                            });
                            this.trim_back_pages();
                        } else {
                            this.comments_first_page = requested;
                        }
                    }
                    Err(error) => {
                        this.comments_earlier_error = Some(api_task_error_message(error, cx))
                    }
                }
                cx.notify();
            },
        );
    }

    fn maybe_load_earlier_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let top = self.comments_list.logical_scroll_top();
        if top.item_ix > 0 {
            self.comments_earlier_error = None;
            return;
        }
        if self.comments_first_page <= 1
            || self.comments_loading
            || self.comments_earlier_loading
            || self.comments_earlier_error.is_some()
        {
            return;
        }
        let weak = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = weak.update(cx, |this, cx| this.load_earlier_comments(cx));
        });
    }

    fn open_thread(
        &mut self,
        index: usize,
        focus: bool,
        target: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.comments.get(index).cloned() else {
            return;
        };
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.root.rpid == root.rpid)
        {
            self.reply_target = target;
            if focus {
                self.focus_draft(window, cx);
            }
            return;
        }
        self.reply_target = target;
        self.composer_error = None;
        self.emote_open = false;
        self.thread = Some(CommentThread::new(root));
        self.thread_list.reset(2);
        if let Some(draft) = &self.draft {
            draft.update(cx, |state, cx| state.set_placeholder("回复评论", window, cx));
        }
        if focus {
            self.focus_draft(window, cx);
        }
        self.load_thread_page(1, cx);
        cx.notify();
    }

    fn open_thread_by_rpid(
        &mut self,
        rpid: u64,
        focus: bool,
        target: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .comments
            .iter()
            .position(|comment| comment.rpid == rpid)
        {
            self.open_thread(index, focus, target, window, cx);
        }
    }

    fn focus_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(window, cx);
        window.defer(cx, move |window, cx| {
            draft.update(cx, |state, cx| state.focus(window, cx));
        });
        cx.notify();
    }

    fn close_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.thread = None;
        self.reply_target = None;
        self.composer_error = None;
        self.close_emote_picker();
        self.thread_list.reset(1);
        if let Some(draft) = &self.draft {
            draft.update(cx, |state, cx| {
                state.set_placeholder("发一条友善的评论", window, cx)
            });
        }
        cx.notify();
    }

    pub(crate) fn back_in_comments(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.thread.is_none() {
            return false;
        }
        self.close_thread(window, cx);
        true
    }

    fn load_thread_page(&mut self, page: u32, cx: &mut Context<Self>) {
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

    /// Applies the like to every copy the views can show: the comment row in
    /// the list, its preview replies, and the thread's own root clone.
    fn set_comment_liked(&mut self, rpid: u64, like: bool) -> bool {
        let mut found = false;
        for comment in &mut self.comments {
            if comment.rpid == rpid {
                comment.set_liked(like);
                found = true;
            }
            if let Some(replies) = comment.replies.as_mut() {
                for reply in replies {
                    if reply.rpid == rpid {
                        reply.set_liked(like);
                        found = true;
                    }
                }
            }
        }
        if let Some(thread) = &mut self.thread {
            if thread.root.rpid == rpid {
                thread.root.set_liked(like);
                found = true;
            }
            if let Some(reply) = thread
                .replies
                .iter_mut()
                .find(|reply| reply.rpid == rpid)
            {
                reply.set_liked(like);
                found = true;
            }
        }
        found
    }

    fn toggle_comment_like(
        &mut self,
        rpid: u64,
        aid: u64,
        like: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(client) = client::get(cx) else {
            return;
        };
        if !self.set_comment_liked(rpid, like) {
            return;
        }
        self.touch_row_of(rpid);
        cx.notify();

        let window_handle = window.window_handle();
        spawn_api(
            cx,
            async move { client.like_comment(aid, rpid, like).await },
            move |this, result, cx| {
                let Err(error) = result else {
                    return;
                };
                this.set_comment_liked(rpid, !like);
                this.touch_row_of(rpid);
                let message = api_task_error_message(error, cx);
                let _ = window_handle.update(cx, |_, window, cx| {
                    toast::push(window, cx, Notification::error(message));
                });
                cx.notify();
            },
        );
    }

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

    fn set_emote_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.close_emote_picker();
            cx.notify();
            return;
        }
        self.emote_open = true;
        self.load_emotes(cx);
        cx.notify();
    }

    fn close_emote_picker(&mut self) {
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

    fn render_emote_picker(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            draft.read(cx).focus_handle(cx).is_focused(window)
        })
    }

    pub(super) fn render_comments_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.thread.is_some() {
            return self.render_thread_tab(window, cx);
        }
        self.maybe_load_earlier_comments(window, cx);
        let composer = self.render_composer(window, cx);
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

    fn render_thread_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let composer = self.render_composer(window, cx);
        let header = self.render_thread_header(cx);
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

    fn render_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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

    fn render_comment_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border = cx.theme().border;
        let Some(comment) = self.comments.get(index) else {
            return self.render_comment_tail(window, cx);
        };
        div()
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
            .into_any_element()
    }

    fn render_thread_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(thread) = &self.thread else {
            return div().into_any_element();
        };
        if index == 0 {
            let border = cx.theme().border;
            return div()
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
                .into_any_element();
        }
        if let Some(reply) = thread.replies.get(index - 1) {
            return div()
                .w_full()
                .pb_2()
                .child(self.render_comment(
                    reply,
                    true,
                    CommentAction::ReplyToReply,
                    thread.root.rpid,
                    cx,
                ))
                .into_any_element();
        }
        self.render_thread_tail(window, cx)
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

    fn render_comment(
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
                                                            this.focus_draft(window, cx);
                                                        }
                                                        CommentAction::ReplyToReply => {
                                                            this.reply_target = Some(rpid);
                                                            this.focus_draft(window, cx);
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

fn has_hidden_replies(comment: &Comment, shown: usize) -> bool {
    comment.rcount as usize > shown
}

fn drain_front_pages(pages: &mut VecDeque<usize>) -> (u32, usize) {
    let mut pages_dropped = 0;
    let mut items_dropped = 0;
    while pages.len() > COMMENT_RETAIN_PAGES {
        let Some(size) = pages.pop_front() else {
            break;
        };
        pages_dropped += 1;
        items_dropped += size;
    }
    (pages_dropped, items_dropped)
}

fn drain_back_pages(pages: &mut VecDeque<usize>) -> (u32, usize) {
    let mut pages_dropped = 0;
    let mut items_dropped = 0;
    while pages.len() > COMMENT_RETAIN_PAGES {
        let Some(size) = pages.pop_back() else {
            break;
        };
        pages_dropped += 1;
        items_dropped += size;
    }
    (pages_dropped, items_dropped)
}

#[cfg(test)]
mod tests {
    use super::{
        COMMENT_RETAIN_PAGES, CommentThread, drain_back_pages, drain_front_pages,
        has_hidden_replies,
    };
    use crate::info::{InfoPanel, InfoSelections};
    use gpui_kit::base::v_flex;
    use gpui_kit::TestSupportExt as _;
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Bounds, Entity, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use rigili_bilibili::{Comment, EmoteItem, EmotePackage, EmotePanel};
    use std::collections::VecDeque;

    fn comment(rpid: u64, rcount: u64) -> Comment {
        Comment {
            rpid,
            rcount,
            ..Default::default()
        }
    }

    fn plain(rpid: u64) -> Comment {
        Comment {
            rpid,
            ..Default::default()
        }
    }

    #[test]
    fn expands_only_when_more_replies_are_hidden() {
        let mut root = comment(1, 7);
        assert!(has_hidden_replies(&root, 2));

        root.rcount = 2;
        assert!(!has_hidden_replies(&root, 2));

        assert!(!has_hidden_replies(&comment(2, 0), 0));
        assert!(has_hidden_replies(&comment(3, 3), 2));
    }

    #[test]
    fn a_thread_keeps_one_root_row_and_one_tail_row() {
        let mut thread = CommentThread::new(comment(1, 3));
        assert!(thread.more());
        assert_eq!(thread.tail_row(), 1);
        assert_eq!(thread.row_count(), 2);

        thread.replies.push(plain(2));
        assert_eq!(thread.tail_row(), 2);
        assert_eq!(thread.row_count(), 3);
        assert!(thread.more());

        thread.replies.extend([plain(3), plain(4)]);
        assert!(!thread.more());
    }

    #[gpui_kit::test]
    fn renders_a_thread_with_its_root_and_replies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        cx.update(rigili_core::client::init);
        let (window, panel) = cx.update(|cx| {
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

        panel.update(cx, |panel, _| {
            panel.details = Some(std::sync::Arc::new(crate::info::VideoDetails::new(
                Default::default(),
            )));
            panel.tab = crate::info::InfoTab::Comments;
            panel.comments = vec![comment(10, 2)];
            panel.comments_list.reset(2);
            panel.thread = Some(CommentThread {
                root: comment(10, 2),
                replies: vec![plain(11), plain(12)],
                page: 1,
                total: 2,
                loading: false,
                error: None,
            });
            panel.thread_list.reset(4);
        });

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .expect("render thread");

        panel.update(cx, |panel, _| {
            assert_eq!(panel.thread_list.item_count(), 4);
        });
    }

    #[gpui_kit::test]
    fn renders_the_first_two_preview_replies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        cx.update(rigili_core::client::init);
        let (window, panel) = cx.update(|cx| {
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

        panel.update(cx, |panel, _| {
            panel.details = Some(std::sync::Arc::new(crate::info::VideoDetails::new(
                Default::default(),
            )));
            panel.tab = crate::info::InfoTab::Comments;
            let mut root = comment(10, 7);
            root.replies = Some(vec![plain(11), plain(12), plain(13)]);
            panel.comments = vec![root];
            panel.comments_list.reset(2);
        });

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .expect("render comments");
    }

    #[gpui_kit::test]
    fn a_thread_leaves_the_comment_list_where_it_was(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        let (window, panel) = cx.update(|cx| {
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

        panel.update(cx, |panel, _| {
            panel.comments = vec![comment(10, 2), comment(20, 0)];
            panel.comments_list.reset(panel.comments.len() + 1);
        });

        cx.update_window(window, |_, window, cx| {
            panel.update(cx, |panel, cx| panel.open_thread(1, false, None, window, cx));
        })
        .expect("open thread");

        cx.update_window(window, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                assert_eq!(panel.thread.as_ref().map(|thread| thread.root.rpid), Some(20));
                assert_eq!(panel.thread_list.item_count(), 2);
                assert!(panel.back_in_comments(window, cx));
                assert!(panel.thread.is_none());
                assert_eq!(panel.comments_list.item_count(), 3);
                assert!(!panel.back_in_comments(window, cx));
            });
        })
        .expect("close thread");
    }

    struct EmotePickerHarness {
        panel: Entity<InfoPanel>,
    }

    impl Render for EmotePickerHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .child(
                    div()
                        .id("emote-picker-probe")
                        .test_support()
                        .size(px(10.)),
                )
                .child(self.panel.update(cx, |panel, cx| panel.render_emote_picker(cx)))
        }
    }

    #[gpui_kit::test]
    fn renders_text_emotes_in_the_picker(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        cx.update(|cx| {
            assert!(rigili_core::emotes::start(cx));
            rigili_core::emotes::succeed(
                cx,
                EmotePanel {
                    packages: Some(vec![EmotePackage {
                        text: "颜文字".into(),
                        url: String::new(),
                        emote: Some(vec![EmoteItem {
                            text: "(╯°□°）╯︵ ┻━┻".into(),
                            url: "(╯°□°）╯︵ ┻━┻".into(),
                        }]),
                    }]),
                },
            );
            assert_eq!(rigili_core::emotes::packages(cx).len(), 1);
        });
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(400.), px(300.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    let panel = cx.new(|_| InfoPanel::new(selections));
                    cx.new(|_| EmotePickerHarness { panel })
                },
            )
            .expect("open emote picker window")
        });

        let bounds = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.render_frame(cx);
                window.find("emote-picker-probe").bounds();
                window.find(("comment-emote-item", 0u64)).bounds()
            })
            .expect("emote item bounds");
        assert!(bounds.size.width >= px(32.));
    }

    #[gpui_kit::test]
    fn opens_and_closes_the_emote_picker(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        // A loaded (empty) list keeps the picker from reaching for the network.
        cx.update(|cx| {
            assert!(rigili_core::emotes::start(cx));
            rigili_core::emotes::succeed(cx, Default::default());
        });
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
            panel.set_emote_picker_open(true, cx);
            assert!(panel.emote_open);

            panel.set_emote_picker_open(false, cx);
            assert!(!panel.emote_open);

            panel.set_emote_picker_open(true, cx);
            panel.close_emote_picker();
            assert!(!panel.emote_open);
        });
    }

    #[test]
    fn retains_a_bounded_page_window() {
        let mut pages = VecDeque::from(vec![20; COMMENT_RETAIN_PAGES]);
        assert_eq!(drain_front_pages(&mut pages), (0, 0));
        assert_eq!(pages.len(), COMMENT_RETAIN_PAGES);

        let mut pages = VecDeque::from(vec![20; COMMENT_RETAIN_PAGES + 2]);
        assert_eq!(drain_front_pages(&mut pages), (2, 40));
        assert_eq!(pages.len(), COMMENT_RETAIN_PAGES);

        let mut pages = VecDeque::from(vec![20; COMMENT_RETAIN_PAGES + 3]);
        assert_eq!(drain_back_pages(&mut pages), (3, 60));
        assert_eq!(pages.len(), COMMENT_RETAIN_PAGES);
    }
}
