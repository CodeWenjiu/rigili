use gpui_kit::*;
use rigili_bilibili::{
    FavItem, HistoryItem, LiveRoom, PgcItem, PopularVideo, RecommendVideo, SearchLiveRoom,
    SearchMedia, SearchVideo, SpaceVideo, WatchLaterItem, normalize_url,
};
use rigili_core::{OpenVideo, format_count, format_date, format_duration, format_ymd};

#[derive(Clone)]
pub(crate) struct VideoCard {
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
    pub(crate) fn key(&self) -> String {
        format!("video:{}", self.bvid)
    }

    pub(crate) fn id(&self) -> &SharedString {
        &self.id
    }

    pub(crate) fn bvid(&self) -> &str {
        &self.bvid
    }

    pub(crate) fn cover(&self) -> &str {
        &self.cover
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    pub(crate) fn author_mid(&self) -> Option<u64> {
        self.author_mid
    }

    pub(crate) fn meta(&self) -> &str {
        &self.meta
    }

    pub(crate) fn duration(&self) -> &str {
        &self.duration
    }

    pub(crate) fn cid(&self) -> Option<u64> {
        self.cid
    }

    pub(crate) fn views(&self) -> &str {
        &self.views
    }

    pub(crate) fn danmaku(&self) -> &str {
        &self.danmaku
    }

    pub(crate) fn like(&self) -> &str {
        &self.like
    }

    pub(crate) fn open(&self) -> OpenVideo {
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

#[derive(Clone)]
pub(crate) struct LiveCard {
    pub(crate) id: SharedString,
    pub(crate) room_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) online: String,
}

impl LiveCard {
    pub(crate) fn key(&self) -> String {
        format!("live:{}", self.room_id)
    }
}

impl From<LiveRoom> for LiveCard {
    fn from(room: LiveRoom) -> Self {
        let cover = if room.cover.is_empty() {
            room.system_cover
        } else {
            room.cover
        };
        let meta = if room.area_name.is_empty() {
            room.uname
        } else {
            format!("{} · {}", room.uname, room.area_name)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.roomid)),
            room_id: room.roomid,
            cover: normalize_url(&cover),
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

impl From<SearchLiveRoom> for LiveCard {
    fn from(room: SearchLiveRoom) -> Self {
        let meta = if room.area.is_empty() {
            room.name
        } else {
            format!("{} · {}", room.name, room.area)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.room_id)),
            room_id: room.room_id,
            cover: room.cover,
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PgcCard {
    pub(crate) id: SharedString,
    pub(crate) season_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) score: String,
    pub(crate) index_show: String,
    pub(crate) badge: Option<String>,
}

impl PgcCard {
    pub(crate) fn key(&self) -> String {
        format!("pgc:{}", self.season_id)
    }
}

impl From<PgcItem> for PgcCard {
    fn from(item: PgcItem) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: (!item.badge.is_empty()).then_some(item.badge),
        }
    }
}

impl From<SearchMedia> for PgcCard {
    fn from(item: SearchMedia) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: item.badge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LiveCard, VideoCard};
    use rigili_bilibili::{
        FavCounts, FavItem, FavUpper, HistoryItem, HistoryRef, LiveRoom, SearchVideo, SpaceVideo,
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
    fn normalizes_live_room_cards() {
        let card = LiveCard::from(LiveRoom {
            roomid: 7,
            title: "直播".into(),
            uname: "UP主".into(),
            online: 12_345,
            cover: "//i0.hdslb.com/cover.jpg".into(),
            system_cover: String::new(),
            area_name: "游戏".into(),
            face: String::new(),
        });

        assert_eq!(card.id.as_ref(), "live:7");
        assert_eq!(card.cover, "https://i0.hdslb.com/cover.jpg");
        assert_eq!(card.meta, "UP主 · 游戏");
        assert_eq!(card.online, "1.2万");
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
