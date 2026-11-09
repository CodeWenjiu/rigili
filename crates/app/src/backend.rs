use gpuili_bilibili::{ApiError, Client, Popular, api};

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
}
