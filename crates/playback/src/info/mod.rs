rigili_macro::mod_prv!(comments, details, favorite, header, render);

use std::sync::Arc;

use gpui_kit::component::input::TextareaState;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{ArchiveRelation, Comment, RecommendVideo, VideoInfo, VideoPage};
use rigili_core::{format_count, format_datetime};

const INFO_WIDTH: Pixels = px(340.);
const RELATED_LIMIT: usize = 20;
const RELATED_THUMB_WIDTH: Pixels = px(120.);
const RELATED_THUMB_HEIGHT: Pixels = px(68.);
const COMMENT_AVATAR: Pixels = px(28.);
const NESTED_COMMENT_AVATAR: Pixels = px(20.);
const NESTED_COMMENT_LIMIT: usize = 2;
const COMMENT_MAX_LEN: usize = 1000;

pub(super) enum InfoEvent {
    SelectPage(u64),
    PlayRelated {
        bvid: String,
        cid: u64,
        title: String,
    },
    ToggleFollow {
        mid: u64,
        follow: bool,
    },
    ToggleLike {
        aid: u64,
        like: bool,
    },
    AddCoin {
        aid: u64,
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
    followed: bool,
    followers: Option<u64>,
    owner_likes: Option<u64>,
    follow_busy: bool,
    follow_touched: bool,
    liked: bool,
    like_count: u64,
    like_busy: bool,
    like_touched: bool,
    coins_given: u32,
    coin_count: u64,
    coin_busy: bool,
    coin_touched: bool,
    favorited: bool,
    favorite_count: u64,
    favorite_touched: bool,
    share_count: u64,
    tab: InfoTab,
    comment_count: u64,
    comments: Vec<Comment>,
    comments_scroll: ScrollHandle,
    comments_loading: bool,
    comments_loaded: bool,
    comments_more: bool,
    comments_page: u32,
    comments_total: u64,
    comments_error: Option<String>,
    comments_more_loading: bool,
    comments_more_error: Option<String>,
    draft: Option<Entity<TextareaState>>,
    _draft_sub: Option<Subscription>,
    comment_sending: bool,
    comment_error: Option<String>,
    watch_later: bool,
    watch_later_busy: bool,
    watch_later_touched: bool,
    refresh_busy: bool,
    refresh_error: Option<String>,
}

impl EventEmitter<InfoEvent> for InfoPanel {}

impl InfoPanel {
    pub(super) fn new() -> Self {
        Self {
            details: None,
            selected_cid: None,
            busy: false,
            scroll: ScrollHandle::default(),
            related: Vec::new(),
            description_expanded: false,
            followed: false,
            followers: None,
            owner_likes: None,
            follow_busy: false,
            follow_touched: false,
            liked: false,
            like_count: 0,
            like_busy: false,
            like_touched: false,
            coins_given: 0,
            coin_count: 0,
            coin_busy: false,
            coin_touched: false,
            favorited: false,
            favorite_count: 0,
            favorite_touched: false,
            share_count: 0,
            tab: InfoTab::Info,
            comment_count: 0,
            comments: Vec::new(),
            comments_scroll: ScrollHandle::default(),
            comments_loading: false,
            comments_loaded: false,
            comments_more: false,
            comments_page: 1,
            comments_total: 0,
            comments_error: None,
            comments_more_loading: false,
            comments_more_error: None,
            draft: None,
            _draft_sub: None,
            comment_sending: false,
            comment_error: None,
            watch_later: false,
            watch_later_busy: false,
            watch_later_touched: false,
            refresh_busy: false,
            refresh_error: None,
        }
    }

    pub(super) fn is_loaded(&self) -> bool {
        self.details.is_some()
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        *self = Self::new();
        cx.notify();
    }

