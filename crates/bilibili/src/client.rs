use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Client as HttpClient, RequestBuilder, Response, StatusCode, Url,
    cookie::{CookieStore, Jar},
    header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderValue, REFERER},
};
use serde::{Deserialize, de::DeserializeOwned};
use tokio::sync::{Mutex, OnceCell, Semaphore};

use crate::{ApiError, Session, SessionError, WbiSigner, session};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const API_REFERER: &str = "https://www.bilibili.com/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_LIMIT: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(250);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(200);
const MAX_CONCURRENT_REQUESTS: usize = 8;
const COOKIE_URL: &str = "https://api.bilibili.com/";

#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    jar: Arc<Jar>,
    cookies: Arc<std::sync::Mutex<BTreeMap<String, String>>>,
    session_dir: Option<PathBuf>,
    next_request: Arc<Mutex<Option<Instant>>>,
    requests: Arc<Semaphore>,
    wbi: Arc<OnceCell<WbiSigner>>,
    dev_id: String,
}

impl Client {
    pub fn new() -> Result<Self, ApiError> {
        Self::with_data_dir(session::data_dir())
    }

    pub fn with_data_dir(session_dir: Option<PathBuf>) -> Result<Self, ApiError> {
        let jar = Arc::new(Jar::default());
        let mut cookies = BTreeMap::new();

        if let Some(dir) = session_dir.as_deref()
            && let Ok(Some(session)) = Session::load(dir)
        {
            for (name, value) in session.cookies() {
                add_cookie(&jar, name, value);
            }
            cookies = session.cookies().clone();
        }

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
            .cookie_provider(Arc::clone(&jar))
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            http,
            jar,
            cookies: Arc::new(std::sync::Mutex::new(cookies)),
            session_dir,
            next_request: Arc::new(Mutex::new(None)),
            requests: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            wbi: Arc::new(OnceCell::new()),
            dev_id: generate_dev_id(),
        })
    }

    pub fn set_cookies(&self, cookies: impl IntoIterator<Item = (String, String)>) {
        let mut stored = lock_cookies(&self.cookies);
        for (name, value) in cookies {
            add_cookie(&self.jar, &name, &value);
            stored.insert(name, value);
        }
    }

    pub fn cookie_values(&self) -> BTreeMap<String, String> {
        lock_cookies(&self.cookies).clone()
    }

    pub fn csrf(&self) -> Result<String, ApiError> {
        self.cookie_values()
            .get("bili_jct")
            .filter(|value| !value.is_empty())
            .cloned()
            .ok_or_else(login_required)
    }

    pub fn account_uid(&self) -> Result<u64, ApiError> {
        self.cookie_values()
            .get("DedeUserID")
            .and_then(|value| value.parse().ok())
            .ok_or_else(login_required)
    }

    pub(crate) fn dev_id(&self) -> &str {
        &self.dev_id
    }

    pub fn jar_cookies(&self) -> BTreeMap<String, String> {
        let Ok(url) = Url::parse(COOKIE_URL) else {
            return BTreeMap::new();
        };
        let Some(header) = CookieStore::cookies(&*self.jar, &url) else {
            return BTreeMap::new();
        };
        let Ok(text) = header.to_str() else {
            return BTreeMap::new();
        };
        text.split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect()
    }

    pub fn save_session(&self) -> Result<(), SessionError> {
        let Some(dir) = self.session_dir.as_deref() else {
            return Ok(());
        };
        Session::new(self.cookie_values()).save(dir)
    }

    pub fn clear_session(&self) -> Result<(), SessionError> {
        lock_cookies(&self.cookies).clear();
        match self.session_dir.as_deref() {
            Some(dir) => Session::clear(dir),
            None => Ok(()),
        }
    }

    pub async fn get_json<T>(&self, url: &str, params: &[(&str, String)]) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let response = self.send(|| self.http.get(url).query(params)).await?;
        let body = response.text().await?;
        self.parse(&body)
    }

    pub async fn get_wbi_json<T>(&self, url: &str, params: &[(&str, String)]) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let signer = self.wbi.get_or_try_init(|| WbiSigner::fetch(self)).await?;
        let params = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<Vec<_>>();
        let query = signer.sign(&params, unix_timestamp());
        self.get_url_json(&format!("{url}?{query}")).await
    }

    pub async fn post_form<T>(&self, url: &str, params: &[(&str, String)]) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let response = self.send(|| self.http.post(url).form(params)).await?;
        let body = response.text().await?;
        self.parse(&body)
    }

    pub async fn post_wbi_form<T>(
        &self,
        url: &str,
        params: &[(&str, String)],
        form: &[(&str, String)],
    ) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let signer = self.wbi.get_or_try_init(|| WbiSigner::fetch(self)).await?;
        let params = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<Vec<_>>();
        let query = signer.sign(&params, unix_timestamp());
        let target = format!("{url}?{query}");
        let response = self.send(|| self.http.post(&target).form(form)).await?;
        let body = response.text().await?;
        self.parse(&body)
    }

    pub async fn post_form_ack(
        &self,
        url: &str,
        params: &[(&str, String)],
    ) -> Result<(), ApiError> {
        let response = self.send(|| self.http.post(url).form(params)).await?;
        let body = response.text().await?;
        self.parse_ack(&body)
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
        self.parse(&body)
    }

    fn parse<T>(&self, body: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        match parse_envelope(body) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.is_login_required() {
                    let _ = self.clear_session();
                }
                Err(error)
            }
        }
    }

    fn parse_ack(&self, body: &str) -> Result<(), ApiError> {
        let envelope: Envelope<serde_json::Value> = serde_json::from_str(body)?;
        if envelope.code == 0 {
            return Ok(());
        }

        let error = ApiError::Bilibili {
            code: envelope.code,
            message: envelope.message,
        };
        if error.is_login_required() {
            let _ = self.clear_session();
        }
        Err(error)
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
    result: Option<T>,
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
        .or(envelope.result)
        .ok_or_else(|| ApiError::InvalidResponse("响应缺少 data".into()))
}

