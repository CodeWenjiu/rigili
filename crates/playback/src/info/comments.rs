use std::collections::{HashMap, VecDeque};

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
use rigili_bilibili::{Comment, CommentPage, avatar, normalize_url};
use rigili_core::client;
use rigili_core::emotes;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_core::{
    api_error_message, api_task_error_message, format_comment_time, format_count, message_markdown,
};

use super::{
    COMMENT_AVATAR, COMMENT_MAX_LEN, InfoEvent, InfoPanel, NESTED_COMMENT_AVATAR,
    NESTED_COMMENT_LIMIT,
};

const COMMENT_RETAIN_PAGES: usize = 12;

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

/// One row of the virtual comment list.
///
/// Replies used to render inside their comment, so a thread with hundreds of
/// them was a single enormous list item that defeated the virtualization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommentRow {
    Comment(usize),
    Reply { root: usize, reply: usize },
    Control(usize),
    Tail,
}

impl EmoteTarget {
    fn slot(self) -> u64 {
        match self {
            Self::Comment => 0,
            Self::Reply => 1,
        }
    }

    fn key(self) -> (&'static str, u64) {
        ("comment-emote", self.slot())
    }

    fn popover(self) -> (&'static str, u64) {
        ("comment-emote-popover", self.slot())
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
        self.comments_pages.clear();
        self.comments_first_page = 1;
        self.comments_earlier_loading = false;
        self.comments_earlier_error = None;
        self.reply_threads.clear();
        self.reply_target = None;
        self.reply_draft = None;
        self._reply_draft_sub = None;
        self.reply_sending = false;
        self.reply_error = None;
        self.emote_target = None;
        self.reset_comment_rows();
    }

    fn reset_comment_rows(&mut self) {
        let count = self.rebuild_comment_rows();
        self.comments_list.reset(count);
    }

    fn rebuild_comment_rows(&mut self) -> usize {
        self.comment_rows = build_comment_rows(&self.comments, &self.reply_threads);
        self.comment_rows.len()
    }

    fn rows_for_comment(&self, index: usize) -> usize {
        self.comments
            .get(index)
            .map_or(0, |comment| rows_for_comment(comment, &self.reply_threads))
    }

    fn comment_row_start(&self, index: usize) -> usize {
        comment_row_start(&self.comments, &self.reply_threads, index)
    }

    fn list_thread_shape(&self, index: usize, start: usize) -> (usize, bool) {
        let mut replies = 0;
        let mut control = false;
        for row in self.comment_rows.iter().skip(start + 1) {
            match row {
                CommentRow::Reply { root, .. } if *root == index => replies += 1,
                CommentRow::Control(root) if *root == index => control = true,
                _ => break,
            }
        }
        (replies, control)
    }

    fn ends_thread(&self, row: usize) -> bool {
        !matches!(
            self.comment_rows.get(row + 1),
            Some(CommentRow::Reply { .. } | CommentRow::Control(_))
        )
    }

    /// Applies a thread change at `index` to the list.
    ///
    /// The rows of the thread are spliced so the count and the measurements
    /// stay in step with the comments. The rows the thread had are read from
    /// the list itself: callers may already have changed the data (`rcount`,
    /// the thread map) before getting here.
    fn sync_thread(&mut self, index: usize) {
        let start = self.comment_row_start(index);
        let Some(comment) = self.comments.get(index) else {
            return;
        };
        let (replies, control) = thread_shape(comment, &self.reply_threads);
        let was = self.list_thread_shape(index, start);
        let rows = self.rebuild_comment_rows();
        let (at, removed, added) = thread_splice(start, was, (replies, control));
        if removed != added {
            self.comments_list.splice(at..at + removed, added);
        }
        let end = (start + 1 + replies + usize::from(control)).min(rows);
        if start < end {
            self.comments_list.remeasure_items(start..end);
        }
    }

    fn touch_row(&self, row: usize) {
        if row < self.comments_list.item_count() {
            self.comments_list.remeasure_items(row..row + 1);
        }
    }

    fn row_of_rpid(&self, rpid: u64) -> Option<usize> {
        row_of_rpid(&self.comments, &self.reply_threads, rpid)
    }

    fn touch_row_of(&self, rpid: u64) {
        if let Some(row) = self.row_of_rpid(rpid) {
            self.touch_row(row);
        }
    }

