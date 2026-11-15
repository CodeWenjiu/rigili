use crate::{ApiError, Client, CommentPage};

const COMMENTS_URL: &str = "https://api.bilibili.com/x/v2/reply";
const ADD_URL: &str = "https://api.bilibili.com/x/v2/reply/add";
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

impl Client {
    pub async fn comments(
        &self,
        kind: u32,
        oid: &str,
        page: u32,
        sort: CommentSort,
    ) -> Result<CommentPage, ApiError> {
        self.get_json(
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

    pub async fn add_comment(&self, aid: u64, message: &str) -> Result<(), ApiError> {
        self.post_form_ack(
            ADD_URL,
            &[
                ("type", "1".to_owned()),
                ("oid", aid.to_string()),
                ("message", message.to_owned()),
                ("plat", "1".to_owned()),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }
}
