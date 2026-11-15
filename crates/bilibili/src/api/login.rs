use std::collections::BTreeMap;

use percent_encoding::percent_decode_str;

use crate::{ApiError, Client, Nav, QrCode, QrPoll};

const GENERATE_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const POLL_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";
const NAV_URL: &str = "https://api.bilibili.com/x/web-interface/nav";

const LOGIN_COOKIES: [&str; 4] = ["SESSDATA", "bili_jct", "DedeUserID", "DedeUserID__ckMd5"];

impl Client {
    pub async fn qr_generate(&self) -> Result<QrCode, ApiError> {
        self.get_json(GENERATE_URL, &[]).await
    }

    pub async fn qr_poll(&self, qrcode_key: &str) -> Result<QrPoll, ApiError> {
        self.get_json(POLL_URL, &[("qrcode_key", qrcode_key.to_owned())])
            .await
    }

    pub async fn nav(&self) -> Result<Nav, ApiError> {
        self.get_json(NAV_URL, &[]).await
    }
}

pub fn cookies_from_login_url(url: &str) -> BTreeMap<String, String> {
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return BTreeMap::new();
    };

    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(name, _)| LOGIN_COOKIES.contains(name))
        .map(|(name, value)| (name.to_owned(), decode(value)))
        .collect()
}

fn decode(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::cookies_from_login_url;

    #[test]
    fn extracts_login_cookies_and_ignores_other_urls() {
        let cookies = cookies_from_login_url(
            "https://passport.biligame.com/crossDomain?DedeUserID=42&DedeUserID__ckMd5=abc\
             &Expires=1762732800&SESSDATA=token%2C1762732800%2Csign&bili_jct=csrf&gourl=https%3A%2F%2Fwww.bilibili.com",
        );

        assert_eq!(cookies.len(), 4);
        assert_eq!(cookies["DedeUserID"], "42");
        assert_eq!(cookies["DedeUserID__ckMd5"], "abc");
        assert_eq!(cookies["SESSDATA"], "token,1762732800,sign");
        assert_eq!(cookies["bili_jct"], "csrf");

        assert!(cookies_from_login_url("https://www.bilibili.com").is_empty());
        assert!(cookies_from_login_url("https://example.com/?other=1").is_empty());
    }
}
