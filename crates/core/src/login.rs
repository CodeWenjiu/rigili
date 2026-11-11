use gpui_kit::*;
use rigili_bilibili::Nav;

#[derive(Default)]
pub struct LoginState {
    user: Option<Nav>,
}

impl Global for LoginState {}

pub fn user(cx: &App) -> Option<Nav> {
    cx.global::<LoginState>().user.clone()
}

pub fn set_user(user: Option<Nav>, cx: &mut App) {
    cx.set_global(LoginState { user });
    cx.refresh_windows();
}

pub fn expire(cx: &mut App) {
    if user(cx).is_some() {
        set_user(None, cx);
    }
}
