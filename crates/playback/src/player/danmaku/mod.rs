rigili_macro::mod_prv!(state);
rigili_macro::mod_pub!(super, bar, feed);

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use gpui_kit::component::WindowExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Client, Danmaku};
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;

use crate::player::surface::PlayerView;
use crate::settings::DanmakuSettings;

use self::state::DanmakuState;

const DANMAKU_MAX_LEN: usize = 100;

pub(crate) struct DanmakuDemand {
    enabled: AtomicBool,
    playing: AtomicBool,
    position: AtomicU64,
}

impl DanmakuDemand {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            playing: AtomicBool::new(false),
            position: AtomicU64::new(0),
        }
    }

    pub(crate) fn should_load(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) && self.playing.load(Ordering::Relaxed)
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub(crate) fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }

    pub(crate) fn set_position(&self, time: f64) {
        self.position
            .store((time.max(0.) * 1000.) as u64, Ordering::Relaxed);
    }

    pub(crate) fn position(&self) -> f64 {
        self.position.load(Ordering::Relaxed) as f64 / 1000.
    }
}

#[derive(Clone)]
pub(super) struct DanmakuSource {
    client: Arc<Client>,
    aid: u64,
    bvid: String,
    cid: u64,
}

pub(super) struct DanmakuLayer {
    state: DanmakuState,
}

impl DanmakuLayer {
    pub(super) fn new(settings: DanmakuSettings, font_family: SharedString) -> Self {
        Self {
            state: DanmakuState::with_settings(settings, font_family),
        }
    }

    pub(super) fn update(
        &mut self,
        time: f64,
        area: Bounds<Pixels>,
        scale: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.settings().enabled {
            return;
        }
        if self.state.update(time, area, scale, window) {
            cx.notify();
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.state.clear();
        cx.notify();
    }

    pub(super) fn set_sorted(&mut self, items: Arc<Vec<Danmaku>>, cx: &mut Context<Self>) {
        self.state.set_sorted(items);
        cx.notify();
    }

    pub(super) fn seek(&mut self, time: f64, cx: &mut Context<Self>) {
        self.state.seek(time);
        cx.notify();
    }

    pub(super) fn settings(&self) -> DanmakuSettings {
        self.state.settings()
    }

    pub(super) fn active_count(&self) -> usize {
        self.state.active_count()
    }

    pub(super) fn set_settings(
        &mut self,
        settings: DanmakuSettings,
        time: f64,
        cx: &mut Context<Self>,
    ) {
        self.state.update_settings(settings, time);
        cx.notify();
    }
}

impl Render for DanmakuLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.state.settings().enabled {
            self.state.render().into_any_element()
        } else {
            div().absolute().size_full().into_any_element()
        }
    }
}

impl PlayerView {
    pub(crate) fn clear_danmaku(&mut self, cx: &mut Context<Self>) {
        self.danmaku_demand.set_position(0.);
        self.danmaku_layer.update(cx, |layer, cx| layer.clear(cx));
    }

    pub(crate) fn set_danmaku(&mut self, items: Arc<Vec<Danmaku>>, cx: &mut Context<Self>) {
        self.danmaku_layer
            .update(cx, |layer, cx| layer.set_sorted(items, cx));
    }

    pub(super) fn update_danmaku(
        &mut self,
        time: f64,
        area: Bounds<Pixels>,
        scale: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.danmaku_layer
            .update(cx, |layer, cx| layer.update(time, area, scale, window, cx));
    }

    pub(super) fn seek_danmaku(&mut self, time: f64, cx: &mut Context<Self>) {
        self.danmaku_demand.set_position(time);
        self.danmaku_layer
            .update(cx, |layer, cx| layer.seek(time, cx));
    }

    pub(crate) fn danmaku_demand(&self) -> Arc<DanmakuDemand> {
        self.danmaku_demand.clone()
    }

    pub(crate) fn danmaku_settings(&self, cx: &App) -> DanmakuSettings {
        self.danmaku_layer.read(cx).settings()
    }