    fn touch_reply_row(&self) {
        if let Some(target) = &self.reply_target {
            self.touch_row_of(target.parent);
        }
    }

    fn touch_comment_tail(&self) {
        let index = self.comment_rows.len().saturating_sub(1);
        if index < self.comments_list.item_count() {
            self.comments_list.remeasure_items(index..index + 1);
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
        self.reply_threads.clear();
        self.reply_target = None;
        self.reply_error = None;
        self.emote_target = None;
        self.reset_comment_rows();
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
                this.reset_comment_rows();
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
                        this.comments.extend(added);
                        let previous = this.comment_rows.len();
                        let rows = this.rebuild_comment_rows();
                        this.comments_list
                            .splice(previous - 1..previous - 1, rows - previous);
                        if count > 0 {
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
        let dropped_rows: usize = (0..dropped).map(|index| self.rows_for_comment(index)).sum();
        let roots: Vec<u64> = self
            .comments
            .drain(0..dropped)
            .map(|comment| comment.rpid)
            .collect();
        for root in roots {
            self.reply_threads.remove(&root);
        }
        self.comments_first_page = self.comments_first_page.saturating_add(pages_dropped);
        self.rebuild_comment_rows();
        self.comments_list.splice(0..dropped_rows, 0);
        self.comments_list.scroll_to(ListOffset {
            item_ix: top.item_ix.saturating_sub(dropped_rows),
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
        let start_row = self.comment_row_start(start);
        let dropped_rows: usize = (start..self.comments.len())
            .map(|index| self.rows_for_comment(index))
            .sum();
        let roots: Vec<u64> = self
            .comments
            .drain(start..)
            .map(|comment| comment.rpid)
            .collect();
        for root in roots {
            self.reply_threads.remove(&root);
        }
        self.rebuild_comment_rows();
        self.comments_list
            .splice(start_row..start_row + dropped_rows, 0);
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
                            let previous = this.comment_rows.len();
                            let rows = this.rebuild_comment_rows();
                            let added_rows = rows - previous;
                            this.comments_pages.push_front(count);
                            this.comments_first_page = requested;
                            this.comments_list.splice(0..0, added_rows);
                            this.comments_list.scroll_to(ListOffset {
                                item_ix: top.item_ix + added_rows,
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
        let Some(index) = self
            .comments
            .iter()
            .position(|comment| comment.rpid == root)
        else {
            return;
        };
        if self
            .reply_threads
            .get(&root)
            .is_some_and(|thread| thread.loading)
        {
            return;
        }
        {
            let thread = self.reply_threads.entry(root).or_default();
            if page == 1
                && thread.replies.is_empty()
                && let Some(comment) = self.comments.get(index)
            {
                thread.replies = comment
                    .replies()
                    .iter()
                    .take(NESTED_COMMENT_LIMIT)
                    .cloned()
                    .collect();
            }
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                if let Some(thread) = self.reply_threads.get_mut(&root) {
                    thread.error = Some(api_error_message(error));
                }
                self.sync_thread(index);
                cx.notify();
                return;
            }
        };
        let aid = details.aid;
        if let Some(thread) = self.reply_threads.get_mut(&root) {
            thread.loading = true;
            thread.error = None;
        }
        self.sync_thread(index);
        cx.notify();

        spawn_api(
            cx,
            async move { client.comment_replies(aid, root, page).await },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                let Some(index) = this
                    .comments
                    .iter()
                    .position(|comment| comment.rpid == root)
                else {
                    return;
                };
                {
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
                }
                this.sync_thread(index);
                cx.notify();
            },
        );
    }

    fn collapse_replies(&mut self, root: u64, cx: &mut Context<Self>) {
        let Some(index) = self
            .comments
            .iter()
            .position(|comment| comment.rpid == root)
        else {
            self.reply_threads.remove(&root);
            return;
        };
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
        self.sync_thread(index);
        cx.notify();
    }

    /// Applies the like to every copy of a comment: the list keeps preview
    /// replies and the expanded thread separately, and either can render.
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
        for thread in self.reply_threads.values_mut() {
            for reply in &mut thread.replies {
                if reply.rpid == rpid {
                    reply.set_liked(like);
                    found = true;
                }
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
                        this.reply_error = None;
                        this.touch_reply_row();
                        cx.notify();
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
        let previous = self.reply_target.replace(ReplyTarget {
            root,
            parent,
            uname,
        });
        let same_target = previous
            .as_ref()
            .is_some_and(|target| target.parent == parent);
        self.reply_error = None;
        let draft = self.reply_draft(window, cx);
        if !same_target {
            draft.update(cx, |state, cx| state.set_value("", window, cx));
        }
        window.defer(cx, move |window, cx| {
            draft.update(cx, |state, cx| state.focus(window, cx));
        });
        if let Some(previous) = previous {
            self.touch_row_of(previous.parent);
        }
        self.touch_row_of(parent);
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
            self.touch_reply_row();
            cx.notify();
            return;
        }
        if content.chars().count() > COMMENT_MAX_LEN {
            self.reply_error = Some(format!("回复不能超过 {COMMENT_MAX_LEN} 字"));
            self.touch_reply_row();
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
                self.touch_reply_row();
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
        self.touch_reply_row();
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
                            toast::push(window, cx, Notification::success("回复已发布"));
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
                        this.touch_reply_row();
                        cx.notify();
                    }
                }
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
                self.touch_reply_emote_row();
                cx.notify();
                return;
            }
        };
        self.touch_reply_emote_row();
        cx.notify();

        spawn_api(
            cx,
            async move { client.comment_emotes().await },
            move |this, result, cx| {
                match result {
                    Ok(panel) => emotes::succeed(cx, panel),
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        emotes::fail(cx, message);
                    }
                }
                this.touch_reply_emote_row();
                cx.notify();
            },
        );
    }

