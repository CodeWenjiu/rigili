use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{
    FavItem, HistoryItem, PopularVideo, RecommendVideo, SearchVideo, SpaceVideo, WatchLaterItem,
};

use crate::{OpenVideo, format_count, format_date, format_duration, format_ymd};

#[derive(Clone)]
pub struct VideoCard {
    id: SharedString,
    bvid: String,
    cid: Option<u64>,
    cover: String,
    title: String,
    author: String,
    author_mid: Option<u64>,
    meta: String,
    duration: String,
    views: String,
    danmaku: String,
    like: String,
}

impl VideoCard {
    pub fn key(&self) -> String {
        format!("video:{}", self.bvid)
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    pub fn bvid(&self) -> &str {
        &self.bvid
    }

    pub fn cover(&self) -> &str {
        &self.cover
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn author_mid(&self) -> Option<u64> {
        self.author_mid
    }

    pub fn meta(&self) -> &str {
        &self.meta
    }

    pub fn duration(&self) -> &str {
        &self.duration
    }

    pub fn cid(&self) -> Option<u64> {
        self.cid
    }

    pub fn views(&self) -> &str {
        &self.views
    }

    pub fn danmaku(&self) -> &str {
        &self.danmaku
    }

    pub fn like(&self) -> &str {
        &self.like
    }

    pub fn open(&self) -> OpenVideo {
        OpenVideo {
            bvid: self.bvid.clone(),
            cid: self.cid,
            title: self.title.clone(),
        }
    }
}

impl From<RecommendVideo> for VideoCard {
    fn from(video: RecommendVideo) -> Self {
        let RecommendVideo {
            bvid,
            cid,
            pic,
            title,
            duration,
            pubdate,
            owner,
            stat,
        } = video;
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
            bvid,
            cid: (cid != 0).then_some(cid),
            cover: pic,
            title,
            author: owner.name.clone(),
            author_mid: (owner.mid != 0).then_some(owner.mid),
            meta: format!("{} · {}", owner.name, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(stat.view),
            danmaku: format_count(stat.danmaku),
            like: format_count(stat.like),
        }
    }
}

impl From<PopularVideo> for VideoCard {
    fn from(video: PopularVideo) -> Self {
        let PopularVideo {
            bvid,
            cid,
            pic,
            title,
            duration,
            pubdate,
            owner,
            stat,
            ..
        } = video;
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
            bvid,
            cid: (cid != 0).then_some(cid),
            cover: pic,
            title,
            author: owner.name.clone(),
            author_mid: (owner.mid != 0).then_some(owner.mid),
            meta: format!("{} · {}", owner.name, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(stat.view),
            danmaku: format_count(stat.danmaku),
            like: format_count(stat.like),
        }
    }
}

impl From<SearchVideo> for VideoCard {
    fn from(video: SearchVideo) -> Self {
        let SearchVideo {
            bvid,
            title,
            pic,
            author,
            mid,
            duration,
            pubdate,
            play,
            danmaku,
            like,
        } = video;
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
            bvid,
            cid: None,
            cover: pic,
            title,
            author: author.clone(),
            author_mid: (mid != 0).then_some(mid),
            meta: format!("{} · {}", author, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(play),
            danmaku: format_count(danmaku),
            like: format_count(like),
        }
    }
}

impl From<SpaceVideo> for VideoCard {
    fn from(video: SpaceVideo) -> Self {
        let id = SharedString::from(video.bvid.as_str());

        Self {
            id,
            bvid: video.bvid,
            cid: None,
            cover: video.pic,
            title: video.title,
            author: video.author.clone(),
            author_mid: (video.mid != 0).then_some(video.mid),
            meta: format!("{} · {}", video.author, format_ymd(video.created)),
            duration: video.length,
            views: format_count(video.play),
            danmaku: format_count(video.video_review),
            like: String::new(),
        }
    }
}

impl From<FavItem> for VideoCard {
    fn from(item: FavItem) -> Self {
        let id = SharedString::from(item.bvid.as_str());

        Self {
            id,
            bvid: item.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.upper.name.clone(),
            author_mid: (item.upper.mid != 0).then_some(item.upper.mid),
            meta: format!("{} · {}", item.upper.name, format_date(item.pubtime)),
            duration: format_duration(item.duration),
            views: format_count(item.cnt_info.play),
            danmaku: format_count(item.cnt_info.danmaku),
            like: String::new(),
        }
    }
}

impl From<HistoryItem> for VideoCard {
    fn from(item: HistoryItem) -> Self {
        let id = SharedString::from(item.history.bvid.as_str());

        Self {
            id,
            bvid: item.history.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.author_name.clone(),
            author_mid: (item.author_mid != 0).then_some(item.author_mid),
            meta: format!("{} · {}", item.author_name, format_date(item.view_at)),
            duration: format_duration(item.duration),
            views: String::new(),
            danmaku: String::new(),
            like: String::new(),
        }
    }
}

impl From<WatchLaterItem> for VideoCard {
    fn from(item: WatchLaterItem) -> Self {
        let id = SharedString::from(item.bvid.as_str());

        Self {
            id,
            bvid: item.bvid,
            cid: (item.cid != 0).then_some(item.cid),
            cover: item.pic,
            title: item.title,
            author: item.owner.name.clone(),
            author_mid: (item.owner.mid != 0).then_some(item.owner.mid),
            meta: format!("{} · {}", item.owner.name, format_date(item.pubdate)),
            duration: format_duration(item.duration),
            views: format_count(item.stat.view),
            danmaku: format_count(item.stat.danmaku),
            like: format_count(item.stat.like),
        }
    }
}

pub fn video_title(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    lines: usize,
) -> Stateful<Div> {
    let text = text.into();
    let tooltip = text.clone();

    div()
        .id(id)
        .w_full()
        .min_w_0()
        .text_sm()
        .line_clamp(lines)
        .text_ellipsis()
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(text)
}

pub fn video_meta(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    icon: Option<IconName>,
) -> Stateful<Div> {
    let text = text.into();
    let tooltip = text.clone();

    h_flex()
        .id(id)
        .items_center()
        .gap_1()
        .min_w_0()
        .overflow_hidden()
        .text_xs()
        .when(!tooltip.is_empty(), move |this| {
            this.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        })
        .when_some(icon, |this, icon| {
            this.child(Icon::new(icon).with_size(px(12.)))
        })
        .child(div().min_w_0().truncate().child(text))
}

#[cfg(test)]
mod tests {
    use super::VideoCard;
    use rigili_bilibili::{
        FavCounts, FavItem, FavUpper, HistoryItem, HistoryRef, Owner, RecommendVideo, SearchVideo,
        SpaceVideo, Stat,
    };

    #[test]
    fn keeps_author_mids_for_profile_links() {
        let card = VideoCard::from(FavItem {
            bvid: "BV1".into(),
            title: String::new(),
            cover: String::new(),
            duration: 0,
            pubtime: 0,
            upper: FavUpper {
                mid: 42,
                name: "UP".into(),
            },
            cnt_info: FavCounts::default(),
        });
        assert_eq!(card.author_mid(), Some(42));

        let card = VideoCard::from(HistoryItem {
            title: String::new(),
            cover: String::new(),
            author_name: "UP".into(),
            author_mid: 43,
            duration: 0,
            view_at: 0,
            history: HistoryRef::default(),
        });
        assert_eq!(card.author_mid(), Some(43));

        let card = VideoCard::from(SpaceVideo {
            bvid: "BV3".into(),
            author: "UP".into(),
            mid: 44,
            ..Default::default()
        });
        assert_eq!(card.author_mid(), Some(44));
    }

    #[test]
    fn formats_recommend_cards() {
        let card = VideoCard::from(RecommendVideo {
            bvid: "BV1".into(),
            cid: 42,
            pic: "https://example.invalid/cover.jpg".into(),
            title: "标题".into(),
            duration: 166,
            pubdate: 1_762_646_400,
            owner: Owner {
                mid: 7,
                name: "UP".into(),
                face: String::new(),
            },
            stat: Stat {
                view: 12_345,
                danmaku: 678,
                like: 90,
                ..Default::default()
            },
        });

        assert_eq!(card.title(), "标题");
        assert_eq!(card.cid(), Some(42));
        assert_eq!(card.author_mid(), Some(7));
        assert_eq!(card.duration(), "02:46");
        assert_eq!(card.views(), "1.2万");
        assert_eq!(card.danmaku(), "678");
    }

    #[test]
    fn formats_search_cards() {
        let card = VideoCard::from(SearchVideo {
            bvid: "BV1".into(),
            title: "标题".into(),
            pic: "https://example.invalid/cover.jpg".into(),
            author: "UP主".into(),
            mid: 0,
            duration: 156,
            pubdate: 1_762_646_400,
            play: 12_345,
            danmaku: 678,
            like: 90,
        });

        assert_eq!(card.meta, "UP主 · 11-9");
        assert_eq!(card.duration, "02:36");
        assert_eq!(card.views, "1.2万");
        assert_eq!(card.danmaku, "678");
        assert_eq!(card.like, "90");
    }
}
