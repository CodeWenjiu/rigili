use std::{
    ffi::c_int,
    ops::ControlFlow,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use crate::{
    VideoSize,
    mpv::{Mpv, MpvError, RenderWake, Renderer, event},
};

use super::{
    handle::{Command, NativeSlot, RenderBackend},
    status::{PlaybackPhase, StatusSlot, VideoFrame},
    sync::{lock, mutate_status},
};
use crate::native::NativeFrame;

const EVENT_NONE: c_int = 0;
const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_LOG_MESSAGE: c_int = 2;
const MPV_EVENT_COMMAND_REPLY: c_int = 5;
const MPV_EVENT_START_FILE: c_int = 6;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;

const MPV_RENDER_UPDATE_FRAME: u64 = 1;
const AUDIO_ADD_TAG: u64 = 1 << 63;
const GENERATION_MASK: u64 = !AUDIO_ADD_TAG;

const STATUS_INTERVAL: Duration = Duration::from_millis(100);
const MONITOR_INTERVAL: Duration = Duration::from_secs(1);
const LOADING_WAIT: Duration = Duration::from_millis(10);
const IDLE_WAIT: Duration = Duration::from_millis(100);

// Readback copies and uploads every output pixel, so cap the software path
// before it reaches the renderer.
const MAX_OUTPUT_EDGE: u32 = 2560;

const PLAYBACK_CACHE: &[(&str, &str)] = &[
    ("cache-secs", "20"),
    ("demuxer-max-bytes", "32MiB"),
    ("demuxer-max-back-bytes", "8MiB"),
];

// Previews are short and always visible at once, so a large read-ahead only
// pins demuxer memory behind the home feed.
const PREVIEW_CACHE: &[(&str, &str)] = &[
    ("cache-secs", "8"),
    ("demuxer-max-bytes", "16MiB"),
    ("demuxer-max-back-bytes", "4MiB"),
];

pub(super) fn run_worker(
    receiver: mpsc::Receiver<Command>,
    backend: RenderBackend,
    wake: Arc<RenderWake>,
    frame_slot: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<StatusSlot>,
    native: Arc<NativeSlot>,
    fonts: Option<PathBuf>,
) {
    let mpv = match Mpv::new(
        match backend {
            RenderBackend::GpuPreferred => PLAYBACK_CACHE,
            RenderBackend::Software => PREVIEW_CACHE,
        },
        fonts.as_deref(),
    ) {
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
                MPV_EVENT_LOG_MESSAGE => {
                    if let Some(message) = event::log_message(&event) {
                        rigili_monitor::monitor!(
                            "mpv",
                            "[{}:{}] {}",
                            message.prefix,
                            message.level,
                            message.text
                        );
                    }
                }
                MPV_EVENT_START_FILE => {
                    state.current_entry = event::start_file_entry(&event);
                    rigili_monitor::monitor!("mpv", "start file ({:?})", state.current_entry);
                }
                MPV_EVENT_COMMAND_REPLY => {
                    if event.reply_userdata & AUDIO_ADD_TAG != 0
                        && event.reply_userdata & GENERATION_MASK == state.generation
                    {
                        let elapsed = state
                            .audio_requested
                            .take()
                            .map(|started| started.elapsed());
                        if event.error < 0 {
                            let error = MpvError::from_code(event.error);
                            rigili_monitor::monitor!(
                                "mpv",
                                "audio-add failed after {elapsed:?}: {error}"
                            );
                            mutate_status(&status, |status| {
                                status.error = Some(error.to_string());
                            });
                        } else {
                            rigili_monitor::monitor!("mpv", "audio added after {elapsed:?}");
                        }
                    }
                }
                MPV_EVENT_VIDEO_RECONFIG => {
                    state.needs_redraw = true;
                    if !state.first_frame_logged {
                        rigili_monitor::monitor!(
                            "mpv",
                            "video reconfigured after {:?}: size={:?}",
                            state.first_frame_at.map(|started| started.elapsed()),
                            mpv.video_size()
                        );
                    }
                }
                MPV_EVENT_FILE_LOADED => {
                    state.loading = false;
                    rigili_monitor::monitor!(
                        "mpv",
                        "file loaded after {:?}: size={:?} format={} hwdec={}",
                        state.load_started.map(|started| started.elapsed()),
                        mpv.video_size(),
                        mpv.get_property_string("video-format")
                            .unwrap_or_else(|| "none".to_owned()),
                        mpv.get_property_string("hwdec-current")
                            .unwrap_or_else(|| "none".to_owned())
                    );
                    state.load_started = None;
                    if let Some(audio) = state.pending_audio.take() {
                        match mpv.command_async(
                            &["audio-add", audio.as_str(), "select"],
                            AUDIO_ADD_TAG | state.generation,
                        ) {
                            Ok(()) => {
                                state.audio_requested = Some(Instant::now());
                                rigili_monitor::monitor!("mpv", "audio-add issued");
                            }
                            Err(error) => {
                                rigili_monitor::monitor!("mpv", "audio-add failed: {error}");
                                mutate_status(&status, |status| {
                                    status.error = Some(error.to_string());
                                });
                            }
                        }
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
                    if state.current_entry.is_some()
                        && state.current_entry == event::end_file_entry(&event)
                    {
                        state.current_entry = None;
                        state.loading = false;
                        state.load_started = None;
                        state.audio_requested = None;
                        state.first_frame_at = None;
                        state.native_frame = false;
                        let error = event::end_file_error(&event);
                        if error < 0 {
                            state.pending_audio.take();
                            state.pending_seek = None;
                            rigili_monitor::monitor!(
                                "mpv",
                                "end file: reason={}, error={}",
                                event::end_file_reason(&event),
                                MpvError::from_code(error)
                            );
                            mutate_status(&status, |status| {
                                status.error = Some(MpvError::from_code(error).to_string());
                            });
                        } else {
                            rigili_monitor::monitor!(
                                "mpv",
                                "end file: reason={}, error=ok",
                                event::end_file_reason(&event)
                            );
                        }
                    } else {
                        rigili_monitor::monitor!(
                            "mpv",
                            "end file: reason={}, replaced",
                            event::end_file_reason(&event)
                        );
                    }
                }
                _ => {}
            }
        }

        let update = render.update();
        let info = render.next_frame_info();
        // Once a frame exists, an OSD redraw can reuse it. Gating those on a new
        // video frame tied the overlay to the video's frame rate.
        let ready = ready_to_render(
            state.has_frame || state.native_frame,
            info.present,
            info.redraw,
        );
        if (state.needs_redraw || update & MPV_RENDER_UPDATE_FRAME != 0) && ready {
            let render_started = Instant::now();
            if native.enabled() {
                if let Some(size) = state.output_size(&mpv, None, true) {
                    if let Some(slot) = native.acquire() {
                        match state.render_native(&mut render, &native, slot, size) {
                            Ok(()) => state.native_frame = true,
                            Err(error) => {
                                eprintln!(
                                    "direct GPU output failed, falling back to readback: {error}"
                                );
                                native.fail();
                                state.has_frame = false;
                                state.needs_redraw = true;
                                push_frame(&mut state, &mpv, &mut render, &frame_slot);
                            }
                        }
                    }
                } else {
                    state.needs_redraw = true;
                }
            } else {
                push_frame(&mut state, &mpv, &mut render, &frame_slot);
            }
            state.log_first_frame(&mpv, render_started.elapsed());
        }

        if last_status.elapsed() >= STATUS_INTERVAL {
            update_status(&mpv, &status, state.loading);
            last_status = Instant::now();
            if rigili_monitor::enabled() {
                state.monitor(&mpv, &status);
            }
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
    load_started: Option<Instant>,
    last_phase: PlaybackPhase,
    last_monitor: Option<Instant>,
    generation: u64,
    audio_requested: Option<Instant>,
    current_entry: Option<i64>,
    first_frame_at: Option<Instant>,
    first_frame_logged: bool,
    native_frame: bool,
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
                rigili_monitor::monitor!(
                    "mpv",
                    "load {} (audio: {}, start: {start:?}, paused: {paused})",
                    rigili_monitor::host(&video),
                    audio.as_deref().map(rigili_monitor::host).unwrap_or("none")
                );
                self.load_started = Some(Instant::now());
                self.first_frame_at = Some(Instant::now());
                self.first_frame_logged = false;
                self.native_frame = false;
                self.last_monitor = None;
                self.current_entry = None;
                if self.audio_requested.take().is_some() {
                    mpv.abort_async_command(AUDIO_ADD_TAG | self.generation);
                }
                self.generation = self.generation.wrapping_add(1);
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
                let _ = mpv.set_osd_overlay(None);

                if let Err(error) = mpv.command(&["loadfile", video.as_str(), "replace"]) {
                    rigili_monitor::monitor!("mpv", "loadfile failed: {error}");
                    self.loading = false;
                    self.load_started = None;
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
                    if self.loading {
                        self.pending_pause = !paused;
                    }
                    mutate_status(status, |status| status.paused = !paused);
                }
            }
            Command::Stop => {
                self.generation = self.generation.wrapping_add(1);
                let _ = mpv.set_osd_overlay(None);
                self.pending_audio.take();
                self.pending_seek = None;
                self.pending_pause = false;
                self.loading = false;
                self.load_started = None;
                self.first_frame_at = None;
                self.first_frame_logged = false;
                self.current_entry = None;
                self.has_frame = false;
                self.native_frame = false;
                self.needs_redraw = false;
                lock(frame_slot).take();
                let stopped = mpv.command(&["stop"]);
                mutate_status(status, |status| {
                    status.phase = PlaybackPhase::Idle;
                    status.paused = false;
                    status.time_pos = 0.0;
                    status.duration = 0.0;
                    status.buffered.clear();
                    status.video_size = None;
                    status.error = stopped.err().map(|error| error.to_string());
                });
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
            Command::SetOsd(overlay) => {
                if let Err(error) = mpv.set_osd_overlay(overlay.as_ref()) {
                    rigili_monitor::monitor!("mpv", "cannot update the OSD overlay: {error}");
                }
                self.needs_redraw = true;
            }
            Command::Shutdown => return ControlFlow::Break(()),
        }

        ControlFlow::Continue(())
    }

    fn render(&mut self, mpv: &Mpv, render: &mut Renderer) -> Option<VideoFrame> {
        let size = self.output_size(mpv, Some(MAX_OUTPUT_EDGE), false)?;

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

    fn log_first_frame(&mut self, mpv: &Mpv, render_time: Duration) {
        if self.first_frame_logged || !(self.has_frame || self.native_frame) {
            return;
        }
        self.first_frame_logged = true;
        rigili_monitor::monitor!(
            "mpv",
            "first frame after {:?} (render {render_time:?}, hwdec={}, size={:?})",
            self.first_frame_at.take().map(|started| started.elapsed()),
            mpv.get_property_string("hwdec-current")
                .unwrap_or_else(|| "none".to_owned()),
            mpv.video_size()
        );
    }

    fn monitor(&mut self, mpv: &Mpv, status: &StatusSlot) {
        let (phase, time_pos, duration, buffered, video_size) = {
            let current = lock(status);
            (
                current.phase,
                current.time_pos,
                current.duration,
                current
                    .buffered
                    .last()
                    .map(|range| range.end - range.start)
                    .unwrap_or_default(),
                current.video_size,
            )
        };
        let loaded_for = self.load_started.map(|started| started.elapsed());

        if phase != self.last_phase {
            self.last_phase = phase;
            self.last_monitor = None;
            let cache = mpv.cache_state().unwrap_or_default();
            rigili_monitor::monitor!(
                "mpv",
                "phase {phase:?} at {time_pos:.2}s (loaded for {loaded_for:?}, cache={:.1}s, rate={:.0}KiB/s)",
                cache.duration,
                cache.raw_input_rate / 1024.
            );
            return;
        }

        if !matches!(
            phase,
            PlaybackPhase::Playing | PlaybackPhase::Loading | PlaybackPhase::Buffering
        ) || self
            .last_monitor
            .is_some_and(|last| last.elapsed() < MONITOR_INTERVAL)
        {
            return;
        }
        self.last_monitor = Some(Instant::now());

        let cache = mpv.cache_state().unwrap_or_default();
        rigili_monitor::monitor!(
            "mpv",
            "phase={phase:?} pos={time_pos:.2}s duration={duration:.2}s buffered={buffered:.1}s loaded_for={loaded_for:?} cache={:.1}s rate={:.0}KiB/s underrun={} cache-idle={} paused-for-cache={} eof={} size={video_size:?} canvas={:?} viewport={:?} hwdec={} drops={}",
            cache.duration,
            cache.raw_input_rate / 1024.,
            cache.underrun,
            cache.idle,
            mpv.get_property_flag("paused-for-cache")
                .unwrap_or_default(),
            mpv.get_property_flag("eof-reached").unwrap_or_default(),
            self.output_size(mpv, None, true),
            self.viewport,
            mpv.get_property_string("hwdec-current")
                .unwrap_or_else(|| "none".to_owned()),
            mpv.get_property_i64("frame-drop-count").unwrap_or_default()
        );
    }

    fn output_size(&self, mpv: &Mpv, max_edge: Option<u32>, upscale: bool) -> Option<VideoSize> {
        let source = mpv.video_size()?;
        let size = target_canvas(source, self.viewport, max_edge, upscale);
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

fn ready_to_render(rendered: bool, present: bool, redraw: bool) -> bool {
    rendered || (present && !redraw)
}

// The render target keeps the player's aspect, so mpv draws the letterbox
// bars into the exported frame and the ASS overlay (danmaku) may use them.
// The danmaku are rasterized at the target's resolution, so the native path
// renders at the display's resolution to keep them sharp; the readback path
// stops at the source size to keep the copies small.
fn target_canvas(
    source: VideoSize,
    viewport: VideoSize,
    max_edge: Option<u32>,
    upscale: bool,
) -> VideoSize {
    if source.width == 0 || source.height == 0 || viewport.width < 2 || viewport.height < 2 {
        return VideoSize::default();
    }

    let fit = (viewport.width as f64 / source.width as f64)
        .min(viewport.height as f64 / source.height as f64);
    let mut scale = if upscale { fit } else { fit.min(1.0) };
    if let Some(max_edge) = max_edge {
        scale = scale.min(max_edge as f64 / source.width.max(source.height) as f64);
    }

    let density = scale / fit;
    let mut width = viewport.width as f64 * density;
    let mut height = viewport.height as f64 * density;
    if let Some(max_edge) = max_edge {
        let long = width.max(height);
        if long > max_edge as f64 {
            let shrink = max_edge as f64 / long;
            width *= shrink;
            height *= shrink;
        }
    }

    VideoSize::new(
        (width.round() as u32).max(1),
        (height.round() as u32).max(1),
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
    let cache_started = Instant::now();
    next.buffered = mpv.buffered_ranges();
    let cache_took = cache_started.elapsed();
    if cache_took > Duration::from_millis(50) {
        rigili_monitor::monitor!("mpv", "cache state read blocked for {cache_took:?}");
    }
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
    fn osd_redraws_reuse_the_frame_after_the_first_one() {
        assert!(!ready_to_render(false, false, false));
        assert!(!ready_to_render(false, true, true));
        assert!(ready_to_render(false, true, false));
        assert!(ready_to_render(true, false, false));
        assert!(ready_to_render(true, true, true));
    }

    #[test]
    fn the_canvas_keeps_the_player_aspect() {
        let cases = [
            (size(1920, 1080), size(1280, 720), size(1280, 720)),
            (size(1920, 1080), size(1000, 1000), size(1000, 1000)),
            (size(1280, 720), size(1920, 1080), size(1280, 720)),
            (size(3840, 2160), size(3840, 2160), size(3840, 2160)),
            (size(2160, 3840), size(2160, 3840), size(2160, 3840)),
            (size(3840, 2160), size(1920, 1080), size(1920, 1080)),
            (size(3840, 2160), size(2880, 1800), size(2880, 1800)),
            (size(1920, 1080), size(0, 0), size(0, 0)),
            (size(0, 0), size(1280, 720), size(0, 0)),
        ];
        for (video, viewport, expected) in cases {
            assert_eq!(
                target_canvas(video, viewport, None, false),
                expected,
                "{video:?} in {viewport:?}"
            );
        }
    }

    #[test]
    fn native_output_upscales_to_the_viewport() {
        assert_eq!(
            target_canvas(size(1920, 1080), size(2285, 1801), None, true),
            size(2285, 1801)
        );
        assert_eq!(
            target_canvas(size(1920, 1080), size(2285, 1801), None, false),
            size(1920, 1513)
        );
    }

    #[test]
    fn readback_caps_the_long_edge() {
        assert_eq!(
            target_canvas(
                size(3840, 2160),
                size(3840, 2160),
                Some(MAX_OUTPUT_EDGE),
                false
            ),
            size(2560, 1440)
        );
        assert_eq!(
            target_canvas(
                size(2160, 3840),
                size(2160, 3840),
                Some(MAX_OUTPUT_EDGE),
                false
            ),
            size(1440, 2560)
        );
        assert_eq!(
            target_canvas(
                size(1920, 1080),
                size(1280, 720),
                Some(MAX_OUTPUT_EDGE),
                false
            ),
            size(1280, 720)
        );
        assert_eq!(
            target_canvas(
                size(1920, 1080),
                size(2000, 800),
                Some(MAX_OUTPUT_EDGE),
                false
            ),
            size(2000, 800)
        );
        assert_eq!(
            target_canvas(
                size(1920, 1080),
                size(3000, 1000),
                Some(MAX_OUTPUT_EDGE),
                false
            ),
            size(2560, 853)
        );
    }

    fn size(width: u32, height: u32) -> VideoSize {
        VideoSize::new(width, height)
    }
}
