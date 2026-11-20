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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use gpui_kit::base::{TextSelection, TextSelectionHandle};
use gpui_kit::component::input::TextareaState;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{ArchiveRelation, Comment, RecommendVideo, VideoInfo, VideoPage};
use rigili_core::{format_count, format_datetime};

use comments::{EmoteTarget, ReplyTarget, ReplyThread};
use details::LikePress;
use interaction::{CoinState, FlagState, ToggleState};

pub(crate) use details::short_label;

const INFO_WIDTH: Pixels = px(340.);
const RELATED_LIMIT: usize = 20;
const RELATED_THUMB_WIDTH: Pixels = px(120.);
const RELATED_THUMB_HEIGHT: Pixels = px(68.);
const COMMENT_AVATAR: Pixels = px(28.);
const NESTED_COMMENT_AVATAR: Pixels = px(20.);
const NESTED_COMMENT_LIMIT: usize = 2;
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
    comment_rows: Vec<comments::CommentRow>,
    comments_list: ListState,
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
    comment_sending: bool,
    comment_error: Option<String>,
    reply_threads: HashMap<u64, ReplyThread>,
    reply_target: Option<ReplyTarget>,
    reply_draft: Option<Entity<TextareaState>>,
    _reply_draft_sub: Option<Subscription>,
    reply_sending: bool,
    reply_error: Option<String>,
    emote_target: Option<EmoteTarget>,
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

impl EventEmitter<InfoEvent> for InfoPanel {}

impl InfoPanel {
    pub(super) fn new(selections: InfoSelections) -> Self {
        Self {
            selections,
            details: None,
            selected_cid: None,
            busy: false,
            scroll: ScrollHandle::default(),
            related: Vec::new(),
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
            comment_rows: Vec::new(),
            comments_list: ListState::new(1, ListAlignment::Top, COMMENT_LIST_OVERDRAW),
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
            comment_sending: false,
            comment_error: None,
            reply_threads: HashMap::new(),
            reply_target: None,
            reply_draft: None,
            _reply_draft_sub: None,
            reply_sending: false,
            reply_error: None,
            emote_target: None,
            emote_package: 0,
            watch_later: FlagState::default(),
            refresh_busy: false,
            refresh_error: None,
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        *self = Self::new(self.selections.clone());
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
        cx.notify();
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
