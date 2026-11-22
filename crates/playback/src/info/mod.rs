rigili_macro::mod_prv!(
    coin,
    comments,
    details,
    dialog,
    favorite,
    header,
    interaction,
    render,
    triple
);

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::{TextSelection, TextSelectionHandle};
use gpui_kit::component::input::TextareaState;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{ArchiveRelation, Comment, RecommendVideo, VideoInfo, VideoPage};
use rigili_core::{format_count, format_datetime};

use comments::CommentThread;
use details::LikePress;
use interaction::{CoinState, FlagState, ToggleState};

pub(crate) use details::short_label;

const INFO_WIDTH: Pixels = px(340.);
const RELATED_LIMIT: usize = 20;
const RELATED_THUMB_WIDTH: Pixels = px(120.);
const RELATED_THUMB_HEIGHT: Pixels = px(68.);
const COMMENT_AVATAR: Pixels = px(28.);
const NESTED_COMMENT_AVATAR: Pixels = px(20.);
const COMMENT_MAX_LEN: usize = 1000;
const COMMENT_LIST_OVERDRAW: Pixels = px(480.);

pub(super) enum InfoEvent {
    SelectPage(u64),
    PlayRelated {
        bvid: String,
        cid: u64,
        title: String,
        new_tab: bool,
    },
    ToggleFollow {
        mid: u64,
        follow: bool,
    },
    ToggleLike {
        aid: u64,
        like: bool,
    },
    OpenSpace {
        mid: u64,
        name: String,
    },
    Reload,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum InfoTab {
    #[default]
    Info,
    Comments,
}

struct VideoDetails {
    aid: u64,
    bvid: SharedString,
    owner_mid: u64,
    owner: SharedString,
    avatar: SharedString,
    title: SharedString,
    views: SharedString,
    danmaku: SharedString,
    pubdate: SharedString,
    description: SharedString,
    no_reprint: bool,
    pages: Arc<Vec<VideoPage>>,
}

impl VideoDetails {
    fn new(info: VideoInfo) -> Self {
        let VideoInfo {
            aid,
            bvid,
            title,
            pubdate,
            owner,
            stat,
            rights,
            desc,
            pages,
            ..
        } = info;
        Self {
            aid,
            bvid: bvid.into(),
            owner_mid: owner.mid,
            owner: owner.name.into(),
            avatar: owner.face.into(),
            title: title.into(),
            views: format_count(stat.view).into(),
            danmaku: format_count(stat.danmaku).into(),
            pubdate: format_datetime(pubdate).into(),
            description: desc.into(),
            no_reprint: rights.no_reprint != 0,
            pages: Arc::new(pages),
        }
    }
}

pub(super) struct InfoPanel {
    details: Option<Arc<VideoDetails>>,
    selected_cid: Option<u64>,
    busy: bool,
    scroll: ScrollHandle,
    related: Vec<RecommendVideo>,
    related_selection: Option<usize>,
    related_focus: FocusHandle,
    related_focus_subs: Option<(Subscription, Subscription)>,
    related_focus_pending: bool,
    related_layout: Rc<RefCell<RelatedLayout>>,
    description_expanded: bool,
    follow: FlagState,
    followers: Option<u64>,
    owner_likes: Option<u64>,
    like: ToggleState,
    coin: CoinState,
    favorite: ToggleState,
    triple_busy: bool,
    triple_pop: Option<u64>,
    triple_pop_generation: u64,
    like_press: Option<LikePress>,
    like_hold: Option<Task<()>>,
    like_press_generation: u64,
    share_count: u64,
    tab: InfoTab,
    comment_count: u64,
    comments: Vec<Comment>,
    comments_selection: Option<usize>,
    comments_focus: FocusHandle,
    comments_focus_subs: Option<(Subscription, Subscription)>,
    comments_focus_pending: bool,
    comments_list: ListState,
    thread: Option<CommentThread>,
    thread_selection: Option<usize>,
    thread_list: ListState,
    comments_loading: bool,
    comments_loaded: bool,
    comments_more: bool,
    comments_page: u32,
    comments_total: u64,
    comments_error: Option<String>,
    comments_more_loading: bool,
    comments_more_error: Option<String>,
    comments_pages: VecDeque<usize>,
    comments_first_page: u32,
    comments_earlier_loading: bool,
    comments_earlier_error: Option<String>,
    draft: Option<Entity<TextareaState>>,
    _draft_sub: Option<Subscription>,
    composer_sending: bool,
    composer_error: Option<String>,
    reply_target: Option<u64>,
    emote_open: bool,
    emote_package: usize,
    watch_later: FlagState,
    refresh_busy: bool,
    refresh_error: Option<String>,
    selections: InfoSelections,
}

#[derive(Clone)]
pub(super) struct InfoSelections {
    pub(super) title: TextSelectionHandle,
    pub(super) bvid: TextSelectionHandle,
    pub(super) no_reprint: TextSelectionHandle,
    pub(super) description: TextSelectionHandle,
}

impl InfoSelections {
    pub(super) fn new(window: &Window, cx: &mut App) -> Self {
        fn selection(window: &Window, cx: &mut App) -> TextSelectionHandle {
            let handle = TextSelectionHandle::new("", cx);
            handle.refresh_window_on_change(window, cx).detach();
            handle
        }

        let selections = Self {
            title: selection(window, cx),
            bvid: selection(window, cx),
            no_reprint: selection(window, cx),
            description: selection(window, cx),
        };
        let window_handle = window.window_handle();
        for handle in [
            &selections.title,
            &selections.bvid,
            &selections.no_reprint,
            &selections.description,
        ] {
            let selections = selections.clone();
            handle
                .subscribe(
                    move |_, cx| selections.clear_spanning(window_handle, cx),
                    cx,
                )
                .detach();
        }
        selections
    }

