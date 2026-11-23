rigili_macro::mod_prv!(composer, list, thread);

use std::collections::VecDeque;

use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::CommentPage;
use rigili_bilibili::api::comment::CommentSort;
use rigili_bilibili::{Comment, CommentPicture};
use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_core::{api_error_message, api_task_error_message};

pub(super) use self::thread::CommentThread;
use super::nav::{clamp_selection, stepped_selection};
use super::{InfoPanel, InfoTab};

const COMMENT_RETAIN_PAGES: usize = 12;
const PREVIEW_REPLIES: usize = 2;

/// The replies of one comment, shown in their own view.
///
/// The reply endpoint returns the whole chain under a root as one flat page of
/// comments, so this needs no nested list: every reply, including a reply to a
/// reply, is a row of the same list.

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
        self.comments_selection = None;
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
        self.thread_selection = None;
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
            comment.rpid == rpid || comment.replies().iter().any(|reply| reply.rpid == rpid)
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
        self.comments_selection = None;
        self.comments_more = false;
        self.comments_pages.clear();
        self.comments_first_page = 1;
        self.comments_earlier_loading = false;
        self.comments_earlier_error = None;
        self.thread = None;
        self.thread_selection = None;
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
        self.comments_selection = self
            .comments_selection
            .map(|index| index.saturating_sub(dropped));
        self.clamp_comments_selection();
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
        self.clamp_comments_selection();
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
                            this.comments_selection = this
                                .comments_selection
                                .map(|index| index.saturating_add(count));
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
                self.focus_composer(window, cx);
            }
            return;
        }
        self.reply_target = target;
        self.composer_error = None;
        self.emote_open = false;
        self.thread = Some(CommentThread::new(root));
        self.thread_selection = Some(0);
        self.thread_list.reset(2);
        if let Some(draft) = &self.draft {
            draft.update(cx, |state, cx| {
                state.set_placeholder("回复评论", window, cx)
            });
        }
        if focus {
            self.focus_composer(window, cx);
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

    fn close_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.thread = None;
        self.thread_selection = None;
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

    pub(crate) fn back_in_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.thread.is_none() {
            return false;
        }
        self.close_thread(window, cx);
        true
    }

    pub(crate) fn comments_focused(&self, window: &Window) -> bool {
        self.tab == InfoTab::Comments && self.comments_focus.focused(window)
    }

    pub(crate) fn focus_comments(&mut self, cx: &mut Context<Self>) {
        if self.tab != InfoTab::Comments || self.comments.is_empty() {
            return;
        }
        let visible = self.comments_list.logical_scroll_top().item_ix;
        self.comments_selection = clamp_selection(
            self.comments_selection.or(Some(visible)),
            self.comments.len(),
        );
        if let Some(index) = self.comments_selection {
            self.comments_list.scroll_to_reveal_item(index);
        }
        self.comments_focus.request_focus();
        cx.notify();
    }

    pub(crate) fn step_comment_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.thread.is_some() {
            self.step_thread_selection(delta, cx);
            return;
        }
        let next = stepped_selection(self.comments_selection, delta, self.comments.len());
        if next == self.comments_selection {
            return;
        }
        self.comments_selection = next;
        if let Some(index) = next {
            self.comments_list.scroll_to_reveal_item(index);
        }
        cx.notify();
    }

    fn step_thread_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(thread) = &self.thread else {
            return;
        };
        let selectable = thread.row_count() - 1;
        let next = stepped_selection(self.thread_selection, delta, selectable);
        if next == self.thread_selection {
            return;
        }
        self.thread_selection = next;
        if let Some(index) = next {
            self.thread_list.scroll_to_reveal_item(index);
        }
        cx.notify();
    }

    pub(crate) fn open_selected_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread.is_some() {
            return;
        }
        let Some(index) = self.comments_selection else {
            return;
        };
        self.open_thread(index, false, None, window, cx);
    }

    pub(crate) fn open_selected_comment_picture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pictures) = selected_comment_pictures(
            self.thread.as_ref(),
            self.thread_selection,
            &self.comments,
            self.comments_selection,
        ) else {
            return;
        };
        rigili_core::comment_picture::open(pictures, 0, window, cx);
    }

    fn clamp_comments_selection(&mut self) {
        self.comments_selection = clamp_selection(self.comments_selection, self.comments.len());
    }

    fn clamp_thread_selection(&mut self) {
        let selectable = self
            .thread
            .as_ref()
            .map_or(0, |thread| thread.row_count() - 1);
        self.thread_selection = clamp_selection(self.thread_selection, selectable);
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
            if let Some(reply) = thread.replies.iter_mut().find(|reply| reply.rpid == rpid) {
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
}

fn selected_comment_pictures<'a>(
    thread: Option<&'a CommentThread>,
    thread_selection: Option<usize>,
    comments: &'a [Comment],
    comments_selection: Option<usize>,
) -> Option<&'a [CommentPicture]> {
    let comment = if let Some(thread) = thread {
        let index = thread_selection?;
        if index == 0 {
            &thread.root
        } else {
            thread.replies.get(index - 1)?
        }
    } else {
        comments.get(comments_selection?)?
    };
    (!comment.content.pictures.is_empty()).then_some(comment.content.pictures.as_slice())
}