    pub(crate) fn toggle_danmaku(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.danmaku_settings(cx);
        settings.enabled = !settings.enabled;
        self.apply_danmaku_settings(settings, cx);
    }

    pub(crate) fn apply_danmaku_settings(
        &mut self,
        settings: DanmakuSettings,
        cx: &mut Context<Self>,
    ) {
        let time = self.playback_time();
        self.danmaku_demand.set_enabled(settings.enabled);
        self.danmaku_layer
            .update(cx, |layer, cx| layer.set_settings(settings, time, cx));
        crate::settings::update(cx, |persisted| persisted.danmaku = settings);
        self.last_activity = Instant::now();
        cx.notify();
    }

    pub(crate) fn set_danmaku_source(
        &mut self,
        client: Arc<Client>,
        aid: u64,
        bvid: String,
        cid: u64,
        cx: &mut Context<Self>,
    ) {
        self.danmaku_source = Some(DanmakuSource {
            client,
            aid,
            bvid,
            cid,
        });
        self.danmaku_sending = false;
        cx.notify();
    }

    pub(crate) fn clear_danmaku_source(&mut self, cx: &mut Context<Self>) {
        if self.danmaku_source.take().is_some() {
            self.danmaku_sending = false;
            cx.notify();
        }
    }

    pub(crate) fn set_danmaku_settings_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.danmaku_settings_open != open {
            self.danmaku_settings_open = open;
            if open {
                self.last_activity = Instant::now();
            }
            cx.notify();
        }
    }

    pub(super) fn on_danmaku_input(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.submit_danmaku(window, cx),
            InputEvent::Focus => {
                self.danmaku_input_focused = true;
                self.last_activity = Instant::now();
                cx.notify();
            }
            InputEvent::Blur => {
                self.danmaku_input_focused = false;
                self.last_activity = Instant::now();
                cx.notify();
            }
            InputEvent::Change => cx.notify(),
        }
    }

    pub(crate) fn submit_danmaku(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.danmaku_sending {
            return;
        }
        let Some(source) = self.danmaku_source.clone() else {
            window.push_notification(Notification::error("当前视频暂不支持发送弹幕"), cx);
            return;
        };
        let content = self
            .danmaku_input
            .read(cx)
            .value()
            .trim()
            .replace(['\n', '\r'], " ");
        if content.is_empty() {
            return;
        }
        if content.chars().count() > DANMAKU_MAX_LEN {
            window.push_notification(
                Notification::error(format!("弹幕不能超过 {DANMAKU_MAX_LEN} 字")),
                cx,
            );
            return;
        }

        let progress = self.playback_time();
        let input = self.danmaku_input.clone();
        let window_handle = window.window_handle();
        let aid = source.aid;
        let cid = source.cid;
        self.danmaku_sending = true;
        cx.notify();

        spawn_api(
            cx,
            async move {
                source
                    .client
                    .post_danmaku(cid, &source.bvid, &content, progress)
                    .await
            },
            move |this, result, cx| {
                if !this
                    .danmaku_source
                    .as_ref()
                    .is_some_and(|current| current.cid == cid && current.aid == aid)
                {
                    return;
                }
                this.danmaku_sending = false;
                match result {
                    Ok(()) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            input.update(cx, |state, cx| state.set_value("", window, cx));
                            window.push_notification(Notification::success("弹幕已发送"), cx);
                        });
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.push_notification(Notification::error(message), cx);
                        });
                    }
                }
                cx.notify();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::DanmakuDemand;

    #[test]
    fn loads_only_while_playing_and_enabled() {
        let demand = DanmakuDemand::new(true);
        assert!(!demand.should_load());

        demand.set_playing(true);
        assert!(demand.should_load());

        demand.set_enabled(false);
        assert!(!demand.should_load());

        demand.set_enabled(true);
        demand.set_playing(false);
        assert!(!demand.should_load());
    }
}
