use crate::{ApiError, Client, Popular};

const POPULAR_URL: &str = "https://api.bilibili.com/x/web-interface/popular";

pub async fn fetch(client: &Client, page: u32, page_size: u32) -> Result<Popular, ApiError> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 50);

    client
        .get_json(
            POPULAR_URL,
            &[("pn", page.to_string()), ("ps", page_size.to_string())],
        )
        .await
}