    fn spans_blocks(&self, cx: &App) -> bool {
        let selected = [
            self.title.snapshot(cx).is_some(),
            self.bvid.snapshot(cx).is_some() || self.no_reprint.snapshot(cx).is_some(),
            self.description.snapshot(cx).is_some(),
        ];
        selected.into_iter().filter(|selected| *selected).count() > 1
    }

    fn clear_spanning(&self, window: AnyWindowHandle, cx: &mut App) {
        if self.spans_blocks(cx) {
            let _ = window.update(cx, |_, window, cx| self.clear_spanning_in(window, cx));
        }
    }

    pub(super) fn clear_spanning_in(&self, window: &mut Window, cx: &mut App) {
        if self.spans_blocks(cx) {
            TextSelection::clear(window, cx);
        }
    }
}

#[derive(Default)]
struct RelatedLayout {
    bounds: Vec<Bounds<Pixels>>,
    offset: Pixels,
    pending: Option<usize>,
}

impl EventEmitter<InfoEvent> for InfoPanel {}

impl InfoPanel {
    pub(super) fn new(selections: InfoSelections, cx: &mut Context<Self>) -> Self {
        Self {
            selections,
            details: None,
            selected_cid: None,
            busy: false,
            scroll: ScrollHandle::default(),
            related: Vec::new(),
            related_selection: None,
            related_focus: cx.focus_handle(),
            related_focus_subs: None,
            related_focus_pending: false,
            related_layout: Rc::new(RefCell::new(RelatedLayout::default())),
            description_expanded: false,
            follow: FlagState::default(),
            followers: None,
            owner_likes: None,
            like: ToggleState::default(),
            coin: CoinState::default(),
            favorite: ToggleState::default(),
            triple_busy: false,
            triple_pop: None,
            triple_pop_generation: 0,
            like_press: None,
            like_hold: None,
            like_press_generation: 0,
            share_count: 0,
            tab: InfoTab::Info,
            comment_count: 0,
            comments: Vec::new(),
            comments_selection: None,
            comments_focus: cx.focus_handle(),
            comments_focus_subs: None,
            comments_focus_pending: false,
            comments_list: ListState::new(1, ListAlignment::Top, COMMENT_LIST_OVERDRAW),
            thread: None,
            thread_selection: None,
            thread_list: ListState::new(1, ListAlignment::Top, COMMENT_LIST_OVERDRAW),
            comments_loading: false,
            comments_loaded: false,
            comments_more: false,
            comments_page: 1,
            comments_total: 0,
            comments_error: None,
            comments_more_loading: false,
            comments_more_error: None,
            comments_pages: VecDeque::new(),
            comments_first_page: 1,
            comments_earlier_loading: false,
            comments_earlier_error: None,
            draft: None,
            _draft_sub: None,
            composer_sending: false,
            composer_error: None,
            reply_target: None,
            emote_open: false,
            emote_package: 0,
            watch_later: FlagState::default(),
            refresh_busy: false,
            refresh_error: None,
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        *self = Self::new(self.selections.clone(), cx);
        self.tab = tab;
        cx.notify();
    }

    pub(super) fn aid(&self) -> Option<u64> {
        self.details.as_ref().map(|details| details.aid)
    }

    pub(super) fn page_duration(&self, cid: u64) -> Option<u64> {
        self.details
            .as_ref()?
            .pages
            .iter()
            .find(|page| page.cid == cid)
            .map(|page| page.duration)
    }

    pub(super) fn set(&mut self, info: VideoInfo, cid: u64, cx: &mut Context<Self>) {
        let stat = info.stat.clone();
        self.details = Some(Arc::new(VideoDetails::new(info)));
        self.selected_cid = Some(cid);
        self.busy = false;
        self.related.clear();
        self.related_selection = None;
        self.reset_related_layout();
        self.description_expanded = false;
        self.follow = FlagState::default();
        self.followers = None;
        self.owner_likes = None;
        self.like = ToggleState::with_count(stat.like);
        self.coin = CoinState::with_count(stat.coin);
        self.favorite = ToggleState::with_count(stat.favorite);
        self.share_count = stat.share;
        self.comment_count = stat.reply;
        self.refresh_busy = false;
        self.refresh_error = None;
        self.reset_comments();
        self.watch_later = FlagState::default();
        self.scroll.set_offset(point(px(0.), px(0.)));
        if self.tab == InfoTab::Comments {
            self.load_comments(cx);
        }
        cx.notify();
    }

    pub(super) fn refresh(
        &mut self,
        info: VideoInfo,
        related: Vec<RecommendVideo>,
        followers: u64,
        likes: u64,
        cx: &mut Context<Self>,
    ) {
        let stat = info.stat.clone();
        let cid = info.cid;
        let details = VideoDetails::new(info);
        let pages = details.pages.clone();
        self.details = Some(Arc::new(details));
        if !pages.iter().any(|page| Some(page.cid) == self.selected_cid) {
            self.selected_cid = Some(cid);
        }
        self.followers = Some(followers);
        self.owner_likes = Some(likes);
        self.set_related(related, cx);
        self.like.accept_count(stat.like);
        self.coin.accept_count(stat.coin);
        self.favorite.accept_count(stat.favorite);
        self.share_count = stat.share;
        self.comment_count = stat.reply;
        self.busy = false;
        self.refresh_busy = false;
        self.refresh_error = None;
        cx.notify();
    }

    pub(super) fn select(&mut self, cid: u64, cx: &mut Context<Self>) {
        self.selected_cid = Some(cid);
        self.busy = false;
        cx.notify();
    }

    pub(super) fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy != busy {
            self.busy = busy;
            cx.notify();
        }
    }