    fn set_emote_picker_open(&mut self, target: EmoteTarget, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.close_emote_picker(target);
            cx.notify();
            return;
        }
        self.emote_target = Some(target);
        if target == EmoteTarget::Reply {
            self.touch_reply_row();
        }
        self.load_emotes(cx);
        cx.notify();
    }

    fn close_emote_picker(&mut self, target: EmoteTarget) {
        if self.emote_target == Some(target) {
            self.emote_target = None;
            if target == EmoteTarget::Reply {
                self.touch_reply_row();
            }
        }
    }

    fn touch_reply_emote_row(&self) {
        if self.emote_target == Some(EmoteTarget::Reply) {
            self.touch_reply_row();
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
            EmoteTarget::Reply => {
                self.reply_error = None;
                self.touch_reply_row();
            }
        }
        cx.notify();
    }

    fn render_emote_button(&self, target: EmoteTarget, cx: &mut Context<Self>) -> AnyElement {
        let open = self.emote_target == Some(target);
        let weak = cx.entity().downgrade();
        Popover::new(target.popover())
            .offset(px(6.))
            .open(open)
            .trigger(
                Button::new(target.key())
                    .ghost()
                    .small()
                    .icon(IconName::Sticker)
                    .accessibility_label("表情")
                    .tooltip("表情"),
            )
            .on_open_change(move |open, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.set_emote_picker_open(target, *open, cx);
                });
            })
            .when(open, |this| {
                this.child(self.render_emote_picker(target, cx))
            })
            .into_any_element()
    }

    fn render_emote_picker(&self, target: EmoteTarget, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
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
                                            .image_cache(&images)
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
                                    if target == EmoteTarget::Reply {
                                        this.touch_reply_row();
                                    }
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
                                        .image_cache(&images)
                                        .size(px(24.))
                                        .object_fit(ObjectFit::Contain)
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
        let focused = |draft: &Option<Entity<TextareaState>>| {
            draft
                .as_ref()
                .is_some_and(|draft| draft.read(cx).focus_handle(cx).is_focused(window))
        };
        focused(&self.draft) || focused(&self.reply_draft)
    }

    pub(super) fn render_comments_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.maybe_load_earlier_comments(window, cx);
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
            .child(
                div()
                    .id("comment-draft-input")
                    .key_context(rigili_core::TEXT_INPUT_CONTEXT)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(Textarea::new(draft).w_full()),
            )
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
                            toast::push(window, cx, Notification::success("评论已发布"));
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

    fn render_comment_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border = cx.theme().border;
        let ends = self.ends_thread(index);
        let shell = move |indented: bool, row: AnyElement| {
            div()
                .w_full()
                .pb_2()
                .when(indented, |this| this.pl(px(36.)))
                .when(ends, |this| this.pb_5().border_b_1().border_color(border))
                .child(row)
        };
        match self.comment_rows.get(index).copied() {
            Some(CommentRow::Comment(root)) => {
                let Some(comment) = self.comments.get(root) else {
                    return self.render_comment_tail(window, cx);
                };
                let rpid = comment.rpid;
                shell(false, self.render_comment(comment, 0, rpid, cx)).into_any_element()
            }
            Some(CommentRow::Reply { root, reply }) => {
                let Some(comment) = self.comments.get(root) else {
                    return self.render_comment_tail(window, cx);
                };
                let rpid = comment.rpid;
                let Some(reply) = visible_replies(comment, &self.reply_threads).get(reply) else {
                    return self.render_comment_tail(window, cx);
                };
                shell(true, self.render_comment(reply, 1, rpid, cx)).into_any_element()
            }
            Some(CommentRow::Control(root)) => {
                shell(true, self.render_thread_control(root, cx)).into_any_element()
            }
            Some(CommentRow::Tail) | None => self.render_comment_tail(window, cx),
        }
    }

    fn render_thread_control(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let danger = cx.theme().danger;
        let Some(rpid) = self.comments.get(index).map(|comment| comment.rpid) else {
            return div().into_any_element();
        };
        let Some(thread) = self.reply_threads.get(&rpid) else {
            return Button::new(("comment-replies-expand", rpid))
                .small()
                .ghost()
                .label("展开回复")
                .on_click(cx.listener(move |this, _, _, cx| this.load_next_replies(rpid, cx)))
                .into_any_element();
        };
        h_flex()
            .items_center()
            .gap_2()
            .when(thread.loading, |this| this.child(Spinner::new().small()))
            .when_some(thread.error.clone(), |this, error| {
                this.child(div().text_xs().text_color(danger).child(error))
                    .child(
                        Button::new(("comment-replies-retry", rpid))
                            .small()
                            .label("重试")
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.load_next_replies(rpid, cx)),
                            ),
                    )
            })
            .when(thread.more && !thread.loading, |this| {
                this.child(
                    Button::new(("comment-replies-more", rpid))
                        .small()
                        .ghost()
                        .label("展开更多回复")
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.load_next_replies(rpid, cx)),
                        ),
                )
            })
            .child(
                Button::new(("comment-replies-collapse", rpid))
                    .small()
                    .ghost()
                    .label("收起回复")
                    .on_click(cx.listener(move |this, _, _, cx| this.collapse_replies(rpid, cx))),
            )
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
            .child(
                div()
                    .id("reply-draft-input")
                    .key_context(rigili_core::TEXT_INPUT_CONTEXT)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(Textarea::new(draft).w_full()),
            )
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
                                        this.touch_reply_row();
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
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let member = &comment.member;
        let mid = member.mid;
        let uname: SharedString = member.uname.clone().into();
        let level = member.level_info.current_level;
        let time = format_comment_time(comment.ctime, &comment.reply_control.time_desc);
        let avatar_size = if depth == 0 {
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

        let nested = if depth == 0 {
            Vec::new()
        } else {
            comment
                .replies()
                .iter()
                .take(NESTED_COMMENT_LIMIT)
                .map(|reply| self.render_comment(reply, depth + 1, root, cx))
                .collect::<Vec<_>>()
        };

        let replying = self
            .reply_target
            .as_ref()
            .is_some_and(|target| target.parent == comment.rpid);
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
            .into_any_element()
    }
}

