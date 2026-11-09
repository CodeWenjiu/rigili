use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Client as HttpClient, RequestBuilder, Response, StatusCode,
    cookie::Jar,
    header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderValue, REFERER},
};
use serde::{Deserialize, de::DeserializeOwned};
use tokio::sync::{Mutex, Semaphore};

use crate::{ApiError, WbiSigner};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const API_REFERER: &str = "https://www.bilibili.com/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_LIMIT: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(250);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(200);
const MAX_CONCURRENT_REQUESTS: usize = 8;

#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    cookies: Arc<Jar>,
    next_request: Arc<Mutex<Option<Instant>>>,
    requests: Arc<Semaphore>,
}

impl Client {
    pub fn new() -> Result<Self, ApiError> {
        let cookies = Arc::new(Jar::default());
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static(API_REFERER));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9"));

        let http = HttpClient::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .cookie_provider(Arc::clone(&cookies))
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            http,
            cookies,
            next_request: Arc::new(Mutex::new(None)),
            requests: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
        })
    }

    pub fn cookies(&self) -> &Arc<Jar> {
        &self.cookies
    }

    pub async fn get_json<T>(&self, url: &str, params: &[(&str, String)]) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let response = self.send(|| self.http.get(url).query(params)).await?;
        let body = response.text().await?;
        parse_envelope(&body)
    }

    pub async fn get_signed<T>(
        &self,
        url: &str,
        params: &[(String, String)],
        signer: &WbiSigner,
    ) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let query = signer.sign(params, unix_timestamp());
        self.get_url_json(&format!("{url}?{query}")).await
    }

    pub(crate) async fn get_raw<T>(&self, url: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let response = self.send(|| self.http.get(url)).await?;
        let body = response.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    async fn get_url_json<T>(&self, url: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let response = self.send(|| self.http.get(url)).await?;
        let body = response.text().await?;
        parse_envelope(&body)
    }

    async fn send(
        &self,
        request: impl Fn() -> RequestBuilder + Send,
    ) -> Result<Response, ApiError> {
        let _permit = self
            .requests
            .acquire()
            .await
            .map_err(|_| ApiError::InvalidResponse("请求并发限制已关闭".into()))?;

        let mut attempt = 0;
        loop {
            self.wait_for_slot().await;
            match request().send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response)
                    if retryable_status(response.status()) && attempt + 1 < RETRY_LIMIT => {}
                Ok(response) => return Err(ApiError::Status(response.status())),
                Err(error) if retryable_error(&error) && attempt + 1 < RETRY_LIMIT => {}
                Err(error) => return Err(ApiError::from(error)),
            }

            attempt += 1;
            tokio::time::sleep(RETRY_DELAY * 2u32.pow(attempt as u32 - 1)).await;
        }
    }

    async fn wait_for_slot(&self) {
        let wait = {
            let mut next_slot = self.next_request.lock().await;
            let now = Instant::now();
            let wait = next_slot
                .map(|slot| slot.saturating_duration_since(now))
                .unwrap_or_default();
            *next_slot = Some(now.max(next_slot.unwrap_or(now)) + MIN_REQUEST_INTERVAL);
            wait
        };

        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

fn parse_envelope<T>(body: &str) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    let envelope: Envelope<T> = serde_json::from_str(body)?;
    if envelope.code != 0 {
        return Err(ApiError::Bilibili {
            code: envelope.code,
            message: envelope.message,
        });
    }
    envelope
        .data
        .ok_or_else(|| ApiError::InvalidResponse("响应缺少 data".into()))
}

fn retryable_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

fn retryable_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlayUrl, Popular, VideoInfo};

    const POPULAR_FIXTURE: &str = include_str!("../fixtures/popular.json");
    const VIEW_FIXTURE: &str = include_str!("../fixtures/view.json");
    const PLAYURL_FIXTURE: &str = include_str!("../fixtures/playurl.json");

    #[test]
    fn parses_popular_fixture() {
        let popular: Popular = parse_envelope(POPULAR_FIXTURE).expect("popular fixture");
        assert!(!popular.list.is_empty());
        assert_eq!(popular.list[0].bvid, "BV1TgWp6MEHJ");
        assert_eq!(popular.list[0].owner.name, "哔哩哔哩英雄联盟赛事");
    }

    #[test]
    fn parses_view_fixture() {
        let video: VideoInfo = parse_envelope(VIEW_FIXTURE).expect("view fixture");
        assert_eq!(video.bvid, "BV1TgWp6MEHJ");
        assert_eq!(video.cid, 43_448_467_681);
        assert_eq!(video.pages.len(), 2);
        assert_eq!(video.pages[1].part, "第二局");
    }

    #[test]
    fn parses_playurl_fixture_and_prefers_avc() {
        let playurl: PlayUrl = parse_envelope(PLAYURL_FIXTURE).expect("playurl fixture");
        let dash = playurl.dash.as_ref().expect("dash");
        let video = dash.best_video().expect("video");
        let audio = dash.best_audio().expect("audio");
        assert_eq!(video.codecid, 7);
        assert_eq!((video.width, video.height), (852, 480));
        assert_eq!(audio.bandwidth, 196_608);
    }

    #[test]
    fn exposes_risk_control_errors() {
        let error = parse_envelope::<Popular>(r#"{"code":-352,"message":"风控校验失败"}"#)
            .expect_err("risk control response");
        assert!(error.is_risk_control());
    }

    #[test]
    fn rejects_missing_data() {
        let error =
            parse_envelope::<Popular>(r#"{"code":0,"message":"0"}"#).expect_err("missing data");
        assert!(matches!(error, ApiError::InvalidResponse(_)));
    }
}
