use std::{
    ffi::c_int,
    ops::ControlFlow,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use crate::{
    VideoSize,
    mpv::{Mpv, MpvError, RenderWake, Renderer},
};

use super::{
    handle::{Command, NativeSlot, RenderBackend},
    status::{PlaybackPhase, StatusSlot, VideoFrame},
    sync::{lock, mutate_status},
};
use crate::native::NativeFrame;

const EVENT_NONE: c_int = 0;
const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;

const MPV_RENDER_UPDATE_FRAME: u64 = 1;

const STATUS_INTERVAL: Duration = Duration::from_millis(100);
const LOADING_WAIT: Duration = Duration::from_millis(10);
const IDLE_WAIT: Duration = Duration::from_millis(100);

// Readback copies and uploads every output pixel, so cap the software path
// before it reaches the renderer.
const MAX_OUTPUT_EDGE: u32 = 2560;

pub(super) fn run_worker(
    receiver: mpsc::Receiver<Command>,
    backend: RenderBackend,
    wake: Arc<RenderWake>,
    frame_slot: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<StatusSlot>,
    native: Arc<NativeSlot>,
) {
    let mpv = match Mpv::new() {
        Ok(mpv) => mpv,
        Err(error) => {
            mutate_status(&status, |status| {
                status.error = Some(error.to_string());
            });
            return;
        }
    };
    let render = match backend {
        RenderBackend::GpuPreferred => Renderer::new(&mpv, wake.clone()),
        RenderBackend::Software => Renderer::new_software(&mpv, wake.clone()),
    };
    let mut render = match render {
        Ok(render) => render,
        Err(error) => {
            mutate_status(&status, |status| {
                status.error = Some(error.to_string());
            });
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

        loop {
            let event = mpv.wait_event(0.0);
            if event.event_id == EVENT_NONE {
                break;
            }
            match event.event_id {
                MPV_EVENT_SHUTDOWN => return,
                MPV_EVENT_VIDEO_RECONFIG => state.needs_redraw = true,
                MPV_EVENT_FILE_LOADED => {
                    state.loading = false;
                    if let Some(audio) = state.pending_audio.take()
                        && let Err(error) = mpv.command(&["audio-add", audio.as_str(), "select"])
                    {
                        mutate_status(&status, |status| {
                            status.error = Some(error.to_string());
                        });
                    }
                    let _ = mpv.set_property_flag("pause", state.pending_pause);
                    if let Some(seek) = state.pending_seek.take() {
                        let target = format!("{seek:.3}");
                        if let Err(error) = mpv.command(&["seek", &target, "absolute+exact"]) {
                            mutate_status(&status, |status| {
                                status.error = Some(error.to_string());
                            });
                        }
                    }
                    state.needs_redraw = true;
                }
                MPV_EVENT_END_FILE => {
                    state.loading = false;
                    if event.error < 0 {
                        state.pending_audio.take();
                        state.pending_seek = None;
                        mutate_status(&status, |status| {
                            status.error = Some(MpvError::from_code(event.error).to_string());
                        });
                    }
                }
                _ => {}
            }
        }

        let update = render.update();
        let info = render.next_frame_info();
        let ready = if state.has_frame {
            info.present
        } else {
            info.present && !info.redraw
        };
        if (state.needs_redraw || update & MPV_RENDER_UPDATE_FRAME != 0) && ready {
            if native.enabled() {
                if let Some(size) = state.output_size(&mpv, None) {
                    if let Some(slot) = native.acquire()
                        && let Err(error) = state.render_native(&mut render, &native, slot, size)
                    {
                        eprintln!("direct GPU output failed, falling back to readback: {error}");
                        native.fail();
                        state.has_frame = false;
                        state.needs_redraw = true;
                        push_frame(&mut state, &mpv, &mut render, &frame_slot);
                    }
                } else {
                    state.needs_redraw = true;
                }
            } else {
                push_frame(&mut state, &mpv, &mut render, &frame_slot);
            }
        }

        if last_status.elapsed() >= STATUS_INTERVAL {
            update_status(&mpv, &status, state.loading);
            last_status = Instant::now();
        }

        wake.wait(if state.loading {
            LOADING_WAIT
        } else {
            IDLE_WAIT
        });
    }
}

#[derive(Default)]
struct State {
    viewport: VideoSize,
    pending_audio: Option<String>,
    pending_seek: Option<f64>,
    pending_pause: bool,
    buffer: Vec<u8>,
    needs_redraw: bool,
    loading: bool,
    has_frame: bool,
}

impl State {
    fn apply(
        &mut self,
        command: Command,
        mpv: &Mpv,
        frame_slot: &Mutex<Option<VideoFrame>>,
        status: &StatusSlot,
    ) -> ControlFlow<()> {
        match command {
            Command::Load {
                video,
                audio,
                start,
                paused,
            } => {
                let switching = start.is_some();
                self.pending_audio = audio;
                self.pending_seek = start;
                self.pending_pause = paused;
                self.loading = true;
                self.has_frame = false;
                mutate_status(status, |status| {
                    status.phase = PlaybackPhase::Loading;
                    if !switching {
                        status.time_pos = 0.0;
                        status.duration = 0.0;
                        status.buffered.clear();
                    }
                    status.error = None;
                });
                lock(frame_slot).take();

                if let Err(error) = mpv.command(&["loadfile", video.as_str(), "replace"]) {
                    self.loading = false;
                    self.pending_audio.take();
                    self.pending_seek = None;
                    self.pending_pause = false;
                    mutate_status(status, |status| {
                        status.phase = PlaybackPhase::Idle;
                        status.error = Some(error.to_string());
                    });
                    return ControlFlow::Continue(());
                }
                self.needs_redraw = true;
            }
            Command::TogglePause => {
                let paused = mpv.get_property_flag("pause").unwrap_or(false);
                if let Err(error) = mpv.set_property_flag("pause", !paused) {
                    mutate_status(status, |status| {
                        status.error = Some(error.to_string());
                    });
                } else {
                    mutate_status(status, |status| status.paused = !paused);
                }
            }
            Command::Seek(seconds) => {
                let target = format!("{seconds:.3}");
                if let Err(error) = mpv.command(&["seek", &target, "absolute+exact"]) {
                    mutate_status(status, |status| {
                        status.error = Some(error.to_string());
                    });
                }
            }
            Command::SetVolume(volume) => {
                if let Err(error) = mpv.set_property_double("volume", volume) {
                    mutate_status(status, |status| {
                        status.error = Some(error.to_string());
                    });
                } else {
                    mutate_status(status, |status| status.volume = volume);
                }
            }
            Command::SetMute(muted) => {
                if let Err(error) = mpv.set_property_flag("mute", muted) {
                    mutate_status(status, |status| {
                        status.error = Some(error.to_string());
                    });
                } else {
                    mutate_status(status, |status| status.muted = muted);
                }
            }
            Command::SetSpeed(speed) => {
                if let Err(error) = mpv.set_property_double("speed", speed) {
                    mutate_status(status, |status| {
                        status.error = Some(error.to_string());
                    });
                } else {
                    mutate_status(status, |status| status.speed = speed);
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

    fn render(&mut self, mpv: &Mpv, render: &mut Renderer) -> Option<VideoFrame> {
        let size = self.output_size(mpv, Some(MAX_OUTPUT_EDGE))?;

        let stride = size.width * 4;
        let length = stride as usize * size.height as usize;
        self.buffer.clear();
        if self.buffer.capacity() < length {
            self.buffer = Vec::with_capacity(length);
        }
        let pointer = self.buffer.spare_capacity_mut().as_mut_ptr().cast::<u8>();

        render
            .render(size.width, size.height, stride, pointer)
            .ok()?;

        // SAFETY: the renderer writes the colour bytes of every pixel and fills
        // the padding byte, so the whole region is initialized before the length
        // is exposed.
        unsafe {
            self.buffer.set_len(length);
        }

        self.needs_redraw = false;
        Some(VideoFrame {
            width: size.width,
            height: size.height,
            data: std::mem::take(&mut self.buffer),
        })
    }

    fn render_native(
        &mut self,
        render: &mut Renderer,
        native: &NativeSlot,
        slot: usize,
        size: VideoSize,
    ) -> Result<(), MpvError> {
        render.render_native(slot, size.width, size.height)?;
        native.publish(NativeFrame {
            slot: slot as u8,
            width: size.width,
            height: size.height,
            dmabuf: render.dmabuf(slot)?,
        });
        self.needs_redraw = false;
        Ok(())
    }

    fn output_size(&self, mpv: &Mpv, max_edge: Option<u32>) -> Option<VideoSize> {
        let source = mpv.video_size()?;
        let size = target_dimensions(source, self.viewport, max_edge);
        (size.width > 0 && size.height > 0).then_some(size)
    }
}

fn push_frame(
    state: &mut State,
    mpv: &Mpv,
    render: &mut Renderer,
    frame_slot: &Mutex<Option<VideoFrame>>,
) {
    if let Some(frame) = state.render(mpv, render) {
        state.has_frame = true;
        *lock(frame_slot) = Some(frame);
    }
}

fn target_dimensions(source: VideoSize, viewport: VideoSize, max_edge: Option<u32>) -> VideoSize {
    if source.width == 0 || source.height == 0 || viewport.width < 2 || viewport.height < 2 {
        return VideoSize::default();
    }

    let mut scale = (viewport.width as f64 / source.width as f64)
        .min(viewport.height as f64 / source.height as f64);
    if let Some(max_edge) = max_edge {
        scale = scale.min(max_edge as f64 / source.width.max(source.height) as f64);
    }
    let scale = scale.min(1.0);
    VideoSize::new(
        ((source.width as f64 * scale).round() as u32).max(1),
        ((source.height as f64 * scale).round() as u32).max(1),
    )
}

fn update_status(mpv: &Mpv, status: &StatusSlot, loading: bool) {
    let mut next = (**lock(status)).clone();
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
    next.buffered = mpv.buffered_ranges();
    next.volume = mpv.get_property_double("volume").unwrap_or(next.volume);
    next.muted = mpv.get_property_flag("mute").unwrap_or(next.muted);
    next.speed = mpv.get_property_double("speed").unwrap_or(next.speed);
    next.hwdec = mpv.get_property_string("hwdec-current");
    next.audio_codec = mpv.get_property_string("audio-codec-name");
    next.video_size = mpv.video_size();

    let mut current = lock(status);
    if **current != next {
        *current = Arc::new(next);
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
            (size(3840, 2160), size(3840, 2160), size(3840, 2160)),
            (size(2160, 3840), size(2160, 3840), size(2160, 3840)),
            (size(3840, 2160), size(1920, 1080), size(1920, 1080)),
            (size(3840, 2160), size(2880, 1800), size(2880, 1620)),
            (size(1920, 1080), size(0, 0), size(0, 0)),
            (size(0, 0), size(1280, 720), size(0, 0)),
        ];
        for (video, viewport, expected) in cases {
            assert_eq!(
                target_dimensions(video, viewport, None),
                expected,
                "{video:?} in {viewport:?}"
            );
        }
    }

    #[test]
    fn readback_caps_the_long_edge() {
        assert_eq!(
            target_dimensions(size(3840, 2160), size(3840, 2160), Some(MAX_OUTPUT_EDGE)),
            size(2560, 1440)
        );
        assert_eq!(
            target_dimensions(size(2160, 3840), size(2160, 3840), Some(MAX_OUTPUT_EDGE)),
            size(1440, 2560)
        );
        assert_eq!(
            target_dimensions(size(1920, 1080), size(1280, 720), Some(MAX_OUTPUT_EDGE)),
            size(1280, 720)
        );
    }

    fn size(width: u32, height: u32) -> VideoSize {
        VideoSize::new(width, height)
    }
}
