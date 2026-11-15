use std::sync::Arc;

use gpui_kit::*;
use rigili_bilibili::{ApiError, Client};

enum ClientState {
    Ready(Arc<Client>),
    Failed(ApiError),
}

impl Global for ClientState {}

pub fn init(cx: &mut App) {
    let state = match Client::new() {
        Ok(client) => ClientState::Ready(Arc::new(client)),
        Err(error) => ClientState::Failed(error),
    };
    cx.set_global(state);
}

pub fn get(cx: &App) -> Result<Arc<Client>, &ApiError> {
    match cx.global::<ClientState>() {
        ClientState::Ready(client) => Ok(client.clone()),
        ClientState::Failed(error) => Err(error),
    }
}

pub fn cached(slot: &mut Option<Arc<Client>>, cx: &App) -> Result<Arc<Client>, String> {
    if let Some(client) = slot {
        return Ok(client.clone());
    }
    match get(cx) {
        Ok(client) => {
            *slot = Some(client.clone());
            Ok(client)
        }
        Err(error) => Err(crate::api_error_message(error)),
    }
}
