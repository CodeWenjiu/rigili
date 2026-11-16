use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::CommentSort;
use rigili_bilibili::{Client, Comment, CommentPage, DynamicItem, avatar};
use rigili_core::client;
use rigili_core::{format_count, format_date, message_markdown};

use crate::shared::browse::paging::{Page, Paged, spawn_first, spawn_more};

const COMMENT_OVERDRAW: Pixels = px(480.);
const COMMENT_MAX_HEIGHT: Pixels = px(420.);

pub(super) struct CommentsView {
    client: Option<Arc<Client>>,
    dynamic: DynamicItem,
    sort: CommentSort,
    state: Paged<Comment, CommentMeta>,
    list: ListState,
    synced_generation: u64,
    synced_count: usize,
    tail: TailState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TailState {
    loading: bool,
    loaded: bool,
    error: bool,
    loading_more: bool,
    more_error: bool,
    has_more: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommentMeta {
    pub(crate) page: u32,
    pub(crate) total: u64,
}

impl CommentsView {
    pub(super) fn new(dynamic: DynamicItem) -> Self {
        Self {
            client: None,
            dynamic,
            sort: CommentSort::default(),
            state: Paged::new(),
            list: ListState::new(1, ListAlignment::Top, COMMENT_OVERDRAW),
            synced_generation: 0,
            synced_count: 0,
            tail: TailState::default(),
        }
    }

    fn cached_client(&mut self, cx: &mut Context<Self>) -> Result<Arc<Client>, String> {
        client::cached(&mut self.client, cx)
    }

    pub(super) fn load(&mut self, reset: bool, cx: &mut Context<Self>) {
        if reset {
            self.load_first(cx);
        } else {
            self.load_more(cx);
        }
    }

    fn load_first(&mut self, cx: &mut Context<Self>) {
        let (kind, oid) = self.dynamic.comment_target();
        let oid = oid.to_owned();
        let sort = self.sort;
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.state.fail_first_reset(error);
                cx.notify();
                return;
            }
        };

        spawn_first(
            self,
            cx,
            |this| &mut this.state,
            |comment: &Comment| comment.rpid.to_string(),
            async move { comment_page(client.comments(kind, &oid, 1, sort).await?, 1) },
            comments_applied,
        );
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let (kind, oid) = self.dynamic.comment_target();
        let oid = oid.to_owned();
        let sort = self.sort;
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.state.fail_more(error);
                cx.notify();
                return;
            }
        };

        spawn_more(
            self,
            cx,
            |this| &mut this.state,
            |comment: &Comment| comment.rpid.to_string(),
            move |meta| {
                let page = meta.page + 1;
                async move { comment_page(client.comments(kind, &oid, page, sort).await?, page) }
            },
            comments_applied,
        );
    }

    fn set_sort(&mut self, sort: CommentSort, cx: &mut Context<Self>) {
        if self.sort == sort {
            return;
        }
        self.sort = sort;
        self.load(true, cx);
    }

    fn retry_more(&mut self, cx: &mut Context<Self>) {
        self.state.retry_more();
        self.load(false, cx);
    }

    fn render_sort_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;

        h_flex()
            .items_center()
            .gap_4()
            .children(
                [
                    (CommentSort::Hot, "最热", 0usize),
                    (CommentSort::Latest, "最新", 1usize),
                ]
                .map(|(sort, label, index)| {
                    let active = self.sort == sort;
                    let group = SharedString::from(format!("comment-sort-{index}"));
                    Button::new(("comment-sort", index))
                        .text()
                        .small()
                        .px_1()
                        .accessibility_label(label)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_sort(sort, cx)))
                        .child(
                            div()
                                .group(group.clone())
                                .text_sm()
                                .text_color(if active { primary } else { muted })
                                .group_hover(group, move |this| this.text_color(primary))
                                .child(label),
                        )
                }),
            )
            .into_any_element()
    }

    fn sync_rows(&mut self) {
        let count = self.state.items.len();
        if self.synced_generation != self.state.generation || count < self.synced_count {
            self.list.reset(count + 1);
        } else if count > self.synced_count {
            self.list.splice(
                self.synced_count..self.synced_count,
                count - self.synced_count,
            );
            self.list.remeasure_items(count..count + 1);
        }
        self.synced_generation = self.state.generation;
        self.synced_count = count;

        let tail = TailState {
            loading: self.state.loading,
            loaded: self.state.loaded,
            error: self.state.error.is_some(),
            loading_more: self.state.loading_more,
            more_error: self.state.more_error.is_some(),
            has_more: self.state.has_more,
        };
        if tail != self.tail {
            self.tail = tail;
            self.list.remeasure_items(count..count + 1);
        }
    }

    fn render_row(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(comment) = self.state.items.get(index) {
            return div()
                .w_full()
                .border_b_1()
                .border_color(cx.theme().border)
                .pb_3()
                .child(self.render_comment(comment, 0, cx))
                .into_any_element();
        }
        self.render_tail(cx)
    }

    fn render_tail(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.state.items.is_empty() {
            self.render_initial_state(cx)
        } else {
            self.render_footer(cx)
        }
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
        let nested = comment.replies();
        let nested_views = nested
            .iter()
            .take(2)
            .map(|reply| self.render_comment(reply, depth + 1, cx))
            .collect::<Vec<_>>();
        let avatar_size = if depth == 0 { 32. } else { 24. };
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

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_3()
                    .child(
                        Avatar::new()
                            .src(avatar(&member.avatar, 64))
                            .name(member.uname.clone())
                            .size(px(avatar_size)),
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
                                            .child(Icon::new(IconName::ThumbsUp).with_size(px(12.)))
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
            .when(!nested_views.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(44.)).children(nested_views))
            })
            .into_any_element()
    }

    fn render_initial_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.state.loading && self.state.items.is_empty() {
            return h_flex()
                .py_6()
                .justify_center()
                .child(Spinner::new())
                .into_any_element();
        }
        if let Some(error) = self.state.error.clone() {
            return h_flex()
                .items_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load(true, cx))),
                )
                .into_any_element();
        }
        if self.state.loaded && self.state.items.is_empty() {
            return h_flex()
                .py_6()
                .justify_center()
                .text_sm()
                .text_color(muted)
                .child("暂无评论")
                .into_any_element();
        }
        div().into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.state.loading_more {
            return h_flex()
                .py_2()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element();
        }
        if let Some(error) = self.state.more_error.clone() {
            return h_flex()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-more-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_more(cx))),
                )
                .into_any_element();
        }
        if self.state.has_more {
            return h_flex()
                .justify_center()
                .child(
                    Button::new("comment-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load(false, cx))),
                )
                .into_any_element();
        }
        if self.state.loaded {
            return h_flex()
                .py_2()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("没有更多评论")
                .into_any_element();
        }
        div().into_any_element()
    }
}

