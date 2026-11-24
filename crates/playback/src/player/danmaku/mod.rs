rigili_macro::mod_prv!(state);
rigili_macro::mod_pub!(super, bar, feed, live);

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::Icon;
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Client, Danmaku};
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;
use rigili_player::OsdOverlay;

use crate::player::surface::{OVERLAY_FOREGROUND, PlayerView};
use crate::settings::DanmakuSettings;

use self::state::DanmakuState;

const DANMAKU_MAX_LEN: usize = 100;
// The like affordance trails the text, so a danmaku close to the right edge
// would hang it outside the video.
const DANMAKU_LIKE_SPACE: f32 = 88.;
// A pointer sweeping across the video crosses many danmaku; only a pause long
// enough to read one deserves a stats request.
const DANMAKU_LIKE_DELAY: Duration = Duration::from_millis(300);

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

pub(super) struct DanmakuPin {
    dmid: u64,
    likes: Option<u32>,
    liked: bool,
    pending: bool,
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

    pub(super) fn update(&mut self, time: f64, width: f32, height: f32) -> bool {
        if !self.state.settings().enabled {
            return false;
        }
        self.state.update(time, width, height)
    }

    pub(super) fn ass(&self, device_scale: f32) -> String {
        self.state.ass(device_scale)
    }

    pub(super) fn clear(&mut self) {
        self.state.clear();
    }

    pub(super) fn set_sorted(&mut self, items: Arc<Vec<Danmaku>>) {
        self.state.set_sorted(items);
    }

    pub(super) fn seek(&mut self, time: f64) {
        self.state.seek(time);
    }

    pub(super) fn settings(&self) -> DanmakuSettings {
        self.state.settings()
    }

    pub(super) fn active_count(&self) -> usize {
        self.state.active_count()
    }

    pub(super) fn set_settings(&mut self, settings: DanmakuSettings, time: f64) {
        self.state.update_settings(settings, time);
    }

    fn pin_at(&mut self, x: f32, y: f32, time: f64, hold: bool) -> bool {
        self.state.pin_at(x, y, time, hold)
    }

    fn unpin(&mut self, time: f64) -> bool {
        self.state.unpin(time)
    }

    fn pinned(&self) -> Option<state::Pinned<'_>> {
        self.state.pinned()
    }
}

impl PlayerView {
    pub(crate) fn clear_danmaku(&mut self, cx: &mut Context<Self>) {
        self.danmaku_demand.set_position(0.);
        self.danmaku_layer.clear();
        self.set_danmaku_overlay(None);
        cx.notify();
    }

    pub(crate) fn set_danmaku(&mut self, items: Arc<Vec<Danmaku>>, cx: &mut Context<Self>) {
        self.danmaku_layer.set_sorted(items);
        cx.notify();
    }

    pub(super) fn update_danmaku(&mut self, time: f64, width: f32, height: f32, device_scale: f32) {
        if self.danmaku_layer.update(time, width, height) {
            self.set_danmaku_overlay(Some(OsdOverlay {
                data: self.danmaku_layer.ass(device_scale),
                width: (width * device_scale).round() as u32,
                height: (height * device_scale).round() as u32,
            }));
        }
    }

    pub(super) fn seek_danmaku(&mut self, time: f64, cx: &mut Context<Self>) {
        self.danmaku_demand.set_position(time);
        self.danmaku_layer.seek(time);
        cx.notify();
    }

    pub(crate) fn danmaku_demand(&self) -> Arc<DanmakuDemand> {
        self.danmaku_demand.clone()
    }

    pub(crate) fn danmaku_settings(&self) -> DanmakuSettings {
        self.danmaku_layer.settings()
    }

    pub(crate) fn toggle_danmaku(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.danmaku_settings();
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
        self.danmaku_layer.set_settings(settings, time);
        if !settings.enabled {
            self.set_danmaku_overlay(None);
        }
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
            toast::push(window, cx, Notification::error("当前视频暂不支持发送弹幕"));
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
            toast::push(
                window,
                cx,
                Notification::error(format!("弹幕不能超过 {DANMAKU_MAX_LEN} 字")),
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
                            toast::push(window, cx, Notification::success("弹幕已发送"));
                        });
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        let _ = window_handle.update(cx, |_, window, cx| {
                            toast::push(window, cx, Notification::error(message));
                        });
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn hover_danmaku(
        &mut self,
        position: Point<Pixels>,
        held: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.danmaku_layer.settings().enabled {
            return;
        }
        let Some(bounds) = self.video_bounds.get() else {
            return;
        };
        let x = f32::from(position.x - bounds.left());
        let y = f32::from(position.y - bounds.top());
        if self.danmaku_layer.pin_at(x, y, self.playback_time(), held) {
            self.pump_danmaku(window);
            cx.notify();
        }
    }

    pub(super) fn leave_danmaku(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.danmaku_layer.unpin(self.playback_time()) {
            self.pump_danmaku(window);
            cx.notify();
        }
    }

