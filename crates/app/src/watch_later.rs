use std::collections::HashMap;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::runtime::Tokio;
use rigili_core::{api_error_message, backend, login};

#[derive(Default)]
struct WatchLaterState {
    user_mid: Option<u64>,
    items: HashMap<String, u64>,
    loading: bool,
    loaded: bool,
}

impl Global for WatchLaterState {}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(WatchLaterState::default());
}

pub(crate) fn contains(cx: &App, bvid: &str) -> bool {
    cx.global::<WatchLaterState>().items.contains_key(bvid)
}

pub(crate) fn ensure(window: &mut Window, cx: &mut App) {
    let user_mid = login::user(cx).map(|user| user.mid);
    let state = cx.global::<WatchLaterState>();

    if user_mid.is_none() {
        if state.user_mid.is_some() || state.loaded || !state.items.is_empty() {
            cx.set_global(WatchLaterState::default());
        }
        return;
    }

    if state.user_mid == user_mid && (state.loaded || state.loading) {
        return;
    }

    let state = cx.global_mut::<WatchLaterState>();
    state.user_mid = user_mid;
    state.items.clear();
    state.loading = true;
    state.loaded = false;

    window.defer(cx, |_, cx| load(cx));
}

fn load(cx: &mut App) {
    let Ok(backend) = backend::get(cx) else {
        let state = cx.global_mut::<WatchLaterState>();
        state.loading = false;
        state.loaded = true;
        return;
    };
    let request = Tokio::spawn(cx, async move { backend.watch_later().await });

    cx.spawn(async move |cx| {
        let result = request.await;
        cx.update(|cx| {
            let state = cx.global_mut::<WatchLaterState>();
            state.loading = false;
            state.loaded = true;
            if let Ok(Ok(page)) = result {
                state.items = page
                    .into_items()
                    .into_iter()
                    .filter(|item| !item.bvid.is_empty())
                    .map(|item| (item.bvid, item.aid))
                    .collect();
            }
            cx.refresh_windows();
        });
    })
    .detach();
}

pub(crate) fn toggle(bvid: SharedString, window: &mut Window, cx: &mut App) {
    let Ok(backend) = backend::get(cx) else {
        return;
    };
    let (in_watch_later, aid) = {
        let state = cx.global::<WatchLaterState>();
        (
            state.items.contains_key(bvid.as_ref()),
            state.items.get(bvid.as_ref()).copied().unwrap_or(0),
        )
    };

    {
        let state = cx.global_mut::<WatchLaterState>();
        if in_watch_later {
            state.items.remove(bvid.as_ref());
        } else {
            state.items.insert(bvid.to_string(), 0);
        }
    }
    cx.refresh_windows();

    let handle = window.window_handle();
    let bvid_key = bvid.to_string();
    let bvid_for_request = bvid_key.clone();
    let request = Tokio::spawn(cx, async move {
        if !in_watch_later {
            return backend.watch_later_add(&bvid_for_request).await;
        }

        let aid = if aid > 0 {
            aid
        } else {
            match backend.watch_later().await {
                Ok(page) => match page
                    .into_items()
                    .into_iter()
                    .find(|item| item.bvid == bvid_for_request)
                {
                    Some(item) => item.aid,
                    None => return Ok(()),
                },
                Err(error) => return Err(error),
            }
        };
        backend.watch_later_remove(aid).await
    });

    cx.spawn(async move |cx| {
        let result = request.await;
        let succeeded = matches!(&result, Ok(Ok(())));
        let expired = matches!(&result, Ok(Err(error)) if error.is_login_required());
        let note = match &result {
            Ok(Ok(())) => Notification::success(if in_watch_later {
                "已从稍后再看移除"
            } else {
                "已加入稍后再看"
            }),
            Ok(Err(error)) => Notification::error(api_error_message(error)),
            Err(_) => Notification::error(if in_watch_later {
                "移出稍后再看失败"
            } else {
                "加入稍后再看失败"
            }),
        };

        let _ = cx.update_window(handle, |_, window, cx| {
            if !succeeded {
                let state = cx.global_mut::<WatchLaterState>();
                if in_watch_later {
                    state.items.insert(bvid_key.clone(), aid);
                } else {
                    state.items.remove(&bvid_key);
                }
                cx.refresh_windows();
            }
            if expired {
                login::expire(cx);
            }
            window.push_notification(note, cx);
        });
    })
    .detach();
}
