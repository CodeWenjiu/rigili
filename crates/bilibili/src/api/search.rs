use serde::de::DeserializeOwned;

use crate::{
    ApiError, ArticleSearch, Client, HotWords, LiveSearch, MediaSearch, SearchAll,
    SearchSuggestions, UserSearch, VideoSearch,
};

const SEARCH_URL: &str = "https://api.bilibili.com/x/web-interface/wbi/search/type";
const ALL_URL: &str = "https://api.bilibili.com/x/web-interface/wbi/search/all/v2";
const SUGGEST_URL: &str = "https://s.search.bilibili.com/main/suggest";
const HOTWORD_URL: &str = "https://s.search.bilibili.com/main/hotword";
const PAGE_SIZE: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchType {
    Video,
    Bangumi,
    Film,
    Live,
    Article,
    User,
}

impl SearchType {
    pub const ALL: [Self; 6] = [
        Self::Video,
        Self::Bangumi,
        Self::Film,
        Self::Live,
        Self::Article,
        Self::User,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Bangumi => "media_bangumi",
            Self::Film => "media_ft",
            Self::Live => "live",
            Self::Article => "article",
            Self::User => "bili_user",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchOrder {
    Default,
    Clicks,
    Newest,
    Danmaku,
    Favorites,
}

impl SearchOrder {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Clicks,
        Self::Newest,
        Self::Danmaku,
        Self::Favorites,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "totalrank",
            Self::Clicks => "click",
            Self::Newest => "pubdate",
            Self::Danmaku => "dm",
            Self::Favorites => "stow",
        }
    }
}

pub async fn fetch_videos(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
) -> Result<VideoSearch, ApiError> {
    fetch_type(client, keyword, page, order, SearchType::Video).await
}

pub async fn fetch_users(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
) -> Result<UserSearch, ApiError> {
    fetch_type(client, keyword, page, order, SearchType::User).await
}

pub async fn fetch_live(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
) -> Result<LiveSearch, ApiError> {
    fetch_type(client, keyword, page, order, SearchType::Live).await
}

pub async fn fetch_articles(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
) -> Result<ArticleSearch, ApiError> {
    fetch_type(client, keyword, page, order, SearchType::Article).await
}

pub async fn fetch_media(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
    search_type: SearchType,
) -> Result<MediaSearch, ApiError> {
    if !matches!(search_type, SearchType::Bangumi | SearchType::Film) {
        return Err(ApiError::InvalidResponse("无效的影视搜索类型".into()));
    }

    fetch_type(client, keyword, page, order, search_type).await
}

pub async fn fetch_all(client: &Client, keyword: &str) -> Result<SearchAll, ApiError> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err(ApiError::InvalidResponse("搜索关键词不能为空".into()));
    }

    client
        .get_wbi_json(ALL_URL, &[("keyword", keyword.to_owned())])
        .await
}

pub async fn fetch_suggestions(
    client: &Client,
    term: &str,
) -> Result<SearchSuggestions, ApiError> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(SearchSuggestions::default());
    }

    client
        .get_json(
            SUGGEST_URL,
            &[
                ("term", term.to_owned()),
                ("main_ver", "v1".into()),
                ("highlight", String::new()),
            ],
        )
        .await
}

pub async fn fetch_hot_words(client: &Client) -> Result<HotWords, ApiError> {
    client.get_raw(HOTWORD_URL).await
}

