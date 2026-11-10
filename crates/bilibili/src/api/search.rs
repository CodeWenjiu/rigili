use crate::{ApiError, Client, SearchResult};

const SEARCH_URL: &str = "https://api.bilibili.com/x/web-interface/wbi/search/type";
const PAGE_SIZE: u32 = 20;

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

pub async fn fetch(
    client: &Client,
    keyword: &str,
    page: u32,
    order: SearchOrder,
) -> Result<SearchResult, ApiError> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err(ApiError::InvalidResponse("搜索关键词不能为空".into()));
    }

    client
        .get_wbi_json(
            SEARCH_URL,
            &[
                ("search_type", "video".into()),
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
    use super::SearchOrder;
    use crate::SearchResult;

    const SEARCH_FIXTURE: &str = include_str!("../../fixtures/search.json");

    #[derive(serde::Deserialize)]
    struct Envelope {
        data: SearchResult,
    }

    #[test]
    fn parses_search_fixture() {
        let envelope: Envelope = serde_json::from_str(SEARCH_FIXTURE).expect("search fixture");
        let search = envelope.data;

        assert_eq!(search.page, 1);
        assert_eq!(search.num_pages, 62);
        assert_eq!(search.num_results, 1_234);
        assert_eq!(search.videos.len(), 2);
        assert_eq!(search.videos[0].title, "【测试】哔哩哔哩");
        assert_eq!(search.videos[1].duration, 3_723);
    }

    #[test]
    fn maps_search_orders_to_api_values() {
        assert_eq!(SearchOrder::Default.as_str(), "totalrank");
        assert_eq!(SearchOrder::Clicks.as_str(), "click");
        assert_eq!(SearchOrder::Newest.as_str(), "pubdate");
        assert_eq!(SearchOrder::Danmaku.as_str(), "dm");
        assert_eq!(SearchOrder::Favorites.as_str(), "stow");
    }
}
