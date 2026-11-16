use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::slider::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use image::RgbaImage;
use rigili_core::cursor;
use rigili_player::{BufferedRange, PlaybackPhase, PlayerStatus, VideoFrame};

use super::{IDLE_POLL_INTERVAL, PAUSED_POLL_INTERVAL, POLL_INTERVAL, PlayerView};

impl PlayerView {
    pub(super) fn poll_interval(&self) -> Duration {
        poll_interval(self.status.phase, self.status.paused)
    }

    pub(super) fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entered = Instant::now();
        let interval = entered.saturating_duration_since(self.last_pump);
        self.last_pump = entered;
        if self.status.phase == PlaybackPhase::Playing
            && !self.status.paused
            && interval > POLL_INTERVAL + Duration::from_millis(14)
        {
            rigili_monitor::monitor!(
                "ui",
                "pump interval {interval:?} (active danmaku {})",
                self.danmaku_layer.read(cx).active_count()
            );
        }

        for frame in self.retired_frames.drain(..) {
            let _ = window.drop_image(frame);
        }

        self.sync_native_bounds(window, cx);
        let mut changed = self.pump_native(window);
        if notice_expired(self.notice.map(|(_, deadline)| deadline), Instant::now()) {
            self.notice = None;
            changed = true;
        }
        if let Some(frame) = self.player.take_frame()
            && let Some(image) = to_render_image(frame)
        {
            if let Some(previous) = self.frame.replace(image) {
                let _ = window.drop_image(previous);
            }
            changed = true;
        }

        let status = self.player.status();
        if !Arc::ptr_eq(&status, &self.status_source) {
            let next = (*status).clone();
            if next.duration > 0.0 && (next.duration - self.progress_max).abs() > 0.5 {
                self.rebuild_progress(next.duration, next.time_pos, window, cx);
            }
            if self.seeking.is_none() {
                self.progress.update(cx, |state, cx| {
                    state.set_value(next.time_pos as f32, window, cx);
                });
            }
            if !self.volume_dragging {
                self.volume.update(cx, |state, cx| {
                    state.set_value(next.volume as f32, window, cx);
                });
            }
            changed |= status_render_needed(&self.status, &next);
            self.status = next;
            self.status_source = status;
            self.play_anchor = Some((self.status.time_pos, Instant::now()));
        }

        self.danmaku_demand
            .set_playing(self.status.phase == PlaybackPhase::Playing && !self.status.paused);
        self.danmaku_demand.set_position(self.playback_time());

        if let Some(area) = self.video_bounds.get() {
            let time = self.playback_time();
            let scale = self.status.video_size.map_or(1.0, |size| {
                (f32::from(area.size.width) / size.width as f32)
                    .min(f32::from(area.size.height) / size.height as f32)
            });
            let started = Instant::now();
            self.update_danmaku(time, area, scale, window, cx);
            let elapsed = started.elapsed();
            if elapsed > Duration::from_millis(2) {
                rigili_monitor::monitor!(
                    "danmaku",
                    "update {elapsed:?} ({} active)",
                    self.danmaku_layer.read(cx).active_count()
                );
            }
        }

        if self.controls_idle() {
            self.controls_visible = false;
            self.preview_time = None;
            cursor::hide_until_mouse_moves(cx);
            changed = true;
        }

        self.sync_native_bounds(window, cx);

        if changed {
            cx.notify();
        }
    }

    fn rebuild_progress(
        &mut self,
        duration: f64,
        time_pos: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.progress_max = duration;
        let max = duration.max(1.0) as f32;
        let value = time_pos.clamp(0.0, duration) as f32;
        self.progress = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(max)
                .step(0.1)
                .default_value(value)
        });
        self._progress_sub = Some(cx.subscribe_in(
            &self.progress,
            window,
            |this, _, event: &SliderEvent, window, cx| this.on_progress(event, window, cx),
        ));
    }
}

fn notice_expired(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_some_and(|deadline| deadline <= now)
}

fn status_render_needed(previous: &PlayerStatus, next: &PlayerStatus) -> bool {
    previous.phase != next.phase
        || previous.paused != next.paused
        || previous.duration != next.duration
        || previous.volume != next.volume
        || previous.muted != next.muted
        || previous.speed != next.speed
        || buffered_changed(&previous.buffered, &next.buffered)
        || previous.video_size != next.video_size
        || previous.error != next.error
        || previous.time_pos.floor() != next.time_pos.floor()
}

fn buffered_changed(previous: &[BufferedRange], next: &[BufferedRange]) -> bool {
    previous.len() != next.len()
        || previous.iter().zip(next).any(|(previous, next)| {
            previous.start.floor() != next.start.floor() || previous.end.floor() != next.end.floor()
        })
}

fn poll_interval(phase: PlaybackPhase, paused: bool) -> Duration {
    match phase {
        PlaybackPhase::Idle | PlaybackPhase::Ended => IDLE_POLL_INTERVAL,
        _ if paused => PAUSED_POLL_INTERVAL,
        _ => POLL_INTERVAL,
    }
}

fn to_render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(frame.width, frame.height, frame.data)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        IDLE_POLL_INTERVAL, PAUSED_POLL_INTERVAL, POLL_INTERVAL, notice_expired, poll_interval,
        status_render_needed,
    };
    use rigili_player::{BufferedRange, PlaybackPhase, PlayerStatus};

    #[test]
    fn notices_expire_after_their_deadline() {
        let now = Instant::now();
        assert!(!notice_expired(None, now));
        assert!(!notice_expired(Some(now + Duration::from_secs(1)), now));
        assert!(notice_expired(Some(now), now));
    }

    #[test]
    fn polls_slowly_when_idle_or_paused() {
        assert_eq!(poll_interval(PlaybackPhase::Playing, false), POLL_INTERVAL);
        assert_eq!(
            poll_interval(PlaybackPhase::Buffering, false),
            POLL_INTERVAL
        );
        assert_eq!(
            poll_interval(PlaybackPhase::Playing, true),
            PAUSED_POLL_INTERVAL
        );
        assert_eq!(poll_interval(PlaybackPhase::Idle, true), IDLE_POLL_INTERVAL);
        assert_eq!(
            poll_interval(PlaybackPhase::Ended, false),
            IDLE_POLL_INTERVAL
        );
    }

    #[test]
    fn renders_only_when_the_visible_clock_or_state_changes() {
        let base = PlayerStatus::default();
        let mut sub_second = base.clone();
        sub_second.time_pos = 0.4;
        assert!(!status_render_needed(&base, &sub_second));

        let mut next_second = base.clone();
        next_second.time_pos = 1.2;
        assert!(status_render_needed(&base, &next_second));

        let mut paused = base.clone();
        paused.paused = true;
        assert!(status_render_needed(&base, &paused));

        let mut buffered = base.clone();
        buffered.buffered = vec![BufferedRange {
            start: 0.,
            end: 10.2,
        }];
        let mut buffered_later = base.clone();
        buffered_later.buffered = vec![BufferedRange {
            start: 0.,
            end: 10.8,
        }];
        assert!(!status_render_needed(&buffered, &buffered_later));
        buffered_later.buffered[0].end = 11.0;
        assert!(status_render_needed(&buffered, &buffered_later));
    }
}
