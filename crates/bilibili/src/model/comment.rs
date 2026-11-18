use std::collections::HashMap;

use serde::Deserialize;

fn deserialize_id<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Number(u64),
        Text(String),
    }

    Ok(match Option::<Id>::deserialize(deserializer)? {
        Some(Id::Number(value)) => value,
        Some(Id::Text(text)) => text.trim().parse().unwrap_or_default(),
        None => 0,
    })
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentPage {
    #[serde(default)]
    pub page: CommentPageInfo,
    #[serde(default)]
    pub replies: Option<Vec<Comment>>,
}

impl CommentPage {
    pub fn into_replies(self) -> Vec<Comment> {
        self.replies.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentPageInfo {
    #[serde(default)]
    pub acount: u64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub num: u32,
    #[serde(default)]
    pub size: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Comment {
    #[serde(default, deserialize_with = "deserialize_id")]
    pub rpid: u64,
    #[serde(default)]
    pub member: CommentMember,
    #[serde(default)]
    pub content: CommentContent,
    #[serde(default)]
    pub like: u64,
    /// Bit 0 reports whether the signed-in user already liked the comment.
    #[serde(default)]
    pub action: u8,
    #[serde(default)]
    pub ctime: i64,
    #[serde(default)]
    pub rcount: u64,
    #[serde(default)]
    pub replies: Option<Vec<Comment>>,
    #[serde(default)]
    pub reply_control: CommentControl,
}

impl Comment {
    pub fn replies(&self) -> &[Comment] {
        self.replies.as_deref().unwrap_or_default()
    }

    pub fn liked(&self) -> bool {
        self.action & 1 != 0
    }

    /// Applies the like the site will record, so the count and the highlight
    /// move before the request lands.
    pub fn set_liked(&mut self, liked: bool) {
        if self.liked() == liked {
            return;
        }
        if liked {
            self.action |= 1;
            self.like = self.like.saturating_add(1);
        } else {
            self.action &= !1;
            self.like = self.like.saturating_sub(1);
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentMember {
    #[serde(default, deserialize_with = "deserialize_id")]
    pub mid: u64,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub level_info: CommentLevel,
}

impl CommentMember {
    pub fn avatar_url(&self) -> String {
        self.avatar.replacen("http://", "https://", 1)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentLevel {
    #[serde(default)]
    pub current_level: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentContent {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub emote: Option<HashMap<String, CommentEmote>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentEmote {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EmotePanel {
    #[serde(default)]
    pub packages: Option<Vec<EmotePackage>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EmotePackage {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub emote: Option<Vec<EmoteItem>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EmoteItem {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentControl {
    #[serde(default)]
    pub time_desc: String,
    #[serde(default)]
    pub location: String,
}

#[cfg(test)]
mod tests {
    use super::{Comment, CommentPage, EmotePanel};

    #[test]
    fn parses_comments_and_tolerates_missing_replies() {
        let page: CommentPage = serde_json::from_str(
            r#"{
                "page": {"acount": 2, "count": 2, "num": 1, "size": 20},
                "replies": [{
                    "rpid": "3546918709889670",
                    "member": {
                        "mid": "100",
                        "uname": "测试用户",
                        "avatar": "http://i0.hdslb.com/face.jpg",
                        "level_info": {"current_level": 5}
                    },
                    "content": {"message": "第一条评论", "emote": {"[doge]": {"text": "[doge]", "url": "https://i0.hdslb.com/bfs/emote/doge.png"}}},
                    "like": 12,
                    "action": 1,
                    "ctime": 1762646400,
                    "rcount": 1,
                    "reply_control": {"time_desc": "1天前发布", "location": "IP属地：北京"},
                    "replies": [{
                        "rpid": 2,
                        "member": {"uname": "回复者"},
                        "content": {"message": "回复内容"},
                        "like": 1
                    }]
                }]
            }"#,
        )
        .expect("comment page fixture");

        assert_eq!(page.page.count, 2);
        let items = page.into_replies();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rpid, 3546918709889670);
        assert_eq!(items[0].member.mid, 100);
        assert_eq!(items[0].member.uname, "测试用户");
        assert_eq!(items[0].member.level_info.current_level, 5);
        assert_eq!(
            items[0].member.avatar_url(),
            "https://i0.hdslb.com/face.jpg"
        );
        assert_eq!(items[0].content.message, "第一条评论");
        assert!(items[0].liked());
        let emote = items[0]
            .content
            .emote
            .as_ref()
            .and_then(|emotes| emotes.get("[doge]"))
            .expect("comment emote");
        assert_eq!(emote.url, "https://i0.hdslb.com/bfs/emote/doge.png");
        assert_eq!(items[0].reply_control.time_desc, "1天前发布");
        assert_eq!(items[0].replies().len(), 1);
        assert_eq!(items[0].replies()[0].rpid, 2);
        assert_eq!(items[0].replies()[0].content.message, "回复内容");

        let empty: CommentPage =
            serde_json::from_str(r#"{"page": {"count": 0}}"#).expect("empty comment fixture");
        assert!(empty.into_replies().is_empty());
    }

    #[test]
    fn liking_moves_the_count_once_in_each_direction() {
        let mut comment = Comment::default();
        assert!(!comment.liked());

        comment.set_liked(true);
        assert!(comment.liked());
        assert_eq!(comment.like, 1);

        comment.set_liked(true);
        assert_eq!(comment.like, 1);

        comment.set_liked(false);
        assert!(!comment.liked());
        assert_eq!(comment.like, 0);

        comment.set_liked(false);
        assert_eq!(comment.like, 0);
    }

    #[test]
    fn parses_emote_packages_and_tolerates_null_packages() {
        let panel: EmotePanel = serde_json::from_str(
            r#"{
                "packages": [{
                    "text": "小黄脸",
                    "url": "//i0.hdslb.com/bfs/emote/package.png",
                    "emote": [{
                        "text": "[doge]",
                        "url": "//i0.hdslb.com/bfs/emote/doge.png"
                    }]
                }]
            }"#,
        )
        .expect("emote panel fixture");

        let packages = panel.packages.expect("emote packages");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].text, "小黄脸");
        let emotes = packages[0].emote.as_ref().expect("package emotes");
        assert_eq!(emotes[0].text, "[doge]");
        assert_eq!(emotes[0].url, "//i0.hdslb.com/bfs/emote/doge.png");

        let empty: EmotePanel =
            serde_json::from_str(r#"{"packages": null}"#).expect("null panel fixture");
        assert!(empty.packages.is_none());
    }
}
