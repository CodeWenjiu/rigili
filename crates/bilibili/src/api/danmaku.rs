use crate::{ApiError, Client, Danmaku, parse_segment};

const SEGMENT_URL: &str = "https://api.bilibili.com/x/v2/dm/web/seg.so";
const POST_URL: &str = "https://api.bilibili.com/x/v2/dm/post";
const WHITE: u32 = 0x00ff_ffff;

impl Client {
    pub async fn danmaku_segment(
        &self,
        cid: u64,
        aid: u64,
        segment: u32,
    ) -> Result<Vec<Danmaku>, ApiError> {
        let url = format!("{SEGMENT_URL}?type=1&oid={cid}&pid={aid}&segment_index={segment}");
        let body = self.get_bytes(&url).await?;
        parse_segment(&body).map_err(|error| ApiError::InvalidResponse(error.to_string()))
    }

    pub async fn post_danmaku(
        &self,
        cid: u64,
        bvid: &str,
        message: &str,
        progress: f64,
    ) -> Result<(), ApiError> {
        let mut form = danmaku_form(cid, bvid, message, progress);
        form.push(("csrf", self.csrf()?));
        self.post_form_ack(POST_URL, &form).await
    }
}

fn danmaku_form(cid: u64, bvid: &str, message: &str, progress: f64) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("type", "1".to_owned()),
        ("oid", cid.to_string()),
        ("msg", message.to_owned()),
        (
            "progress",
            ((progress.max(0.) * 1000.)
                .round()
                .clamp(0., u64::MAX as f64) as u64)
                .to_string(),
        ),
        ("color", WHITE.to_string()),
        ("fontsize", "25".to_owned()),
        ("mode", "1".to_owned()),
        ("pool", "0".to_owned()),
        ("plat", "1".to_owned()),
    ];
    if !bvid.is_empty() {
        form.push(("bvid", bvid.to_owned()));
    }
    form
}

#[cfg(test)]
mod tests {
    use super::danmaku_form;

    fn value<'a>(form: &'a [(&'static str, String)], key: &str) -> &'a str {
        form.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
            .expect(key)
    }

    #[test]
    fn builds_a_scrolling_white_danmaku() {
        let form = danmaku_form(123, "BV1xx", "前方高能", 12.345);
        assert_eq!(value(&form, "oid"), "123");
        assert_eq!(value(&form, "bvid"), "BV1xx");
        assert_eq!(value(&form, "msg"), "前方高能");
        assert_eq!(value(&form, "progress"), "12345");
        assert_eq!(value(&form, "color"), "16777215");
        assert_eq!(value(&form, "mode"), "1");
    }

    #[test]
    fn omits_an_unknown_bvid_and_clamps_progress() {
        let form = danmaku_form(1, "", "text", -3.0);
        assert!(form.iter().all(|(key, _)| *key != "bvid"));
        assert_eq!(value(&form, "progress"), "0");
    }
}
