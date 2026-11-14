use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::CommentSort;
use rigili_bilibili::{
    ArchiveRelation, Comment, CommentPage, RecommendVideo, VideoInfo, VideoPage, thumbnail,
};

use rigili_core::backend;
use rigili_core::runtime::spawn_api;
use rigili_core::{
    api_error_message, api_task_error_message, format_count, format_date, format_datetime,
    format_duration, message_markdown,
};

use super::favorite::FavoriteDialog;

const INFO_WIDTH: Pixels = px(340.);
const RELATED_LIMIT: usize = 20;
const RELATED_THUMB_WIDTH: Pixels = px(120.);
const RELATED_THUMB_HEIGHT: Pixels = px(68.);
const COMMENT_AVATAR: Pixels = px(28.);
const NESTED_COMMENT_AVATAR: Pixels = px(20.);
const NESTED_COMMENT_LIMIT: usize = 2;

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

    fn toggle_description(&mut self, cx: &mut Context<Self>) {
        self.description_expanded = !self.description_expanded;
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

    fn reset_comments(&mut self) {
        self.comments.clear();
        self.comments_loading = false;
        self.comments_loaded = false;
        self.comments_more = false;
        self.comments_page = 1;
        self.comments_total = 0;
        self.comments_error = None;
        self.comments_more_loading = false;
        self.comments_more_error = None;
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
    }

    fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let tab = if index == 1 {
            InfoTab::Comments
        } else {
            InfoTab::Info
        };
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        cx.notify();
        if tab == InfoTab::Comments && !self.comments_loaded && !self.comments_loading {
            self.load_comments(cx);
        }
    }

    pub(super) fn show_tab(&mut self, tab: InfoTab, cx: &mut Context<Self>) {
        let index = match tab {
            InfoTab::Info => 0,
            InfoTab::Comments => 1,
        };
        self.select_tab(index, cx);
    }

    pub(super) fn tab(&self) -> InfoTab {
        self.tab
    }

    pub(super) fn reload_current(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            InfoTab::Info => {
                if self.refresh_busy {
                    return;
                }
                self.refresh_busy = true;
                self.refresh_error = None;
                cx.notify();
                cx.emit(InfoEvent::Reload);
            }
            InfoTab::Comments => self.load_comments(cx),
        }
    }

    fn toggle_watch_later(&mut self, cx: &mut Context<Self>) {
        if self.watch_later_busy {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let Ok(backend) = backend::get(cx) else {
            return;
        };
        let aid = details.aid;
        let bvid = details.bvid.to_string();
        let add = !self.watch_later;
        self.watch_later_busy = true;
        self.watch_later_touched = true;
        cx.notify();

        spawn_api(
            cx,
            async move {
                if add {
                    backend.watch_later_add(&bvid).await
                } else {
                    backend.watch_later_remove(aid).await
                }
            },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.watch_later_busy = false;
                if result.is_ok() {
                    this.watch_later = add;
                }
                cx.notify();
            },
        );
    }

    fn load_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments_loading {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
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
        self.comments_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();

        spawn_api(
            cx,
            async move {
                backend
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
        let Ok(backend) = backend::get(cx) else {
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
                backend
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

    fn render_pages(&self, details: &VideoDetails, cx: &mut Context<Self>) -> AnyElement {
        let selected = details
            .pages
            .iter()
            .position(|page| Some(page.cid) == self.selected_cid);
        let count = match selected {
            Some(index) => format!("{}/{}", index + 1, details.pages.len()),
            None => format!("共 {} P", details.pages.len()),
        };
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;
        let weak = cx.entity().downgrade();

        let rows = details.pages.iter().map(|page| {
            let cid = page.cid;
            let current = Some(cid) == self.selected_cid;
            let label = short_label(page);
            let duration = format_duration(page.duration);
            let panel = weak.clone();

            ListItem::new(("page", cid))
                .text_sm()
                .px_2()
                .py_1()
                .rounded(radius)
                .selected(current)
                .disabled(self.busy)
                .accessibility_label(label.clone())
                .on_click(move |_, _, cx| {
                    let _ = panel.update(cx, |_, cx| cx.emit(InfoEvent::SelectPage(cid)));
                })
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(label))
                        .child(div().text_xs().text_color(muted).child(duration)),
                )
        });

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("选集"),
                    )
                    .child(div().text_xs().text_color(muted).child(count)),
            )
            .child(v_flex().gap_1().children(rows))
            .into_any_element()
    }
}

