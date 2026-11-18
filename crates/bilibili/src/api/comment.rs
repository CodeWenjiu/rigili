use crate::{ApiError, Client, CommentPage, EmotePanel};

const COMMENTS_URL: &str = "https://api.bilibili.com/x/v2/reply";
const REPLIES_URL: &str = "https://api.bilibili.com/x/v2/reply/reply";
const ADD_URL: &str = "https://api.bilibili.com/x/v2/reply/add";
const LIKE_URL: &str = "https://api.bilibili.com/x/v2/reply/action";
const EMOTE_PANEL_URL: &str = "https://api.bilibili.com/x/emote/user/panel/web";
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommentReply {
    pub root: u64,
    pub parent: u64,
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

    pub async fn comment_replies(
        &self,
        aid: u64,
        root: u64,
        page: u32,
    ) -> Result<CommentPage, ApiError> {
        self.get_json(
            REPLIES_URL,
            &[
                ("type", "1".to_owned()),
                ("oid", aid.to_string()),
                ("root", root.to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("pn", page.max(1).to_string()),
            ],
        )
        .await
    }

    pub async fn comment_emotes(&self) -> Result<EmotePanel, ApiError> {
        self.get_json(EMOTE_PANEL_URL, &[("business", "reply".to_owned())])
            .await
    }

    pub async fn add_comment(
        &self,
        aid: u64,
        message: &str,
        reply: Option<CommentReply>,
    ) -> Result<(), ApiError> {
        let mut form = vec![
            ("type", "1".to_owned()),
            ("oid", aid.to_string()),
            ("message", message.to_owned()),
            ("plat", "1".to_owned()),
        ];
        if let Some(reply) = reply {
            form.push(("root", reply.root.to_string()));
            form.push(("parent", reply.parent.to_string()));
        }
        form.push(("csrf", self.csrf()?));
        self.post_form_ack(ADD_URL, &form).await
    }

    pub async fn like_comment(&self, aid: u64, rpid: u64, like: bool) -> Result<(), ApiError> {
        self.post_form_ack(
            LIKE_URL,
            &[
                ("type", "1".to_owned()),
                ("oid", aid.to_string()),
                ("rpid", rpid.to_string()),
                ("action", if like { "1" } else { "0" }.to_owned()),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }
}