    fn danmaku_like_fits(&self, pin: &state::Pinned<'_>) -> bool {
        self.video_bounds.get().is_some_and(|bounds| {
            pin.position.0 + pin.width + DANMAKU_LIKE_SPACE <= f32::from(bounds.size.width)
        })
    }

    /// Keeps the like state in step with whatever the pointer is hovering.
    pub(super) fn sync_danmaku_like(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pin = self.danmaku_layer.pinned();
        let pinned = pin
            .as_ref()
            .filter(|pin| pin.id != 0 && self.danmaku_like_fits(pin))
            .map(|pin| pin.id);
        if pinned == self.danmaku_pin.as_ref().map(|pin| pin.dmid) {
            return;
        }
        self.danmaku_pin = pinned.map(|dmid| DanmakuPin {
            dmid,
            likes: None,
            liked: false,
            pending: false,
        });
        cx.notify();

        let (Some(dmid), Some(source)) = (pinned, self.danmaku_source.clone()) else {
            return;
        };
        let cid = source.cid;
        let generation = self.danmaku_like_generation.wrapping_add(1);
        self.danmaku_like_generation = generation;
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(DANMAKU_LIKE_DELAY).await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.danmaku_like_generation != generation {
                    return;
                }
                let client = source.client.clone();
                spawn_api(
                    cx,
                    async move { client.danmaku_like_stats(cid, dmid).await },
                    move |this, result, cx| {
                        if this.danmaku_like_generation != generation {
                            return;
                        }
                        let Ok(Some(stats)) = result else {
                            return;
                        };
                        let Some(pin) = this.danmaku_pin.as_mut() else {
                            return;
                        };
                        if pin.dmid != dmid {
                            return;
                        }
                        pin.likes = Some(stats.likes);
                        pin.liked = stats.liked;
                        cx.notify();
                    },
                );
            });
        });
        task.detach();
    }

    pub(super) fn like_danmaku(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.danmaku_source.clone() else {
            return;
        };
        let Some(pin) = self.danmaku_pin.as_mut() else {
            return;
        };
        if pin.pending {
            return;
        }
        let dmid = pin.dmid;
        let liked = !pin.liked;
        let previous = pin.likes;
        pin.liked = liked;
        pin.likes = pin.likes.map(|likes| {
            if liked {
                likes.saturating_add(1)
            } else {
                likes.saturating_sub(1)
            }
        });
        pin.pending = true;
        // A like answers the same question as the stats request, so drop any
        // response that was already in flight.
        self.danmaku_like_generation = self.danmaku_like_generation.wrapping_add(1);
        cx.notify();

        let cid = source.cid;
        let window_handle = window.window_handle();
        spawn_api(
            cx,
            async move { source.client.danmaku_thumbup(cid, dmid, liked).await },
            move |this, result, cx| {
                let Some(pin) = this.danmaku_pin.as_mut().filter(|pin| pin.dmid == dmid) else {
                    return;
                };
                pin.pending = false;
                if let Err(error) = result {
                    pin.liked = !liked;
                    pin.likes = previous;
                    let message = api_task_error_message(error, cx);
                    let _ = window_handle.update(cx, |_, window, cx| {
                        toast::push(window, cx, Notification::error(message));
                    });
                }
                cx.notify();
            },
        );
    }

    pub(super) fn pinned_danmaku(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pin = self.danmaku_layer.pinned()?;
        let style = TextStyleRefinement {
            font_family: Some(pin.font_family.clone()),
            font_size: Some(px(pin.font_size).into()),
            line_height: Some(px(rigili_core::fonts::baseline_line_height(pin.font_size)).into()),
            color: Some(rgb(pin.color).into()),
            white_space: Some(WhiteSpace::Nowrap),
            ..Default::default()
        };
        let (left, top) = pin.position;
        let id = ElementId::NamedInteger("danmaku-pinned".into(), pin.id);
        let mut row = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .flex()
            .items_center()
            .gap_1()
            .font_family(pin.font_family.clone())
            .child(
                div()
                    .cursor(CursorStyle::IBeam)
                    .child(SelectableText::new(id, pin.text.to_owned()).text_style(style)),
            );

        if let Some(like) = self.danmaku_pin.as_ref().filter(|like| like.dmid == pin.id) {
            let color = if like.liked {
                cx.theme().accent
            } else {
                OVERLAY_FOREGROUND
            };
            row = row.child(
                div()
                    .id("danmaku-like")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .line_height(relative(1.))
                    .cursor_pointer()
                    .text_xs()
                    .text_color(color)
                    .on_click(cx.listener(|this, _, window, cx| this.like_danmaku(window, cx)))
                    .child(
                        Icon::new(IconName::ThumbsUp)
                            .size(px(16.))
                            .text_color(color),
                    )
                    .when_some(like.likes, |this, likes| this.child(likes.to_string())),
            );
        }

        Some(
            div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .child(row)
                .into_any_element(),
        )
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
