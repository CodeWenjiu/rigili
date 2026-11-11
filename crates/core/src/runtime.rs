use std::future::Future;

use gpui_kit::*;
use rigili_bilibili::ApiError;

pub enum ApiTaskError {
    Api(ApiError),
    Join(tokio::task::JoinError),
}

pub fn spawn_api<V, T, F, U>(cx: &mut Context<V>, future: F, on_done: U)
where
    V: 'static,
    T: Send + 'static,
    F: Future<Output = Result<T, ApiError>> + Send + 'static,
    U: FnOnce(&mut V, Result<T, ApiTaskError>, &mut Context<V>) + 'static,
{
    let task = Tokio::spawn(&*cx, future);
    cx.spawn(async move |weak, cx| {
        let result = match task.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(ApiTaskError::Api(error)),
            Err(error) => Err(ApiTaskError::Join(error)),
        };
        let _ = weak.update(cx, |this, cx| on_done(this, result, cx));
    })
    .detach();
}

pub fn init(cx: &mut App) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to initialize Tokio runtime");
    let handle = runtime.handle().clone();

    cx.set_global(GlobalTokio {
        owned_runtime: Some(runtime),
        handle,
    });
}

struct GlobalTokio {
    owned_runtime: Option<tokio::runtime::Runtime>,
    handle: tokio::runtime::Handle,
}

impl Global for GlobalTokio {}

impl Drop for GlobalTokio {
    fn drop(&mut self) {
        if let Some(runtime) = self.owned_runtime.take() {
            runtime.shutdown_background();
        }
    }
}

pub struct Tokio;

impl Tokio {
    pub fn spawn<C, Fut, R>(cx: &C, future: Fut) -> Task<Result<R, tokio::task::JoinError>>
    where
        C: AppContext,
        Fut: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        cx.read_global(|tokio: &GlobalTokio, cx| {
            let join_handle = tokio.handle.spawn(future);
            let abort = AbortOnDrop(join_handle.abort_handle());

            cx.background_spawn(async move {
                let result = join_handle.await;
                drop(abort);
                result
            })
        })
    }

    pub fn handle(cx: &App) -> tokio::runtime::Handle {
        cx.read_global(|tokio: &GlobalTokio, _| tokio.handle.clone())
    }
}

struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