fn visible_replies<'a>(
    comment: &'a Comment,
    threads: &'a HashMap<u64, ReplyThread>,
) -> &'a [Comment] {
    match threads.get(&comment.rpid) {
        Some(thread) => &thread.replies,
        None => {
            let preview = comment.replies();
            &preview[..preview.len().min(NESTED_COMMENT_LIMIT)]
        }
    }
}

fn has_thread_control(comment: &Comment, threads: &HashMap<u64, ReplyThread>) -> bool {
    threads.contains_key(&comment.rpid)
        || comment.rcount as usize > visible_replies(comment, threads).len()
}

fn thread_shape(comment: &Comment, threads: &HashMap<u64, ReplyThread>) -> (usize, bool) {
    (
        visible_replies(comment, threads).len(),
        has_thread_control(comment, threads),
    )
}

/// The list edit that turns a thread with `from` rows into one with `to`.
///
/// `start` is the row of the comment itself, so the edit begins after the
/// replies both shapes share. A row count that does not change means only the
/// control row was replaced, which the caller can remeasure in place.
fn thread_splice(start: usize, from: (usize, bool), to: (usize, bool)) -> (usize, usize, usize) {
    let shared = from.0.min(to.0);
    (
        start + 1 + shared,
        from.0 - shared + usize::from(from.1),
        to.0 - shared + usize::from(to.1),
    )
}

