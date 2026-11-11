use crate::{ApiError, Client, CommentPage};

const COMMENTS_URL: &str = "https://api.bilibili.com/x/v2/reply";
const PAGE_SIZE: u32 = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentSort {
    #[default]
    Hot,
    Latest,
}

impl CommentSort {
    fn query(self) -> &'static str {
        match self {
            Self::Hot => "1",
            Self::Latest => "0",
        }
    }
}

pub async fn fetch(
    client: &Client,
    kind: u32,
    oid: &str,
    page: u32,
    sort: CommentSort,
) -> Result<CommentPage, ApiError> {
    client
        .get_json(
            COMMENTS_URL,
            &[
                ("type", kind.to_string()),
                ("oid", oid.to_owned()),
                ("sort", sort.query().into()),
                ("ps", PAGE_SIZE.to_string()),
                ("pn", page.max(1).to_string()),
            ],
        )
        .await
}
