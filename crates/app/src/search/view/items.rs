use rigili_bilibili::{SearchArticle, SearchLiveRoom, SearchMedia, SearchUser};

use crate::format::{format_count, format_date};
use crate::grid::VideoCard;

pub(crate) enum SearchItem {
    Video(VideoCard),
    Media(MediaItem),
    Live(LiveItem),
    Article(ArticleItem),
    User(UserItem),
}

impl SearchItem {
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Video(video) => format!("video:{}", video.bvid()),
            Self::Media(media) => format!("pgc:{}", media.season_id),
            Self::Live(live) => format!("live:{}", live.room_id),
            Self::Article(article) => format!("article:{}", article.id),
            Self::User(user) => format!("user:{}", user.mid),
        }
    }
}

pub(crate) struct MediaItem {
    pub(crate) season_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) score: String,
    pub(crate) index_show: String,
    pub(crate) badge: Option<String>,
}

pub(crate) struct LiveItem {
    pub(crate) room_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) online: String,
}

pub(crate) struct ArticleItem {
    pub(crate) id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) meta: String,
    pub(crate) views: String,
    pub(crate) likes: String,
    pub(crate) replies: String,
}

pub(crate) struct UserItem {
    pub(crate) mid: u64,
    pub(crate) face: String,
    pub(crate) name: String,
    pub(crate) level: u32,
    pub(crate) fans: String,
    pub(crate) videos: String,
    pub(crate) description: String,
    pub(crate) preview: Vec<VideoCard>,
}

impl From<SearchMedia> for MediaItem {
    fn from(media: SearchMedia) -> Self {
        Self {
            season_id: media.season_id,
            cover: media.cover,
            title: media.title,
            subtitle: media.subtitle,
            score: media.score,
            index_show: media.index_show,
            badge: media.badge,
        }
    }
}

impl From<SearchLiveRoom> for LiveItem {
    fn from(room: SearchLiveRoom) -> Self {
        let meta = if room.area.is_empty() {
            room.name
        } else {
            format!("{} · {}", room.name, room.area)
        };

        Self {
            room_id: room.room_id,
            cover: room.cover,
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

impl From<SearchArticle> for ArticleItem {
    fn from(article: SearchArticle) -> Self {
        let meta = if article.author.is_empty() {
            format_date(article.pubdate)
        } else {
            format!("{} · {}", article.author, format_date(article.pubdate))
        };

        Self {
            id: article.id,
            cover: article.cover,
            title: article.title,
            summary: article.summary,
            meta,
            views: format_count(article.views),
            likes: format_count(article.likes),
            replies: format_count(article.replies),
        }
    }
}

impl From<SearchUser> for UserItem {
    fn from(user: SearchUser) -> Self {
        Self {
            mid: user.mid,
            face: user.face,
            name: user.name,
            level: user.level,
            fans: format_count(user.fans),
            videos: user.video_count.to_string(),
            description: user
                .official
                .or_else(|| (!user.sign.is_empty()).then_some(user.sign))
                .unwrap_or_default(),
            preview: user.preview.into_iter().map(VideoCard::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArticleItem, LiveItem, MediaItem, SearchItem, UserItem};
    use crate::grid::VideoCard;
    use rigili_bilibili::{SearchArticle, SearchLiveRoom, SearchMedia, SearchUser, SearchVideo};

    fn video(bvid: &str) -> VideoCard {
        VideoCard::from(SearchVideo {
            bvid: bvid.into(),
            title: String::new(),
            pic: String::new(),
            author: String::new(),
            duration: 0,
            pubdate: 0,
            play: 0,
            danmaku: 0,
            like: 0,
        })
    }

    #[test]
    fn maps_search_results_to_items() {
        let media = MediaItem::from(SearchMedia {
            season_id: 7,
            title: "番剧".into(),
            cover: "https://example.invalid/cover.jpg".into(),
            index_show: "全12话".into(),
            subtitle: "动画 · 奇幻".into(),
            score: "9.6".into(),
            badge: Some("独家".into()),
        });
        assert_eq!(SearchItem::Media(media).key(), "pgc:7");

        let live = LiveItem::from(SearchLiveRoom {
            room_id: 42,
            uid: 1,
            name: "主播".into(),
            face: String::new(),
            cover: String::new(),
            title: "直播".into(),
            area: "游戏".into(),
            online: 12_345,
        });
        assert_eq!(live.meta, "主播 · 游戏");
        assert_eq!(live.online, "1.2万");
        assert_eq!(SearchItem::Live(live).key(), "live:42");

        let article = ArticleItem::from(SearchArticle {
            id: 9,
            title: "专栏".into(),
            summary: "摘要".into(),
            cover: String::new(),
            author: "作者".into(),
            views: 1_000,
            likes: 20,
            replies: 3,
            pubdate: 1_762_646_400,
        });
        assert_eq!(article.meta, "作者 · 11-9");
        assert_eq!(article.views, "1000");
        assert_eq!(SearchItem::Article(article).key(), "article:9");

        let user = UserItem::from(SearchUser {
            mid: 5,
            name: "用户".into(),
            face: String::new(),
            fans: 25_000,
            video_count: 32,
            level: 4,
            sign: "签名".into(),
            official: Some("认证".into()),
            preview: vec![SearchVideo {
                bvid: "BV1".into(),
                title: String::new(),
                pic: String::new(),
                author: String::new(),
                duration: 0,
                pubdate: 0,
                play: 0,
                danmaku: 0,
                like: 0,
            }],
        });
        assert_eq!(user.fans, "2.5万");
        assert_eq!(user.description, "认证");
        assert_eq!(user.preview.len(), 1);
        assert_eq!(SearchItem::User(user).key(), "user:5");

        assert_eq!(SearchItem::Video(video("BV1")).key(), "video:BV1");
    }
}
