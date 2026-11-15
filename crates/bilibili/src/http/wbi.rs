use md5::{Digest, Md5};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Deserialize;

use crate::{ApiError, Client};

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

const WBI_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

const NAV_URL: &str = "https://api.bilibili.com/x/web-interface/nav";

#[derive(Clone, Debug)]
pub struct WbiSigner {
    mixin_key: String,
}

impl WbiSigner {
    pub fn new(img_key: &str, sub_key: &str) -> Self {
        let raw = format!("{img_key}{sub_key}");
        let chars = raw.chars().collect::<Vec<_>>();
        let mixin_key = MIXIN_KEY_ENC_TAB
            .iter()
            .take(32)
            .filter_map(|&index| chars.get(index).copied())
            .collect();
        Self { mixin_key }
    }

    pub async fn fetch(client: &Client) -> Result<Self, ApiError> {
        let response: NavResponse = client.get_raw(NAV_URL).await?;
        let data = response
            .data
            .ok_or_else(|| ApiError::InvalidResponse("nav 响应缺少 data".into()))?;
        let img_key = key_from_url(&data.wbi_img.img_url)?;
        let sub_key = key_from_url(&data.wbi_img.sub_url)?;
        Ok(Self::new(img_key, sub_key))
    }

    pub fn sign(&self, params: &[(String, String)], timestamp: u64) -> String {
        let mut params = params.to_vec();
        params.push(("wts".to_owned(), timestamp.to_string()));
        params.sort_by(|left, right| left.0.cmp(&right.0));

        let query = params
            .iter()
            .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        let w_rid = hex::encode(Md5::digest(format!("{query}{}", self.mixin_key).as_bytes()));

        format!("{query}&w_rid={w_rid}")
    }
}

#[derive(Deserialize)]
struct NavResponse {
    data: Option<NavData>,
}

#[derive(Deserialize)]
struct NavData {
    wbi_img: WbiImg,
}

#[derive(Deserialize)]
struct WbiImg {
    img_url: String,
    sub_url: String,
}

fn key_from_url(url: &str) -> Result<&str, ApiError> {
    url.rsplit('/')
        .next()
        .and_then(|name| name.split('.').next())
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ApiError::InvalidResponse(format!("无效的 WBI key 地址：{url}")))
}

fn encode(value: &str) -> String {
    let filtered = value
        .chars()
        .filter(|character| !matches!(character, '!' | '\'' | '(' | ')' | '*'))
        .collect::<String>();
    utf8_percent_encode(&filtered, WBI_ENCODE_SET).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> WbiSigner {
        WbiSigner::new(
            "7cd084941338484aae1ad9425b84077c",
            "4932caff0ff746eab6f01bf08b70ac45",
        )
    }

    #[test]
    fn builds_mixin_key() {
        assert_eq!(signer().mixin_key, "ea1db124af3c7062474693fa704f4ff8");
    }

    #[test]
    fn signs_sorted_query() {
        let params = vec![
            ("foo".to_owned(), "114".to_owned()),
            ("bar".to_owned(), "514".to_owned()),
            ("zab".to_owned(), "1919810".to_owned()),
        ];
        assert_eq!(
            signer().sign(&params, 1702204169),
            "bar=514&foo=114&wts=1702204169&zab=1919810&w_rid=8f6f2b5b3d485fe1886cec6a0be8c5d4"
        );
    }

    #[test]
    fn strips_forbidden_characters_and_encodes() {
        assert_eq!(encode("a!b'c(d)e*f"), "abcdef");
        assert_eq!(encode("a b&c"), "a%20b%26c");
    }
}