fn selection_outline(primary: Hsla, radius: Pixels, active: bool) -> impl IntoElement {
    let color = if active {
        primary
    } else {
        primary.opacity(0.4)
    };
    div()
        .absolute()
        .top(px(-6.))
        .bottom_0()
        .left(px(-8.))
        .right(px(-8.))
        .rounded(radius)
        .border_2()
        .border_color(color)
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
    use super::list::has_hidden_replies;
    use super::{
        COMMENT_RETAIN_PAGES, CommentThread, drain_back_pages, drain_front_pages,
        selected_comment_pictures,
    };
    use crate::info::{InfoPanel, InfoSelections, InfoTab};
    use gpui_kit::TestSupportExt as _;
    use gpui_kit::base::v_flex;
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Bounds, Entity, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use rigili_bilibili::{Comment, CommentPicture, EmoteItem, EmotePackage, EmotePanel};
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
    fn finds_the_picture_of_the_focused_comment() {
        fn pictured(rpid: u64) -> Comment {
            let mut comment = plain(rpid);
            comment.content.pictures = vec![CommentPicture {
                img_src: "//i0.hdslb.com/bfs/new_dyn/test.png".into(),
                ..Default::default()
            }];
            comment
        }

        let list = [pictured(1), plain(2)];
        assert!(selected_comment_pictures(None, None, &list, Some(0)).is_some());
        assert!(selected_comment_pictures(None, None, &list, Some(1)).is_none());
        assert!(selected_comment_pictures(None, None, &list, None).is_none());

        let thread = CommentThread {
            root: pictured(1),
            replies: vec![plain(2), pictured(3)],
            page: 1,
            total: 2,
            loading: false,
            error: None,
        };
        assert!(selected_comment_pictures(Some(&thread), Some(0), &list, None).is_some());
        assert!(selected_comment_pictures(Some(&thread), Some(1), &list, None).is_none());
        assert!(selected_comment_pictures(Some(&thread), Some(2), &list, None).is_some());
        assert!(selected_comment_pictures(Some(&thread), Some(3), &list, None).is_none());
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, _| {
            panel.comments = vec![comment(10, 2), comment(20, 0)];
            panel.comments_list.reset(panel.comments.len() + 1);
        });

        cx.update_window(window, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.open_thread(1, false, None, window, cx)
            });
        })
        .expect("open thread");

        cx.update_window(window, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                assert_eq!(
                    panel.thread.as_ref().map(|thread| thread.root.rpid),
                    Some(20)
                );
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
                .child(div().id("emote-picker-probe").test_support().size(px(10.)))
                .child(
                    self.panel
                        .update(cx, |panel, cx| panel.render_emote_picker(cx)),
                )
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
                    let panel = cx.new(|cx| InfoPanel::new(selections, cx));
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
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

    #[gpui_kit::test]
    fn keyboard_navigation_steps_and_opens_comments(cx: &mut TestAppContext) {
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, cx| {
            panel.tab = InfoTab::Comments;
            panel.comments = vec![comment(10, 2), comment(20, 0), comment(30, 0)];
            panel.comments_list.reset(4);

            panel.step_comment_selection(1, cx);
            assert_eq!(panel.comments_selection, Some(0));
            panel.step_comment_selection(1, cx);
            assert_eq!(panel.comments_selection, Some(1));
            panel.step_comment_selection(1, cx);
            assert_eq!(panel.comments_selection, Some(2));
            panel.step_comment_selection(1, cx);
            assert_eq!(panel.comments_selection, Some(2));
            panel.step_comment_selection(-3, cx);
            assert_eq!(panel.comments_selection, Some(0));
        });

        cx.update_window(window, |_, window, cx| {
            panel.update(cx, |panel, cx| {
                panel.open_selected_thread(window, cx);
                assert_eq!(
                    panel.thread.as_ref().map(|thread| thread.root.rpid),
                    Some(10)
                );
                assert_eq!(panel.thread_selection, Some(0));

                let count = {
                    let thread = panel.thread.as_mut().expect("open thread");
                    thread.replies = vec![plain(11), plain(12)];
                    thread.total = 2;
                    thread.row_count()
                };
                panel.thread_list.reset(count);

                panel.step_comment_selection(1, cx);
                assert_eq!(panel.thread_selection, Some(1));
                assert_eq!(panel.comments_selection, Some(0));
                panel.step_comment_selection(3, cx);
                assert_eq!(panel.thread_selection, Some(2));
                panel.step_comment_selection(-5, cx);
                assert_eq!(panel.thread_selection, Some(0));

                assert!(panel.back_in_comments(window, cx));
                assert!(panel.thread.is_none());
                assert_eq!(panel.thread_selection, None);
                assert_eq!(panel.comments_selection, Some(0));
            });
        })
        .expect("open and close the thread");
    }

    #[gpui_kit::test]
    fn keeps_the_selected_comment_across_page_trims(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
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
                    cx.new(|cx| InfoPanel::new(selections, cx))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, _| {
            panel.comments = (1..=30u64).map(plain).collect();
            panel.comments_list.reset(31);
            panel.comments_selection = Some(25);
            panel.comments_pages = VecDeque::from(vec![10; COMMENT_RETAIN_PAGES + 2]);

            panel.trim_front_pages();
            assert_eq!(panel.comments.len(), 10);
            assert_eq!(panel.comments_selection, Some(5));

            panel.comments.truncate(5);
            panel.comments_list.reset(6);
            panel.clamp_comments_selection();
            assert_eq!(panel.comments_selection, Some(4));
        });
    }

    #[gpui_kit::test]
    fn keyboard_navigation_keeps_the_selected_comment_visible(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        cx.update(rigili_core::client::init);
        let (window, panel) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(500.), px(500.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|cx| InfoPanel::new(selections, cx))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, _| {
            panel.details = Some(std::sync::Arc::new(crate::info::VideoDetails::new(
                Default::default(),
            )));
            panel.tab = InfoTab::Comments;
            panel.comments = (1..=20u64).map(plain).collect();
            panel.comments_list.reset(21);
            panel.comments_loaded = true;
            panel.comments_selection = Some(0);
        });

        draw(cx, window);
        for _ in 0..8 {
            panel.update(cx, |panel, cx| panel.step_comment_selection(1, cx));
            draw(cx, window);
        }

        panel.read_with(cx, |panel, _| {
            assert_eq!(panel.comments_selection, Some(8));
            assert!(
                panel.comments_list.logical_scroll_top().item_ix > 0,
                "the comment list did not follow the selection"
            );
        });
    }

    fn draw(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .expect("draw info panel");
        cx.run_until_parked();
    }
}