fn rows_for_comment(comment: &Comment, threads: &HashMap<u64, ReplyThread>) -> usize {
    let (replies, control) = thread_shape(comment, threads);
    1 + replies + usize::from(control)
}

fn build_comment_rows(
    comments: &[Comment],
    threads: &HashMap<u64, ReplyThread>,
) -> Vec<CommentRow> {
    let mut rows = Vec::with_capacity(comments.len() + 1);
    for (index, comment) in comments.iter().enumerate() {
        rows.push(CommentRow::Comment(index));
        for reply in 0..visible_replies(comment, threads).len() {
            rows.push(CommentRow::Reply { root: index, reply });
        }
        if has_thread_control(comment, threads) {
            rows.push(CommentRow::Control(index));
        }
    }
    rows.push(CommentRow::Tail);
    rows
}

fn comment_row_start(
    comments: &[Comment],
    threads: &HashMap<u64, ReplyThread>,
    index: usize,
) -> usize {
    comments
        .iter()
        .take(index)
        .map(|comment| rows_for_comment(comment, threads))
        .sum()
}

fn row_of_rpid(
    comments: &[Comment],
    threads: &HashMap<u64, ReplyThread>,
    rpid: u64,
) -> Option<usize> {
    let mut row = 0;
    for comment in comments {
        if comment.rpid == rpid {
            return Some(row);
        }
        let replies = visible_replies(comment, threads);
        if let Some(reply) = replies.iter().position(|reply| reply.rpid == rpid) {
            return Some(row + 1 + reply);
        }
        // Replies of a reply stay inside its row, where at most
        // `NESTED_COMMENT_LIMIT` of them are rendered.
        for (reply, nested) in replies.iter().enumerate() {
            if nested.replies().iter().any(|nested| nested.rpid == rpid) {
                return Some(row + 1 + reply);
            }
        }
        row += 1 + replies.len() + usize::from(has_thread_control(comment, threads));
    }
    None
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
        COMMENT_RETAIN_PAGES, CommentRow, EmoteTarget, ReplyThread, build_comment_rows,
        comment_row_start, drain_back_pages, drain_front_pages, row_of_rpid, thread_splice,
    };
    use crate::info::{InfoPanel, InfoSelections};
    use gpui_kit::prelude::*;
    use gpui_kit::{
        Bounds, ListAlignment, ListState, TestAppContext, WindowBounds, WindowOptions, px, size,
    };
    use rigili_bilibili::Comment;
    use std::collections::{HashMap, VecDeque};

    fn comment(rpid: u64, rcount: u64, replies: Vec<Comment>) -> Comment {
        Comment {
            rpid,
            rcount,
            replies: Some(replies),
            ..Default::default()
        }
    }

    fn plain(rpid: u64) -> Comment {
        Comment {
            rpid,
            ..Default::default()
        }
    }

    fn thread(replies: Vec<Comment>) -> ReplyThread {
        ReplyThread {
            replies,
            ..Default::default()
        }
    }

    #[test]
    fn splits_threads_into_rows() {
        let comments = vec![
            comment(10, 0, Vec::new()),
            comment(20, 5, vec![plain(21), plain(22), plain(23)]),
            comment(30, 5, Vec::new()),
        ];

        assert_eq!(
            build_comment_rows(&comments, &HashMap::new()),
            vec![
                CommentRow::Comment(0),
                CommentRow::Comment(1),
                CommentRow::Reply { root: 1, reply: 0 },
                CommentRow::Reply { root: 1, reply: 1 },
                CommentRow::Control(1),
                CommentRow::Comment(2),
                CommentRow::Control(2),
                CommentRow::Tail,
            ]
        );

        let mut threads = HashMap::new();
        threads.insert(30, thread(vec![plain(31), plain(32), plain(33)]));
        assert_eq!(
            build_comment_rows(&comments, &threads),
            vec![
                CommentRow::Comment(0),
                CommentRow::Comment(1),
                CommentRow::Reply { root: 1, reply: 0 },
                CommentRow::Reply { root: 1, reply: 1 },
                CommentRow::Control(1),
                CommentRow::Comment(2),
                CommentRow::Reply { root: 2, reply: 0 },
                CommentRow::Reply { root: 2, reply: 1 },
                CommentRow::Reply { root: 2, reply: 2 },
                CommentRow::Control(2),
                CommentRow::Tail,
            ]
        );
    }

    #[test]
    fn splices_only_the_rows_that_change() {
        // The first page replaces the control under the two preview replies.
        assert_eq!(thread_splice(4, (2, true), (20, true)), (7, 1, 19));
        // Another page appends below twenty replies and the control.
        assert_eq!(thread_splice(4, (20, true), (40, true)), (25, 1, 21));
        // Collapsing drops every reply and the control.
        assert_eq!(thread_splice(4, (20, true), (2, false)), (7, 19, 0));
        // A control that appears or disappears changes the count by one.
        assert_eq!(thread_splice(4, (2, false), (2, true)), (7, 0, 1));
        // Repainting the control keeps the count, so it is remeasured.
        assert_eq!(thread_splice(4, (2, true), (2, true)), (7, 1, 1));
    }

    #[test]
    fn appending_a_page_keeps_one_tail_row() {
        let first = vec![plain(1), plain(2)];
        let threads = HashMap::new();
        let previous = build_comment_rows(&first, &threads).len();
        let list = ListState::new(previous, ListAlignment::Top, px(100.));

        let mut comments = first;
        comments.extend([plain(3), plain(4)]);
        let rows = build_comment_rows(&comments, &threads);
        list.splice(previous - 1..previous - 1, rows.len() - previous);

        assert_eq!(list.item_count(), rows.len());
        assert_eq!(rows.last(), Some(&CommentRow::Tail));
    }

    #[test]
    fn row_starts_match_the_built_rows() {
        let comments = vec![
            comment(1, 0, Vec::new()),
            comment(2, 5, vec![plain(3)]),
            comment(4, 0, Vec::new()),
        ];
        let mut threads = HashMap::new();
        threads.insert(2, thread(vec![plain(5), plain(6)]));

        let rows = build_comment_rows(&comments, &threads);
        for index in 0..comments.len() {
            assert_eq!(
                rows[comment_row_start(&comments, &threads, index)],
                CommentRow::Comment(index)
            );
        }
        assert_eq!(
            comment_row_start(&comments, &threads, comments.len()),
            rows.len() - 1
        );
    }

    #[test]
    fn locates_root_reply_and_nested_rows() {
        let mut root = comment(1, 4, Vec::new());
        root.replies = Some(vec![Comment {
            rpid: 2,
            replies: Some(vec![plain(9)]),
            ..Default::default()
        }]);
        let comments = vec![root, comment(3, 0, Vec::new())];
        let mut threads = HashMap::new();
        threads.insert(
            1,
            thread(vec![
                Comment {
                    rpid: 2,
                    replies: Some(vec![plain(9)]),
                    ..Default::default()
                },
                plain(4),
            ]),
        );

        assert_eq!(row_of_rpid(&comments, &threads, 1), Some(0));
        assert_eq!(row_of_rpid(&comments, &threads, 2), Some(1));
        assert_eq!(row_of_rpid(&comments, &threads, 4), Some(2));
        assert_eq!(row_of_rpid(&comments, &threads, 9), Some(1));
        assert_eq!(row_of_rpid(&comments, &threads, 3), Some(4));
        assert_eq!(row_of_rpid(&comments, &threads, 99), None);
    }

    #[gpui_kit::test]
    fn opens_one_emote_picker_at_a_time(cx: &mut TestAppContext) {
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
            panel.set_emote_picker_open(EmoteTarget::Comment, true, cx);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Comment));

            panel.set_emote_picker_open(EmoteTarget::Reply, true, cx);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Reply));

            panel.set_emote_picker_open(EmoteTarget::Reply, false, cx);
            assert_eq!(panel.emote_target, None);

            panel.set_emote_picker_open(EmoteTarget::Comment, true, cx);
            panel.close_emote_picker(EmoteTarget::Reply);
            assert_eq!(panel.emote_target, Some(EmoteTarget::Comment));

            panel.close_emote_picker(EmoteTarget::Comment);
            assert_eq!(panel.emote_target, None);
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