    pub(super) fn set(&mut self, info: VideoInfo, cid: u64, cx: &mut Context<Self>) {
        let stat = info.stat.clone();
        self.details = Some(Arc::new(VideoDetails::new(info)));
        self.selected_cid = Some(cid);
        self.busy = false;
        self.related.clear();
        self.description_expanded = false;
        self.followed = false;
        self.followers = None;
        self.owner_likes = None;
        self.follow_busy = false;
        self.follow_touched = false;
        self.liked = false;
        self.like_count = stat.like;
        self.like_busy = false;
        self.like_touched = false;
        self.coins_given = 0;
        self.coin_count = stat.coin;
        self.coin_busy = false;
        self.coin_touched = false;
        self.favorited = false;
        self.favorite_count = stat.favorite;
        self.favorite_touched = false;
        self.share_count = stat.share;
        self.comment_count = stat.reply;
        self.tab = InfoTab::Info;
        self.refresh_busy = false;
        self.refresh_error = None;
        self.reset_comments();
        self.watch_later = false;
        self.watch_later_busy = false;
        self.watch_later_touched = false;
        self.scroll.set_offset(point(px(0.), px(0.)));
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
        if !self.like_touched {
            self.like_count = stat.like;
        }
        if !self.coin_touched {
            self.coin_count = stat.coin;
        }
        if !self.favorite_touched {
            self.favorite_count = stat.favorite;
        }
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
        if !self.follow_touched {
            self.followed = followed;
        }
        cx.notify();
    }

    pub(super) fn set_interactions(&mut self, relation: ArchiveRelation, cx: &mut Context<Self>) {
        if !self.like_touched {
            self.liked = relation.like;
        }
        if !self.favorite_touched {
            self.favorited = relation.favorite;
        }
        if !self.coin_touched {
            self.coins_given = relation.coin.min(2);
        }
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

    pub(super) fn set_follow_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.follow_busy = busy;
        if busy {
            self.follow_touched = true;
        }
        cx.notify();
    }

    pub(super) fn set_followed(&mut self, followed: bool, cx: &mut Context<Self>) {
        if self.followed != followed {
            self.followed = followed;
            if let Some(followers) = self.followers.as_mut() {
                if followed {
                    *followers = followers.saturating_add(1);
                } else {
                    *followers = followers.saturating_sub(1);
                }
            }
        }
        self.follow_busy = false;
        cx.notify();
    }

    pub(super) fn set_like_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.like_busy = busy;
        if busy {
            self.like_touched = true;
        }
        cx.notify();
    }

    pub(super) fn set_liked(&mut self, liked: bool, cx: &mut Context<Self>) {
        if self.liked != liked {
            self.liked = liked;
            if liked {
                self.like_count = self.like_count.saturating_add(1);
            } else {
                self.like_count = self.like_count.saturating_sub(1);
            }
        }
        self.like_busy = false;
        cx.notify();
    }

    pub(super) fn set_coin_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.coin_busy = busy;
        if busy {
            self.coin_touched = true;
        }
        cx.notify();
    }

    pub(super) fn grant_coin(&mut self, cx: &mut Context<Self>) {
        if self.coins_given < 2 {
            self.coins_given += 1;
            self.coin_count = self.coin_count.saturating_add(1);
        }
        self.coin_busy = false;
        cx.notify();
    }

    pub(super) fn set_favorited(&mut self, favorited: bool, cx: &mut Context<Self>) {
        self.favorite_touched = true;
        if self.favorited != favorited {
            self.favorited = favorited;
            if favorited {
                self.favorite_count = self.favorite_count.saturating_add(1);
            } else {
                self.favorite_count = self.favorite_count.saturating_sub(1);
            }
        }
        cx.notify();
    }

    pub(super) fn set_watch_later(&mut self, added: bool, cx: &mut Context<Self>) {
        if !self.watch_later_touched {
            self.watch_later = added;
        }
        cx.notify();
    }

    pub(super) fn set_refresh_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.refresh_busy = false;
        self.refresh_error = Some(error);
        cx.notify();
    }
}
