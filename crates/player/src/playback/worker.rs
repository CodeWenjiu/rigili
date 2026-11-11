use std::{
    ffi::c_int,
    ops::ControlFlow,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use crate::{
    VideoSize,
    mpv::{Mpv, MpvError, RenderContext},
};

use super::{
    handle::Command,
    status::{PlaybackPhase, PlayerStatus, VideoFrame},
    sync::lock,
};

const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;

const MPV_RENDER_UPDATE_FRAME: u64 = 1;

const EVENT_TIMEOUT: f64 = 0.005;
const STATUS_INTERVAL: Duration = Duration::from_millis(100);

pub(super) fn run_worker(
    receiver: mpsc::Receiver<Command>,
    frame_slot: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<Mutex<PlayerStatus>>,
) {
    let mpv = match Mpv::new() {
        Ok(mpv) => mpv,
        Err(error) => {
            lock(&status).error = Some(error.to_string());
            return;
        }
    };
    let render = match RenderContext::new(&mpv) {
        Ok(render) => render,
        Err(error) => {
            lock(&status).error = Some(error.to_string());
            return;
        }
    };

    let mut state = State::default();
    let mut last_status = Instant::now();

    loop {
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if state.apply(command, &mpv, &frame_slot, &status).is_break() {
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        let event = mpv.wait_event(EVENT_TIMEOUT);
        match event.event_id {
            MPV_EVENT_SHUTDOWN => return,
            MPV_EVENT_VIDEO_RECONFIG => state.needs_redraw = true,
            MPV_EVENT_FILE_LOADED => {
                state.loading = false;
                if let Some(audio) = state.pending_audio.take()
                    && let Err(error) = mpv.command(&["audio-add", audio.as_str(), "select"])
                {
                    lock(&status).error = Some(error.to_string());
                }
                state.needs_redraw = true;
            }
            MPV_EVENT_END_FILE => {
                state.loading = false;
                if event.error < 0 {
                    state.pending_audio.take();
                    lock(&status).error = Some(MpvError::from_code(event.error).to_string());
                }
            }
            _ => {}
        }

        let update = render.update();
        if (state.needs_redraw || update & MPV_RENDER_UPDATE_FRAME != 0)
            && let Some(frame) = state.render(&mpv, &render)
        {
            *lock(&frame_slot) = Some(frame);
        }

        if last_status.elapsed() >= STATUS_INTERVAL {
            update_status(&mpv, &status, state.loading);
            last_status = Instant::now();
        }
    }
}

#[derive(Default)]
struct State {
    viewport: VideoSize,
    pending_audio: Option<String>,
    buffer: Vec<u8>,
    needs_redraw: bool,
    loading: bool,
}

impl State {
    fn apply(
        &mut self,
        command: Command,
        mpv: &Mpv,
        frame_slot: &Mutex<Option<VideoFrame>>,
        status: &Mutex<PlayerStatus>,
    ) -> ControlFlow<()> {
        match command {
            Command::Load { video, audio } => {
                self.pending_audio = audio;
                self.loading = true;
                {
                    let mut status = lock(status);
                    status.phase = PlaybackPhase::Loading;
                    status.time_pos = 0.0;
                    status.duration = 0.0;
                    status.error = None;
                }
                lock(frame_slot).take();

                if let Err(error) = mpv.command(&["loadfile", video.as_str(), "replace"]) {
                    self.loading = false;
                    let mut status = lock(status);
                    status.phase = PlaybackPhase::Idle;
                    status.error = Some(error.to_string());
                    return ControlFlow::Continue(());
                }
                let _ = mpv.set_property_flag("pause", false);
                self.needs_redraw = true;
            }
            Command::TogglePause => {
                let paused = mpv.get_property_flag("pause").unwrap_or(false);
                if let Err(error) = mpv.set_property_flag("pause", !paused) {
                    lock(status).error = Some(error.to_string());
                } else {
                    lock(status).paused = !paused;
                }
            }
            Command::Seek(seconds) => {
                let target = format!("{seconds:.3}");
                if let Err(error) = mpv.command(&["seek", &target, "absolute+exact"]) {
                    lock(status).error = Some(error.to_string());
                }
            }
            Command::SetVolume(volume) => {
                if let Err(error) = mpv.set_property_double("volume", volume) {
                    lock(status).error = Some(error.to_string());
                } else {
                    lock(status).volume = volume;
                }
            }
            Command::SetSpeed(speed) => {
                if let Err(error) = mpv.set_property_double("speed", speed) {
                    lock(status).error = Some(error.to_string());
                } else {
                    lock(status).speed = speed;
                }
            }
            Command::SetViewport(size) => {
                if self.viewport != size {
                    self.viewport = size;
                    self.needs_redraw = true;
                }
            }
            Command::Shutdown => return ControlFlow::Break(()),
        }

        ControlFlow::Continue(())
    }

    fn render(&mut self, mpv: &Mpv, render: &RenderContext) -> Option<VideoFrame> {
        let source = mpv.video_size()?;
        let size = target_dimensions(source, self.viewport);
        if size.width == 0 || size.height == 0 {
            return None;
        }

        let stride = size.width * 4;
        let length = stride as usize * size.height as usize;
        if self.buffer.len() != length {
            self.buffer.resize(length, 0);
        }

        render
            .render(size.width, size.height, stride, &mut self.buffer)
            .ok()?;
        for pixel in self.buffer.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }

        self.needs_redraw = false;
        Some(VideoFrame {
            width: size.width,
            height: size.height,
            data: std::mem::take(&mut self.buffer),
        })
    }
}

fn target_dimensions(source: VideoSize, viewport: VideoSize) -> VideoSize {
    if source.width == 0 || source.height == 0 || viewport.width < 2 || viewport.height < 2 {
        return VideoSize::default();
    }

    let scale = (viewport.width as f64 / source.width as f64)
        .min(viewport.height as f64 / source.height as f64)
        .min(1.0);
    VideoSize::new(
        ((source.width as f64 * scale).round() as u32).max(1),
        ((source.height as f64 * scale).round() as u32).max(1),
    )
}

fn update_status(mpv: &Mpv, status: &Mutex<PlayerStatus>, loading: bool) {
    let mut next = lock(status).clone();
    let idle = mpv.get_property_flag("idle-active").unwrap_or(false);
    let buffering = mpv.get_property_flag("paused-for-cache").unwrap_or(false);
    let ended = mpv.get_property_flag("eof-reached").unwrap_or(false);

    next.phase = if loading {
        PlaybackPhase::Loading
    } else if buffering {
        PlaybackPhase::Buffering
    } else if ended {
        PlaybackPhase::Ended
    } else if idle {
        PlaybackPhase::Idle
    } else {
        PlaybackPhase::Playing
    };
    next.paused = mpv.get_property_flag("pause").unwrap_or(next.paused);
    next.time_pos = mpv.get_property_double("time-pos").unwrap_or(next.time_pos);
    next.duration = mpv.get_property_double("duration").unwrap_or(next.duration);
    next.volume = mpv.get_property_double("volume").unwrap_or(next.volume);
    next.speed = mpv.get_property_double("speed").unwrap_or(next.speed);
    next.hwdec = mpv.get_property_string("hwdec-current");
    next.audio_codec = mpv.get_property_string("audio-codec-name");
    next.video_size = mpv.video_size();

    let mut current = lock(status);
    if *current != next {
        *current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dimensions_fit_the_viewport() {
        let cases = [
            (size(1920, 1080), size(1280, 720), size(1280, 720)),
            (size(1920, 1080), size(1000, 1000), size(1000, 563)),
            (size(1280, 720), size(1920, 1080), size(1280, 720)),
            (size(1920, 1080), size(0, 0), size(0, 0)),
            (size(0, 0), size(1280, 720), size(0, 0)),
        ];
        for (video, viewport, expected) in cases {
            assert_eq!(
                target_dimensions(video, viewport),
                expected,
                "{video:?} in {viewport:?}"
            );
        }
    }

    fn size(width: u32, height: u32) -> VideoSize {
        VideoSize::new(width, height)
    }
}
