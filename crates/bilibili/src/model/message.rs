use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageSessions {
    #[serde(default)]
    pub session_list: Vec<MessageSession>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageSession {
    #[serde(default)]
    pub talker_id: u64,
    #[serde(default)]
    pub session_type: u32,
    #[serde(default)]
    pub at_time: i64,
    #[serde(default)]
    pub unread_count: u32,
    #[serde(default)]
    pub system_msg_type: u32,
    #[serde(default)]
    pub account_info: Option<MessageAccount>,
    #[serde(default)]
    pub last_msg: Option<MessageEnvelope>,
}

impl MessageSession {
    pub fn timestamp(&self) -> i64 {
        let time = normalize_timestamp(self.at_time);
        if time > 0 {
            time
        } else {
            self.last_msg
                .as_ref()
                .map_or(0, |msg| normalize_timestamp(msg.timestamp))
        }
    }
}

fn normalize_timestamp(time: i64) -> i64 {
    if time > 1_000_000_000_000 {
        time / 1000
    } else {
        time
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageAccount {
    #[serde(default, alias = "nickname")]
    pub name: String,
    #[serde(default, alias = "face")]
    pub pic_url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageEnvelope {
    #[serde(default)]
    pub sender_uid: u64,
    #[serde(default)]
    pub msg_type: u32,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: i64,
}

impl MessageEnvelope {
    pub fn preview(&self) -> String {
        if let Ok(payload) = serde_json::from_str::<SessionPayload>(&self.content) {
            let text = if payload.content.is_empty() {
                payload.title
            } else {
                payload.content
            };
            if !text.is_empty() {
                return text.trim().to_owned();
            }
        }

        match self.msg_type {
            2 => "[图片]".into(),
            3 => "[表情]".into(),
            _ if self.content.is_empty() => "[消息]".into(),
            _ => self.content.trim().to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct SessionPayload {
    #[serde(default)]
    content: String,
    #[serde(default)]
    title: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageFeed {
    #[serde(default)]
    pub items: Vec<MessageFeedItem>,
    #[serde(default)]
    pub cursor: MessageCursor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct MessageCursor {
    #[serde(default)]
    pub is_end: bool,
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub time: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageFeedItem {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub user: MessageUser,
    #[serde(default)]
    pub users: Vec<MessageUser>,
    #[serde(default)]
    pub item: MessageSubject,
    #[serde(default)]
    pub reply: MessageReply,
    #[serde(default)]
    pub counts: u64,
    #[serde(default)]
    pub time: i64,
    #[serde(default)]
    pub at_time: i64,
    #[serde(default)]
    pub like_time: i64,
}

impl MessageFeedItem {
    pub fn timestamp(&self) -> i64 {
        [self.time, self.at_time, self.like_time, self.reply.ctime]
            .into_iter()
            .map(normalize_timestamp)
            .find(|time| *time > 0)
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageUser {
    #[serde(default)]
    pub mid: u64,
    #[serde(default, alias = "name")]
    pub nickname: String,
    #[serde(default, alias = "face")]
    pub avatar: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageUserInfo {
    #[serde(default)]
    pub uid: u64,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageSubject {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub business: String,
    #[serde(default, rename = "type")]
    pub item_type: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageReply {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub ctime: i64,
}

#[cfg(test)]
mod tests {
    use super::{MessageEnvelope, MessageFeedItem, MessageReply, MessageSession, MessageUserInfo};

    #[test]
    fn parses_batch_user_infos() {
        let infos: Vec<MessageUserInfo> = serde_json::from_str(
            r#"[{"uid":42,"uname":"用户","face":"https://example.invalid/face.jpg"}]"#,
        )
        .expect("user infos");
        assert_eq!(infos[0].uid, 42);
        assert_eq!(infos[0].uname, "用户");
        assert_eq!(infos[0].face, "https://example.invalid/face.jpg");
    }

    #[test]
    fn extracts_text_from_session_payloads() {
        let text = MessageEnvelope {
            msg_type: 1,
            content: r#"{"content":"你好"}"#.into(),
            ..MessageEnvelope::default()
        };
        assert_eq!(text.preview(), "你好");

        let image = MessageEnvelope {
            msg_type: 2,
            content: r#"{"url":"https://example.invalid/image.jpg"}"#.into(),
            ..MessageEnvelope::default()
        };
        assert_eq!(image.preview(), "[图片]");

        let plain = MessageEnvelope {
            msg_type: 1,
            content: "纯文本".into(),
            ..MessageEnvelope::default()
        };
        assert_eq!(plain.preview(), "纯文本");
    }

    #[test]
    fn picks_the_first_available_feed_timestamp() {
        let item = MessageFeedItem {
            at_time: 1_700_000_000,
            reply: MessageReply {
                ctime: 1_600_000_000,
                ..MessageReply::default()
            },
            ..MessageFeedItem::default()
        };
        assert_eq!(item.timestamp(), 1_700_000_000);
    }

    #[test]
    fn parses_like_feed_items() {
        let item: MessageFeedItem = serde_json::from_str(
            r#"{
                "id": 7,
                "users": [{"mid": 42, "nickname": "用户", "avatar": "https://example.invalid/face.jpg"}],
                "item": {"title": "还有新三国", "business": "评论"},
                "counts": 3,
                "like_time": 1700000000
            }"#,
        )
        .expect("like item");
        assert_eq!(item.users[0].mid, 42);
        assert_eq!(item.users[0].nickname, "用户");
        assert_eq!(item.item.title, "还有新三国");
        assert_eq!(item.item.business, "评论");
        assert_eq!(item.timestamp(), 1_700_000_000);
    }

    #[test]
    fn normalizes_session_timestamps() {
        let session = MessageSession {
            at_time: 1_700_000_000_000,
            ..MessageSession::default()
        };
        assert_eq!(session.timestamp(), 1_700_000_000);

        let fallback = MessageSession {
            last_msg: Some(MessageEnvelope {
                timestamp: 1_700_000_000_000,
                ..MessageEnvelope::default()
            }),
            ..MessageSession::default()
        };
        assert_eq!(fallback.timestamp(), 1_700_000_000);
    }
}