    pub(super) fn set_owner_counts(&mut self, followers: u64, likes: u64, cx: &mut Context<Self>) {
        self.followers = Some(followers);
        self.owner_likes = Some(likes);
        cx.notify();
    }

    pub(super) fn set_follow_status(&mut self, followed: bool, cx: &mut Context<Self>) {
        self.follow.accept(followed);
        cx.notify();
    }

    pub(super) fn set_interactions(&mut self, relation: ArchiveRelation, cx: &mut Context<Self>) {
        self.like.accept(relation.like);
        self.favorite.accept(relation.favorite);
        self.coin.accept(relation.coin);
        cx.notify();
    }

    pub(super) fn set_related(&mut self, related: Vec<RecommendVideo>, cx: &mut Context<Self>) {
        self.related = related
            .into_iter()
            .filter(|video| !video.bvid.is_empty() && video.cid > 0)
            .take(RELATED_LIMIT)
            .collect();
        self.related_selection = clamp_selection(self.related_selection, self.related.len());
        self.reset_related_layout();
        cx.notify();
    }

    pub(super) fn has_related(&self) -> bool {
        !self.related.is_empty()
    }

    pub(super) fn related_focused(&self, window: &Window) -> bool {
        self.tab == InfoTab::Info && self.related_focus.is_focused(window)
    }

