use gpuili_bilibili::{ApiError, Client, PlayUrl, Popular, VideoInfo, api};

#[derive(Clone)]
pub struct Backend {
    client: Client,
}

impl Backend {
    pub fn new() -> Result<Self, ApiError> {
        Ok(Self {
            client: Client::new()?,
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
}