impl InfoPanel {
    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let count: SharedString = format_count(self.comment_count).into();
        let watch_later = self.watch_later;
        let later_label: SharedString = if watch_later {
            "已加入稍后再看"
        } else {
            "稍后再看"
        }
        .into();
        let later_icon = if watch_later {
            Icon::new(IconName::Clock).text_color(primary)
        } else {
            Icon::new(IconName::Clock)
        };

        let reloading =
            self.refresh_busy || (self.tab == InfoTab::Comments && self.comments_loading);
        let watch_later_button = Button::new("player-watch-later")
            .ghost()
            .accessibility_label(later_label.clone())
            .tooltip(later_label)
            .icon(later_icon)
            .loading(self.watch_later_busy)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_watch_later(cx)));
        let reload_button = Button::new("player-reload")
            .ghost()
            .accessibility_label("重新加载")
            .tooltip("重新加载")
            .icon(IconName::RefreshCw)
            .loading(reloading)
            .on_click(cx.listener(|this, _, _, cx| this.reload(cx)));

        v_flex()
            .flex_shrink_0()
            .child(
                TabBar::new("player-info-tabs")
                    .underline()
                    .small()
                    .px_4()
                    .selected_index(if self.tab == InfoTab::Comments { 1 } else { 0 })
                    .child(Tab::new().label("简介"))
                    .child({
                        let comment_tab = Tab::new().label("评论");
                        if self.comment_count > 0 {
                            comment_tab.suffix(div().text_xs().text_color(muted).child(count))
                        } else {
                            comment_tab
                        }
                    })
                    .on_click(cx.listener(|this, index, _, cx| this.select_tab(*index, cx)))
                    .suffix(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(watch_later_button)
                            .child(reload_button),
                    ),
            )
            .into_any_element()
    }

    fn render_info_tab(&self, details: &VideoDetails, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let link = theme.link;
        let link_hover = theme.link_hover;
        let danger = theme.danger;

        let weak = cx.entity().downgrade();
        let aid = details.aid;
        let owner_mid = details.owner_mid;
        let followed = self.followed;
        let liked = self.liked;
        let like_busy = self.like_busy;
        let coins_given = self.coins_given;
        let coin_busy = self.coin_busy;
        let favorited = self.favorited;
        let share_url = format!("https://www.bilibili.com/video/{}", details.bvid);

        let owner_meta: SharedString = match (self.followers, self.owner_likes) {
            (Some(followers), Some(likes)) => {
                let followers = format_count(followers);
                let likes = format_count(likes);
                format!("{followers}粉丝 · {likes}点赞").into()
            }
            (Some(followers), None) => format!("{}粉丝", format_count(followers)).into(),
            _ => format!("UID {owner_mid}").into(),
        };

        let follow_button = if followed {
            Button::new("follow").ghost()
        } else {
            Button::new("follow").primary()
        }
        .small()
        .icon(if followed {
            IconName::Check
        } else {
            IconName::Plus
        })
        .label(if followed { "已关注" } else { "关注" })
        .disabled(self.follow_busy)
        .on_click({
            let weak = weak.clone();
            move |_, _, cx| {
                let _ = weak.update(cx, |_, cx| {
                    cx.emit(InfoEvent::ToggleFollow {
                        mid: owner_mid,
                        follow: !followed,
                    });
                });
            }
        });

        let has_description = !details.description.is_empty();
        let expanded = (has_description && self.description_expanded).then(|| {
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(details.bvid.clone())
                        .when(details.no_reprint, |this| {
                            this.child(Icon::new(IconName::Ban).size(px(12.)).text_color(danger))
                                .child("未经作者授权，禁止转载")
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(details.description.clone()),
                )
        });

        let pages = (details.pages.len() > 1).then(|| self.render_pages(details, cx));

        let related = (!self.related.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .children(self.related.iter().enumerate().map(|(index, video)| {
                    let bvid = video.bvid.clone();
                    let cid = video.cid;
                    let title = video.title.clone();
                    let weak = weak.clone();
                    related_row(index, video, cx, move |_, _, cx| {
                        let _ = weak.update(cx, |_, cx| {
                            cx.emit(InfoEvent::PlayRelated {
                                bvid: bvid.clone(),
                                cid,
                                title: title.clone(),
                            });
                        });
                    })
                }))
        });

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("player-info-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        v_flex()
                            .gap_4()
                            .p_4()
                            .pb_6()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Avatar::new()
                                            .src(details.avatar.clone())
                                            .name(details.owner.clone())
                                            .size(px(40.)),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .truncate()
                                                    .child(details.owner.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .truncate()
                                                    .child(owner_meta),
                                            ),
                                    )
                                    .child(follow_button),
                            )
                            .child(
                                h_flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_base()
                                            .font_weight(FontWeight::MEDIUM)
                                            .line_clamp(2)
                                            .child(details.title.clone()),
                                    )
                                    .when(has_description, |this| {
                                        this.child(
                                            h_flex()
                                                .id("toggle-description")
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(link)
                                                .hover(move |this| this.text_color(link_hover))
                                                .child(if self.description_expanded {
                                                    "收起"
                                                } else {
                                                    "展开"
                                                })
                                                .child(
                                                    Icon::new(if self.description_expanded {
                                                        IconName::ChevronUp
                                                    } else {
                                                        IconName::ChevronDown
                                                    })
                                                    .size(px(12.)),
                                                )
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak.update(cx, |this, cx| {
                                                            this.toggle_description(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_4()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(stat(IconName::Play, details.views.clone()))
                                    .child(stat(IconName::MessageCircle, details.danmaku.clone()))
                                    .child(stat(IconName::Calendar, details.pubdate.clone())),
                            )
                            .when_some(expanded, |this, expanded| this.child(expanded))
                            .when_some(self.refresh_error.clone(), |this, error| {
                                this.child(div().text_xs().text_color(danger).child(error))
                            })
                            .child(
                                h_flex()
                                    .w_full()
                                    .items_stretch()
                                    .child(
                                        action_button(
                                            ("like", aid),
                                            IconName::ThumbsUp,
                                            format_count(self.like_count).into(),
                                            liked,
                                            self.like_busy,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                if like_busy {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |_, cx| {
                                                    cx.emit(InfoEvent::ToggleLike {
                                                        aid,
                                                        like: !liked,
                                                    });
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("coin", aid),
                                            IconName::Coins,
                                            format_count(self.coin_count).into(),
                                            coins_given > 0,
                                            self.coin_busy,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                if coins_given >= 2 || coin_busy {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |_, cx| {
                                                    cx.emit(InfoEvent::AddCoin { aid });
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("favorite", aid),
                                            IconName::Star,
                                            format_count(self.favorite_count).into(),
                                            favorited,
                                            false,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    let Some(details) = this.details.clone() else {
                                                        return;
                                                    };
                                                    let panel = cx.entity();
                                                    let content = cx.new(|cx| {
                                                        FavoriteDialog::new(
                                                            window,
                                                            cx,
                                                            details.aid,
                                                            panel,
                                                        )
                                                    });
                                                    window.open_dialog(cx, move |dialog, _, _| {
                                                        dialog
                                                            .w(px(360.))
                                                            .title("选择收藏夹")
                                                            .child(content.clone())
                                                    });
                                                });
                                            }
                                        }),
                                    )
                                    .child(action_button(
                                        ("cache", aid),
                                        IconName::Download,
                                        "缓存".into(),
                                        false,
                                        true,
                                        cx,
                                    ))
                                    .child(
                                        action_button(
                                            ("share", aid),
                                            IconName::Share2,
                                            format_count(self.share_count).into(),
                                            false,
                                            false,
                                            cx,
                                        )
                                        .on_click({
                                            let url = share_url.clone();
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    url.clone(),
                                                ));
                                            }
                                        }),
                                    ),
                            )
                            .child(div().h(px(1.)).w_full().bg(border))
                            .when_some(pages, |this, pages| this.child(pages))
                            .when_some(related, |this, related| this.child(related)),
                    ),
            )
            .child(Scrollbar::vertical(&self.scroll))
            .into_any_element()
    }

    fn render_comments_tab(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .child(Scrollbar::vertical(&self.comments_scroll))
            .into_any_element()
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
                    .child(self.render_comment(comment, 0, cx))
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
        let nested = comment
            .replies()
            .iter()
            .take(NESTED_COMMENT_LIMIT)
            .map(|reply| self.render_comment(reply, depth + 1, cx))
            .collect::<Vec<_>>();

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(
                        Avatar::new()
                            .src(member.avatar_url())
                            .name(member.uname.clone())
                            .size(avatar),
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
                                            .child(Icon::new(IconName::ThumbsUp).with_size(px(14.)))
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
            .when(!nested.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(36.)).children(nested))
            })
            .into_any_element()
    }
}

