use std::{sync::Arc, time::Duration};

use futures::{AsyncReadExt, future::BoxFuture};
use gpui_kit::http_client::{AsyncBody, HttpClient, Url, http};
use gpui_kit::*;
use tokio::sync::Semaphore;

use rigili_core::runtime::Tokio;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const REFERER: &str = "https://www.bilibili.com/";
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CONCURRENT_IMAGES: usize = 8;

pub(crate) fn init(cx: &mut App) {
    cx.set_http_client(Arc::new(ImageHttpClient::new(Tokio::handle(cx))));
}

struct ImageHttpClient {
    client: reqwest::Client,
    user_agent: http::HeaderValue,
    handle: tokio::runtime::Handle,
    requests: Arc<Semaphore>,
}

impl ImageHttpClient {
    fn new(handle: tokio::runtime::Handle) -> Self {
        let mut headers = http::HeaderMap::new();
        headers.insert(REFERER_HEADER, http::HeaderValue::from_static(REFERER));

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(TIMEOUT)
            .build()
            .expect("failed to initialize image HTTP client");

        Self {
            client,
            user_agent: http::HeaderValue::from_static(USER_AGENT),
            handle,
            requests: Arc::new(Semaphore::new(MAX_CONCURRENT_IMAGES)),
        }
    }
}

impl HttpClient for ImageHttpClient {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<http::Response<AsyncBody>>> {
        let client = self.client.clone();
        let handle = self.handle.clone();
        let requests = Arc::clone(&self.requests);
        Box::pin(async move {
            let (sender, receiver) = futures::channel::oneshot::channel();
            handle.spawn(async move {
                let result = async {
                    let _permit = requests
                        .acquire()
                        .await
                        .map_err(|_| anyhow::anyhow!("image concurrency limiter closed"))?;
                    let (parts, mut body) = request.into_parts();
                    let mut body_bytes = Vec::new();
                    body.read_to_end(&mut body_bytes).await?;

                    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())?;
                    let mut outgoing = client.request(method, parts.uri.to_string());
                    for (name, value) in parts.headers.iter() {
                        outgoing = outgoing.header(name.clone(), value.clone());
                    }

                    let response = outgoing.body(body_bytes).send().await?;
                    let status = response.status();
                    let headers = response.headers().clone();
                    let body = response.bytes().await?;

                    let mut builder = http::Response::builder().status(status.as_u16());
                    for (name, value) in headers.iter() {
                        builder = builder.header(name.clone(), value.clone());
                    }

                    Ok(builder.body(AsyncBody::from_bytes(body))?)
                }
                .await;
                let _ = sender.send(result);
            });

            receiver
                .await
                .map_err(|_| anyhow::anyhow!("image HTTP task cancelled"))?
        })
    }
}

const REFERER_HEADER: http::HeaderName = http::header::REFERER;
