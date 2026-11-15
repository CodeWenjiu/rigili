use std::collections::HashSet;

use serde::Deserialize;

use crate::{
    ApiError, Client, MessageCursor, MessageFeed, MessageFeedItem, MessageSessions, MessageUserInfo,
};

const SESSIONS_URL: &str = "https://api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions";
const USER_INFOS_URL: &str = "https://api.vc.bilibili.com/account/v1/user/infos";
const SEND_MSG_URL: &str = "https://api.vc.bilibili.com/web_im/v1/web_im/send_msg";
const MSGFEED_URL: &str = "https://api.bilibili.com/x/msgfeed";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageFeedKind {
    Reply,
    At,
    Like,
}

impl MessageFeedKind {
    fn path(self) -> &'static str {
        match self {
            Self::Reply => "reply",
            Self::At => "at",
            Self::Like => "like",
        }
    }

    fn time_param(self) -> &'static str {
        match self {
            Self::Reply => "reply_time",
            Self::At => "at_time",
            Self::Like => "like_time",
        }
    }
}

pub async fn sessions(client: &Client) -> Result<MessageSessions, ApiError> {
    client
        .get_json(
            SESSIONS_URL,
            &[
                ("session_type", "1".into()),
                ("group_id", "0".into()),
                ("build", "0".into()),
                ("mobi_app", "web".into()),
            ],
        )
        .await
}

pub async fn user_infos(client: &Client, uids: &[u64]) -> Result<Vec<MessageUserInfo>, ApiError> {
    let uids = uids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    client.get_json(USER_INFOS_URL, &[("uids", uids)]).await
}

pub async fn send_message(client: &Client, receiver: u64, content: &str) -> Result<(), ApiError> {
    let sender = client.account_uid()?;
    let csrf = client.csrf()?;
    let dev_id = client.dev_id().to_owned();
    let timestamp = crate::http::unix_timestamp();
    let content = serde_json::json!({ "content": content }).to_string();

    let form = [
        ("msg[sender_uid]", sender.to_string()),
        ("msg[receiver_id]", receiver.to_string()),
        ("msg[receiver_type]", "1".to_owned()),
        ("msg[msg_type]", "1".to_owned()),
        ("msg[msg_status]", "0".to_owned()),
        ("msg[dev_id]", dev_id.clone()),
        ("msg[timestamp]", timestamp.to_string()),
        ("msg[new_face_version]", "1".to_owned()),
        ("msg[content]", content),
        ("csrf", csrf.clone()),
        ("csrf_token", csrf),
        ("build", "0".to_owned()),
        ("mobi_app", "web".to_owned()),
    ];
    let query = [
        ("w_sender_uid", sender.to_string()),
        ("w_receiver_id", receiver.to_string()),
        ("w_dev_id", dev_id),
    ];

    client
        .post_wbi_form::<serde_json::Value>(SEND_MSG_URL, &query, &form)
        .await
        .map(|_| ())
}

pub async fn feed(
    client: &Client,
    kind: MessageFeedKind,
    cursor: Option<MessageCursor>,
) -> Result<MessageFeed, ApiError> {
    let url = format!("{MSGFEED_URL}/{}", kind.path());
    let mut params = vec![
        ("platform", "web".to_owned()),
        ("build", "0".to_owned()),
        ("mobi_app", "web".to_owned()),
    ];
    if let Some(cursor) = cursor.filter(|cursor| cursor.id > 0) {
        params.push(("id", cursor.id.to_string()));
        params.push((kind.time_param(), cursor.time.to_string()));
    }

    if kind == MessageFeedKind::Like {
        let feed: LikeFeed = client.get_wbi_json(&url, &params).await?;
        let mut seen = HashSet::new();
        let mut items = Vec::with_capacity(feed.latest.items.len() + feed.total.items.len());
        for item in feed.latest.items.into_iter().chain(feed.total.items) {
            if item.id == 0 || seen.insert(item.id) {
                items.push(item);
            }
        }
        return Ok(MessageFeed {
            items,
            cursor: feed.total.cursor,
        });
    }

    client.get_wbi_json(&url, &params).await
}

#[derive(Default, Deserialize)]
struct LikeFeed {
    #[serde(default)]
    latest: LikeSection,
    #[serde(default)]
    total: LikeSection,
}

#[derive(Default, Deserialize)]
struct LikeSection {
    #[serde(default)]
    items: Vec<MessageFeedItem>,
    #[serde(default)]
    cursor: MessageCursor,
}

#[cfg(test)]
mod tests {
    use super::LikeFeed;

    #[test]
    fn parses_like_feed_sections() {
        let feed: LikeFeed = serde_json::from_str(
            r#"{
                "latest": {"items": [], "last_view_at": 0},
                "total": {
                    "cursor": {"id": 11, "is_end": false, "time": 1700000000},
                    "items": [{"id": 7, "users": [{"mid": 42, "nickname": "用户"}]}]
                }
            }"#,
        )
        .expect("like feed");
        assert!(feed.latest.items.is_empty());
        assert_eq!(feed.total.items.len(), 1);
        assert_eq!(feed.total.items[0].users[0].mid, 42);
        assert_eq!(feed.total.cursor.time, 1_700_000_000);
    }
}