impl Render for InfoPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let Some(details) = self.details.clone() else {
            return div().into_any_element();
        };

        let content = match self.tab {
            InfoTab::Info => self.render_info_tab(&details, cx),
            InfoTab::Comments => self.render_comments_tab(cx),
        };

        v_flex()
            .w(INFO_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(cx))
            .child(content)
            .into_any_element()
    }
}

fn stat(icon: IconName, value: SharedString) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).size(px(14.)))
        .child(value)
}

fn action_button(
    id: (&'static str, u64),
    icon: IconName,
    label: SharedString,
    active: bool,
    dimmed: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let color = if active {
        theme.primary
    } else {
        theme.muted_foreground
    };
    v_flex()
        .id(id)
        .flex_1()
        .items_center()
        .gap_1()
        .py_2()
        .rounded(theme.radius)
        .cursor_pointer()
        .opacity(if dimmed { 0.45 } else { 1. })
        .when(!dimmed, |this| {
            this.hover(move |this| this.bg(theme.accent))
        })
        .child(Icon::new(icon).size(px(22.)).text_color(color))
        .child(div().text_xs().text_color(color).child(label))
}

fn related_row(
    index: usize,
    video: &RecommendVideo,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let radius = theme.radius;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let cover = thumbnail(&video.pic, 240);
    let title: SharedString = video.title.clone().into();
    let owner: SharedString = video.owner.name.clone().into();
    let views: SharedString = format_count(video.stat.view).into();
    let danmaku: SharedString = format_count(video.stat.danmaku).into();
    let duration = format_duration(video.duration);

    h_flex()
        .id(("related", index))
        .w_full()
        .items_start()
        .gap_2()
        .p_1()
        .rounded(radius)
        .cursor_pointer()
        .hover(move |this| this.bg(accent))
        .on_click(on_click)
        .child(
            div()
                .relative()
                .w(RELATED_THUMB_WIDTH)
                .h(RELATED_THUMB_HEIGHT)
                .flex_shrink_0()
                .rounded(radius)
                .overflow_hidden()
                .bg(muted.opacity(0.12))
                .child(img(cover).size_full().object_fit(ObjectFit::Cover))
                .child(
                    div()
                        .absolute()
                        .right_1()
                        .bottom_1()
                        .px_1()
                        .rounded(px(2.))
                        .bg(hsla(0., 0., 0., 0.65))
                        .text_size(px(10.))
                        .text_color(Hsla::white())
                        .child(duration),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(div().text_sm().line_clamp(2).child(title))
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            div()
                                .px(px(3.))
                                .rounded(px(2.))
                                .border_1()
                                .border_color(muted.opacity(0.4))
                                .text_size(px(9.))
                                .child("UP"),
                        )
                        .child(div().min_w_0().truncate().child(owner)),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(stat(IconName::Play, views))
                        .child(stat(IconName::MessageCircle, danmaku)),
                ),
        )
}

fn short_label(page: &VideoPage) -> String {
    if page.part.is_empty() {
        format!("P{}", page.page)
    } else {
        format!("P{} · {}", page.page, page.part)
    }
}

#[cfg(test)]
mod tests {
    use super::short_label;
    use rigili_bilibili::VideoPage;

    fn page(number: u32, part: &str) -> VideoPage {
        VideoPage {
            cid: 42,
            page: number,
            part: part.into(),
            duration: 0,
        }
    }

    #[test]
    fn labels_pages() {
        assert_eq!(short_label(&page(2, "第二局")), "P2 · 第二局");
        assert_eq!(short_label(&page(1, "")), "P1");
    }
}