fn comment_page(
    page: CommentPage,
    requested: u32,
) -> Result<Page<Comment, CommentMeta>, rigili_bilibili::ApiError> {
    let CommentPage {
        page: info,
        replies,
    } = page;
    let incoming = replies.unwrap_or_default();
    let has_more = !incoming.is_empty();
    let number = if info.num == 0 { requested } else { info.num };
    Ok(Page::new(
        incoming,
        has_more,
        CommentMeta {
            page: number,
            total: info.count.max(info.acount),
        },
    ))
}

fn comments_applied(this: &mut CommentsView) {
    let state = &mut this.state;
    state.has_more = state.has_more && (state.items.len() as u64) < state.meta.total;
}

impl Render for CommentsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_rows();
        let muted = cx.theme().muted_foreground;
        let sort_tabs = self.render_sort_tabs(cx);
        let total = if self.state.loaded {
            Some(format!("共 {} 条评论", format_count(self.state.meta.total)))
        } else {
            None
        };

        v_flex()
            .w_full()
            .gap_3()
            .image_cache(rigili_core::image_cache::avatars(cx))
            .child(
                h_flex()
                    .items_center()
                    .gap_4()
                    .child(sort_tabs)
                    .child(div().flex_1())
                    .when_some(total, |this, total| {
                        this.child(div().text_xs().text_color(muted).child(total))
                    }),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .occlude()
                    .child(
                        list(self.list.clone(), cx.processor(Self::render_row))
                            .w_full()
                            .max_h(COMMENT_MAX_HEIGHT)
                            .with_sizing_behavior(ListSizingBehavior::Infer),
                    )
                    .child(Scrollbar::vertical(&self.list)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::CommentsView;
    use gpui_kit::{TestAppContext, px};
    use rigili_bilibili::{Comment, CommentContent, DynamicItem};

    use crate::shared::test_support::{frames, open};

    fn view() -> CommentsView {
        CommentsView::new(DynamicItem::default())
    }

    #[test]
    fn tracks_rows_as_pages_are_replaced_and_appended() {
        let mut view = view();
        assert_eq!(view.list.item_count(), 1);

        view.state.generation = 1;
        view.state.items = vec![Comment::default(), Comment::default()];
        view.sync_rows();
        assert_eq!(view.list.item_count(), 3);

        view.state.items.push(Comment::default());
        view.sync_rows();
        assert_eq!(view.list.item_count(), 4);

        view.state.generation = 2;
        view.state.items.truncate(1);
        view.sync_rows();
        assert_eq!(view.list.item_count(), 2);
    }

    #[gpui_kit::test]
    fn caps_the_inline_comment_height(cx: &mut TestAppContext) {
        cx.update(rigili_core::image_cache::init);
        let (window, view) = open(cx, |_| {
            let mut view = view();
            view.state.generation = 1;
            view.state.loaded = true;
            view.state.items = (0..40)
                .map(|_| Comment {
                    content: CommentContent {
                        message: "这是一条很长的评论。".repeat(40),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .collect();
            view
        });

        frames(cx, window, 4);

        let height = cx.update(|cx| view.read(cx).list.viewport_bounds().size.height);
        assert!(height > px(0.), "the list should lay out: {height:?}");
        assert!(height <= px(420.), "the list should be capped: {height:?}");
    }

    #[gpui_kit::test]
    fn leaves_short_inline_lists_compact(cx: &mut TestAppContext) {
        cx.update(rigili_core::image_cache::init);
        let (window, view) = open(cx, |_| {
            let mut view = view();
            view.state.generation = 1;
            view.state.loaded = true;
            view.state.items = vec![Comment::default()];
            view
        });

        frames(cx, window, 6);

        let height = cx.update(|cx| view.read(cx).list.viewport_bounds().size.height);
        assert!(height > px(0.), "the list should lay out: {height:?}");
        assert!(
            height < px(420.),
            "a short list should not fill the cap: {height:?}"
        );
    }
}
