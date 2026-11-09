use std::{collections::BTreeMap, sync::Arc};

use gpuili_bilibili::{
    ApiError, Client, Nav, PlayUrl, Popular, QrCode, QrPoll, SessionError, VideoInfo, api,
};

#[derive(Clone)]
pub struct Backend {
    client: Arc<Client>,
}

impl Backend {
    pub fn new() -> Result<Self, ApiError> {
        Ok(Self {
            client: Arc::new(Client::new()?),
        })
    }

    pub async fn popular(&self, page: u32, page_size: u32) -> Result<Popular, ApiError> {
        api::popular::fetch(&self.client, page, page_size).await
    }

    pub async fn view(&self, bvid: &str) -> Result<VideoInfo, ApiError> {
        api::view::fetch(&self.client, bvid).await
    }

    pub async fn playurl(&self, bvid: &str, cid: u64) -> Result<PlayUrl, ApiError> {
        api::playurl::fetch(&self.client, bvid, cid).await
    }

    pub async fn qr_generate(&self) -> Result<QrCode, ApiError> {
        api::login::generate(&self.client).await
    }

    pub async fn qr_poll(&self, qrcode_key: &str) -> Result<QrPoll, ApiError> {
        api::login::poll(&self.client, qrcode_key).await
    }

    pub async fn nav(&self) -> Result<Nav, ApiError> {
        api::login::nav(&self.client).await
    }

    pub fn apply_cookies(&self, cookies: BTreeMap<String, String>) {
        self.client.set_cookies(cookies);
    }

    pub fn cookie_jar_values(&self) -> BTreeMap<String, String> {
        self.client.jar_cookies()
    }

    pub fn save_session(&self) -> Result<(), SessionError> {
        self.client.save_session()
    }

    pub fn has_session(&self) -> bool {
        !self.client.cookie_values().is_empty()
    }
}