    pub(super) fn focus_related(&mut self, cx: &mut Context<Self>) {
        if self.tab != InfoTab::Info || self.related.is_empty() {
            return;
        }
        let index = self.related_selection.unwrap_or(0).min(self.related.len() - 1);
        self.related_selection = Some(index);
        self.reveal_related(index);
        self.related_focus_pending = true;
        cx.notify();
    }

    pub(super) fn step_related(&mut self, delta: isize, cx: &mut Context<Self>) {
        let next = stepped_selection(self.related_selection, delta, self.related.len());
        if next == self.related_selection {
            return;
        }
        self.related_selection = next;
        if let Some(index) = next {
            self.reveal_related(index);
        }
        cx.notify();
    }

    pub(super) fn play_related(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(video) = self
            .related_selection
            .and_then(|index| self.related.get(index))
        else {
            return;
        };
        cx.emit(InfoEvent::PlayRelated {
            bvid: video.bvid.clone(),
            cid: video.cid,
            title: video.title.clone(),
            new_tab,
        });
    }

    fn reset_related_layout(&mut self) {
        let mut layout = self.related_layout.borrow_mut();
        layout.bounds.clear();
        layout.pending = None;
    }

    fn reveal_related(&mut self, index: usize) {
        let mut layout = self.related_layout.borrow_mut();
        let Some(row) = layout.bounds.get(index).copied() else {
            layout.pending = Some(index);
            return;
        };
        let offset = layout.offset;
        drop(layout);

        // The bounds were captured under the offset they were painted with, so
        // shift them by however far the panel has scrolled since.
        let current = self.scroll.offset();
        let shift = current.y - offset;
        let row = Bounds::new(row.origin + point(px(0.), shift), row.size);
        let y = scroll_y_to_show(
            self.scroll.bounds(),
            row,
            current.y,
            self.scroll.max_offset().y,
        );
        if y != current.y {
            self.scroll.set_offset(point(current.x, y));
        }
    }

    pub(super) fn clear_spanning_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selections.clear_spanning_in(window, cx);
    }

    pub(super) fn set_follow_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.follow.set_busy(busy);
        cx.notify();
    }

    pub(super) fn set_followed(&mut self, followed: bool, cx: &mut Context<Self>) {
        if self.follow.active != followed
            && let Some(followers) = self.followers.as_mut()
        {
            if followed {
                *followers = followers.saturating_add(1);
            } else {
                *followers = followers.saturating_sub(1);
            }
        }
        self.follow.apply(followed);
        cx.notify();
    }

    pub(super) fn set_like_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.like.set_busy(busy);
        cx.notify();
    }

    pub(super) fn set_liked(&mut self, liked: bool, cx: &mut Context<Self>) {
        self.like.apply(liked);
        cx.notify();
    }

    pub(super) fn grant_coins(&mut self, count: u32, cx: &mut Context<Self>) {
        self.coin.grant(count);
        cx.notify();
    }

    pub(super) fn set_favorited(&mut self, favorited: bool, cx: &mut Context<Self>) {
        self.favorite.apply(favorited);
        cx.notify();
    }

    pub(super) fn set_watch_later(&mut self, added: bool, cx: &mut Context<Self>) {
        self.watch_later.accept(added);
        cx.notify();
    }

    pub(super) fn set_refresh_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.refresh_busy = false;
        self.refresh_error = Some(error);
        cx.notify();
    }
}

fn stepped_selection(current: Option<usize>, delta: isize, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let Some(current) = current else {
        return Some(0);
    };
    Some((current as isize + delta).clamp(0, last as isize) as usize)
}

fn clamp_selection(selection: Option<usize>, len: usize) -> Option<usize> {
    selection
        .filter(|_| len > 0)
        .map(|index| index.min(len - 1))
}

