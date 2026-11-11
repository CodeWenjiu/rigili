use serde::Deserialize;

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
    #[serde(default)]
    pub rpid: u64,
    #[serde(default)]
    pub member: CommentMember,
    #[serde(default)]
    pub content: CommentContent,
    #[serde(default)]
    pub like: u64,
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
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommentMember {
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
    use super::CommentPage;

    #[test]
    fn parses_comments_and_tolerates_missing_replies() {
        let page: CommentPage = serde_json::from_str(
            r#"{
                "page": {"acount": 2, "count": 2, "num": 1, "size": 20},
                "replies": [{
                    "rpid": 1,
                    "member": {
                        "uname": "测试用户",
                        "avatar": "http://i0.hdslb.com/face.jpg",
                        "level_info": {"current_level": 5}
                    },
                    "content": {"message": "第一条评论"},
                    "like": 12,
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
        assert_eq!(items[0].member.uname, "测试用户");
        assert_eq!(items[0].member.level_info.current_level, 5);
        assert_eq!(
            items[0].member.avatar_url(),
            "https://i0.hdslb.com/face.jpg"
        );
        assert_eq!(items[0].content.message, "第一条评论");
        assert_eq!(items[0].reply_control.time_desc, "1天前发布");
        assert_eq!(items[0].replies().len(), 1);
        assert_eq!(items[0].replies()[0].content.message, "回复内容");

        let empty: CommentPage =
            serde_json::from_str(r#"{"page": {"count": 0}}"#).expect("empty comment fixture");
        assert!(empty.into_replies().is_empty());
    }
}