async fn fetch_type<T>(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
    search_type: SearchType,
) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err(ApiError::InvalidResponse("搜索关键词不能为空".into()));
    }

    client
        .get_wbi_json(
            SEARCH_URL,
            &[
                ("search_type", search_type.as_str().to_owned()),
                ("keyword", keyword.to_owned()),
                ("page", page.max(1).to_string()),
                ("page_size", PAGE_SIZE.to_string()),
                ("order", order.as_str().to_owned()),
            ],
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::{SearchOrder, SearchType};
    use crate::{ArticleSearch, LiveSearch, MediaSearch, SearchAll, UserSearch, VideoSearch};

    #[derive(serde::Deserialize)]
    struct Envelope<T> {
        data: T,
    }

    fn parse<T: serde::de::DeserializeOwned>(fixture: &str) -> T {
        serde_json::from_str::<Envelope<T>>(fixture)
            .expect("search fixture")
            .data
    }

    #[test]
    fn parses_search_fixtures() {
        let videos: VideoSearch = parse(include_str!("../../fixtures/search.json"));
        assert_eq!(videos.page, 1);
        assert_eq!(videos.num_pages, 62);
        assert_eq!(videos.num_results, 1_234);
        assert_eq!(videos.videos.len(), 2);
        assert_eq!(videos.videos[0].title, "【测试】哔哩哔哩");
        assert_eq!(videos.videos[0].like, 100);
        assert_eq!(videos.videos[1].duration, 3_723);

        let users: UserSearch = parse(include_str!("../../fixtures/search_users.json"));
        assert_eq!(users.num_pages, 50);
        assert_eq!(users.num_results, 1_000);
        assert_eq!(users.users.len(), 1);
        let user = &users.users[0];
        assert_eq!(user.mid, 1_561_367_430);
        assert_eq!(user.name, "安东森");
        assert_eq!(user.fans, 24_841);
        assert_eq!(user.video_count, 32);
        assert_eq!(user.level, 4);
        assert_eq!(user.official.as_deref(), Some("丹麦羽毛球运动员安东森"));
        assert_eq!(user.preview.len(), 2);
        assert_eq!(user.preview[0].play, 19_767);
        assert_eq!(user.preview[0].title, "2024 成都汤尤杯");
        assert_eq!(user.preview[1].pic, "https://i1.hdslb.com/a.jpg");

        let media: MediaSearch = parse(include_str!("../../fixtures/search_media.json"));
        assert_eq!(media.num_results, 1);
        assert_eq!(media.items.len(), 1);
        let item = &media.items[0];
        assert_eq!(item.season_id, 47_836);
        assert_eq!(item.title, "鬼灭之刃");
        assert_eq!(item.score, "9.7");
        assert_eq!(item.subtitle, "动画 · 奇幻");
        assert_eq!(item.index_show, "全8话");
        assert_eq!(item.badge.as_deref(), Some("独家"));
        assert_eq!(item.cover, "https://i0.hdslb.com/cover.jpg");

        let articles: ArticleSearch = parse(include_str!("../../fixtures/search_articles.json"));
        assert_eq!(articles.articles.len(), 1);
        let article = &articles.articles[0];
        assert_eq!(article.id, 1_234_567);
        assert_eq!(article.title, "专栏标题");
        assert_eq!(article.cover, "https://i0.hdslb.com/cover.jpg");
        assert_eq!(article.views, 12_345);
        assert_eq!(article.likes, 678);
        assert_eq!(article.replies, 90);
        assert_eq!(article.summary, "专栏摘要");

        let live: LiveSearch = parse(include_str!("../../fixtures/search_live.json"));
        assert_eq!(live.page, 1);
        assert_eq!(live.num_pages, 2);
        assert_eq!(live.num_results, 26);
        assert_eq!(live.rooms.len(), 1);
        let room = &live.rooms[0];
        assert_eq!(room.room_id, 1_963_455_165);
        assert_eq!(room.name, "人枉");
        assert_eq!(room.title, "26.3aa，欢迎指导");
        assert_eq!(room.area, "我的世界");
        assert_eq!(room.online, 43);
        assert_eq!(
            room.cover,
            "https://i0.hdslb.com/bfs/live-key-frame/keyframe.jpg"
        );

        let all: SearchAll = parse(include_str!("../../fixtures/search_all.json"));
        assert_eq!(all.users.len(), 1);
        assert_eq!(all.users[0].name, "安东森");
        let count = |key: &str| all.pageinfo.get(key).map_or(0, |info| info.num_results);
        assert_eq!(count(SearchType::Video.as_str()), 1_000);
        assert_eq!(count(SearchType::Bangumi.as_str()), 1);
        assert_eq!(count(SearchType::Film.as_str()), 1);
        assert_eq!(count(SearchType::Live.as_str()), 26);
        assert_eq!(count(SearchType::Article.as_str()), 1_000);
        assert_eq!(count(SearchType::User.as_str()), 1_000);
        assert_eq!(count("missing"), 0);
    }

    #[test]
    fn maps_api_values() {
        assert_eq!(
            SearchOrder::ALL.map(SearchOrder::as_str),
            ["totalrank", "click", "pubdate", "dm", "stow"]
        );
        assert_eq!(
            SearchType::ALL.map(SearchType::as_str),
            [
                "video",
                "media_bangumi",
                "media_ft",
                "live",
                "article",
                "bili_user"
            ]
        );
    }
}