fn retryable_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

fn retryable_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

pub(crate) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn login_required() -> ApiError {
    ApiError::Bilibili {
        code: -101,
        message: "账号未登录".into(),
    }
}

fn generate_dev_id() -> String {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let seed = nanos.wrapping_add(u128::from(counter) << 64);

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let high = hasher.finish();
    let mut hasher = DefaultHasher::new();
    (high, seed).hash(&mut hasher);
    let low = hasher.finish();

    let mut bytes = ((u128::from(high) << 64) | u128::from(low)).to_be_bytes();
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn add_cookie(jar: &Jar, name: &str, value: &str) {
    let cookie = format!("{name}={value}; Domain=.bilibili.com; Path=/; Secure");
    let url = Url::parse(COOKIE_URL).expect("cookie url is a constant");
    jar.add_cookie_str(&cookie, &url);
}

fn lock_cookies(
    cookies: &std::sync::Mutex<BTreeMap<String, String>>,
) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
    cookies
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlayUrl, Popular, Session, VideoInfo};
    use std::{collections::BTreeMap, fs, path::PathBuf};

    const POPULAR_FIXTURE: &str = include_str!("../fixtures/popular.json");
    const VIEW_FIXTURE: &str = include_str!("../fixtures/view.json");
    const PLAYURL_FIXTURE: &str = include_str!("../fixtures/playurl.json");

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rigili-client-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn session_cookies() -> BTreeMap<String, String> {
        BTreeMap::from([("SESSDATA".to_string(), "value".to_string())])
    }

    #[test]
    fn persists_and_clears_a_session() {
        let dir = temp_dir("session");
        let client = Client::with_data_dir(Some(dir.clone())).expect("client");

        client.set_cookies(session_cookies());
        client.save_session().expect("save session");
        assert_eq!(
            Session::load(&dir)
                .expect("load")
                .expect("session")
                .cookies(),
            &session_cookies()
        );

        let restored = Client::with_data_dir(Some(dir.clone())).expect("client");
        assert_eq!(restored.cookie_values(), session_cookies());

        restored.clear_session().expect("clear session");
        assert!(restored.cookie_values().is_empty());
        assert!(Session::load(&dir).expect("load").is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_cookies_back_from_the_jar() {
        let client = Client::with_data_dir(None).expect("client");
        client.set_cookies([("SESSDATA".to_string(), "value".to_string())]);
        assert_eq!(
            client.jar_cookies().get("SESSDATA").map(String::as_str),
            Some("value")
        );
    }

    #[test]
    fn clears_the_session_when_login_is_required() {
        let dir = temp_dir("login-required");
        Session::new(session_cookies())
            .save(&dir)
            .expect("save session");
        let client = Client::with_data_dir(Some(dir.clone())).expect("client");
        assert!(!client.cookie_values().is_empty());

        let error = client
            .parse::<Popular>(r#"{"code":-101,"message":"账号未登录"}"#)
            .expect_err("login required");
        assert!(error.is_login_required());
        assert!(client.cookie_values().is_empty());
        assert!(Session::load(&dir).expect("load").is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exposes_account_cookies_and_a_stable_device_id() {
        let client = Client::with_data_dir(None).expect("client");
        assert!(matches!(
            client.csrf(),
            Err(ApiError::Bilibili { code: -101, .. })
        ));
        assert!(matches!(
            client.account_uid(),
            Err(ApiError::Bilibili { code: -101, .. })
        ));

        client.set_cookies([
            ("bili_jct".to_string(), "csrf".to_string()),
            ("DedeUserID".to_string(), "42".to_string()),
        ]);
        assert_eq!(client.csrf().expect("csrf"), "csrf");
        assert_eq!(client.account_uid().expect("uid"), 42);

        let dev_id = client.dev_id();
        assert_eq!(dev_id.len(), 36);
        let groups = dev_id.split('-').map(str::len).collect::<Vec<_>>();
        assert_eq!(groups, [8, 4, 4, 4, 12]);
        assert!(
            dev_id
                .chars()
                .all(|character| character.is_ascii_hexdigit() || character == '-')
        );
        assert_eq!(client.clone().dev_id(), dev_id);
    }

    #[test]
    fn parses_api_fixtures() {
        let popular: Popular = parse_envelope(POPULAR_FIXTURE).expect("popular fixture");
        assert!(!popular.list.is_empty());
        assert_eq!(popular.list[0].bvid, "BV1TgWp6MEHJ");
        assert_eq!(popular.list[0].owner.name, "哔哩哔哩英雄联盟赛事");

        let info: VideoInfo = parse_envelope(VIEW_FIXTURE).expect("view fixture");
        assert_eq!(info.bvid, "BV1TgWp6MEHJ");
        assert_eq!(info.cid, 43_448_467_681);
        assert_eq!(info.pages.len(), 2);
        assert_eq!(info.pages[1].part, "第二局");

        let playurl: PlayUrl = parse_envelope(PLAYURL_FIXTURE).expect("playurl fixture");
        let dash = playurl.dash.as_ref().expect("dash");
        let video = dash.best_video().expect("video");
        let audio = dash.best_audio().expect("audio");
        assert_eq!(video.codecid, 7);
        assert_eq!((video.width, video.height), (852, 480));
        assert_eq!(audio.bandwidth, 196_608);
    }

    #[test]
    fn surfaces_api_errors() {
        let risk = parse_envelope::<Popular>(r#"{"code":-352,"message":"风控校验失败"}"#)
            .expect_err("risk control response");
        assert!(risk.is_risk_control());

        let missing =
            parse_envelope::<Popular>(r#"{"code":0,"message":"0"}"#).expect_err("missing data");
        assert!(matches!(missing, ApiError::InvalidResponse(_)));
    }

    #[test]
    fn accepts_acknowledgements_without_data() {
        let client = Client::with_data_dir(None).expect("client");
        client
            .parse_ack(r#"{"code":0,"message":"0"}"#)
            .expect("acknowledgement");

        let error = client
            .parse_ack(r#"{"code":-400,"message":"请求错误"}"#)
            .expect_err("api error");
        assert!(matches!(error, ApiError::Bilibili { code: -400, .. }));
    }
}