fn scroll_y_to_show(
    viewport: Bounds<Pixels>,
    row: Bounds<Pixels>,
    current: Pixels,
    max: Pixels,
) -> Pixels {
    // Prepaint bounds already carry the scroll offset, so move the current
    // offset by the visible delta instead of recomputing from layout space.
    let y = if row.top() < viewport.top() {
        current + viewport.top() - row.top()
    } else if row.bottom() > viewport.bottom() {
        current + viewport.bottom() - row.bottom()
    } else {
        current
    };
    y.clamp(-max, px(0.))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::{
        AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
    };
    use rigili_bilibili::{Owner, RecommendVideo, Stat, VideoInfo, VideoPage, VideoRights};

    use super::{
        InfoPanel, InfoSelections, VideoDetails, clamp_selection, scroll_y_to_show,
        stepped_selection,
    };

    #[test]
    fn steps_a_selection_within_bounds() {
        assert_eq!(stepped_selection(None, 1, 0), None);
        assert_eq!(stepped_selection(None, -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(1), 1, 3), Some(2));
        assert_eq!(stepped_selection(Some(1), -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(0), -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(2), 1, 3), Some(2));
    }

    #[test]
    fn clamps_a_selection_to_the_list() {
        assert_eq!(clamp_selection(None, 0), None);
        assert_eq!(clamp_selection(Some(2), 0), None);
        assert_eq!(clamp_selection(Some(2), 3), Some(2));
        assert_eq!(clamp_selection(Some(7), 3), Some(2));
    }

    #[test]
    fn scrolls_only_far_enough_to_reveal_a_related_row() {
        let viewport = Bounds::new(point(px(0.), px(100.)), size(px(400.), px(800.)));

        let below = Bounds::new(point(px(0.), px(2000.)), size(px(400.), px(100.)));
        assert_eq!(
            scroll_y_to_show(viewport, below, px(0.), px(2000.)),
            px(-1200.)
        );

        let clipped = Bounds::new(point(px(0.), px(850.)), size(px(400.), px(100.)));
        assert_eq!(
            scroll_y_to_show(viewport, clipped, px(-1150.), px(2000.)),
            px(-1200.)
        );

        let visible = Bounds::new(point(px(0.), px(800.)), size(px(400.), px(100.)));
        assert_eq!(
            scroll_y_to_show(viewport, visible, px(-1200.), px(2000.)),
            px(-1200.)
        );

        let inside = Bounds::new(point(px(0.), px(200.)), size(px(400.), px(100.)));
        assert_eq!(scroll_y_to_show(viewport, inside, px(0.), px(2000.)), px(0.));

        let above = Bounds::new(point(px(0.), px(-500.)), size(px(400.), px(100.)));
        assert_eq!(scroll_y_to_show(viewport, above, px(-500.), px(2000.)), px(0.));
    }

    #[gpui_kit::test]
    fn keeps_the_selected_related_row_visible(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(rigili_core::image_cache::init);
        let info = VideoInfo {
            bvid: "BV1".into(),
            aid: 1,
            cid: 1,
            pic: String::new(),
            title: "title".into(),
            desc: "description".into(),
            duration: 60,
            pubdate: 0,
            owner: Owner::default(),
            stat: Stat::default(),
            rights: VideoRights::default(),
            pages: vec![VideoPage {
                cid: 1,
                page: 1,
                part: String::new(),
                duration: 60,
            }],
        };
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
                    cx.new(|cx| {
                        let mut panel = InfoPanel::new(selections, cx);
                        panel.details = Some(Arc::new(VideoDetails::new(info)));
                        panel.related = (0..20)
                            .map(|index| RecommendVideo {
                                bvid: format!("BV{index}"),
                                cid: index + 1,
                                title: format!("related {index}"),
                                ..Default::default()
                            })
                            .collect();
                        panel.related_selection = Some(0);
                        panel
                    })
                },
            )
            .expect("open info panel window")
        });

        draw(cx, window);
        for _ in 0..12 {
            panel.update(cx, |panel, cx| panel.step_related(1, cx));
            draw(cx, window);
        }

        panel.read_with(cx, |panel, _| {
            let selection = panel.related_selection.expect("related selection");
            let layout = panel.related_layout.borrow();
            let row = layout
                .bounds
                .get(selection)
                .copied()
                .expect("related row bounds");
            let viewport = panel.scroll.bounds();
            assert!(row.top() >= viewport.top(), "row starts above the panel");
            assert!(row.bottom() <= viewport.bottom(), "row ends below the panel");
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
