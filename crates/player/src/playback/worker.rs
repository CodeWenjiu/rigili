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
    status::{PlayerStatus, VideoFrame},
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

    let mut viewport = VideoSize::default();
    let mut pending_audio = None;
    let mut buffer = Vec::new();
    let mut needs_redraw = true;
    let mut last_status = Instant::now();

    loop {
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if apply_command(
                        command,
                        &mpv,
                        &mut viewport,
                        &mut needs_redraw,
                        &frame_slot,
                        &status,
                        &mut pending_audio,
                    )
                    .is_break()
                    {
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
            MPV_EVENT_VIDEO_RECONFIG => needs_redraw = true,
            MPV_EVENT_FILE_LOADED => {
                lock(&status).loading = false;
                if let Some(audio) = pending_audio.take()
                    && let Err(error) = mpv.command(&["audio-add", audio.as_str(), "select"])
                {
                    lock(&status).error = Some(error.to_string());
                }
                needs_redraw = true;
            }
            MPV_EVENT_END_FILE => {
                let mut status = lock(&status);
                status.loading = false;
                if event.error < 0 {
                    pending_audio.take();
                    status.error = Some(MpvError::from_code(event.error).to_string());
                }
            }
            _ => {}
        }

        let update = render.update();
        if (needs_redraw || update & MPV_RENDER_UPDATE_FRAME != 0)
            && let Some(frame) = render_frame(&mpv, &render, viewport, &mut buffer)
        {
            *lock(&frame_slot) = Some(frame);
            needs_redraw = false;
        }

        if last_status.elapsed() >= STATUS_INTERVAL {
            update_status(&mpv, &status);
            last_status = Instant::now();
        }
    }
}

fn apply_command(
    command: Command,
    mpv: &Mpv,
    viewport: &mut VideoSize,
    needs_redraw: &mut bool,
    frame_slot: &Mutex<Option<VideoFrame>>,
    status: &Mutex<PlayerStatus>,
    pending_audio: &mut Option<String>,
) -> ControlFlow<()> {
    match command {
        Command::Load { video, audio } => {
            *pending_audio = audio;
            {
                let mut status = lock(status);
                status.loading = true;
                status.ended = false;
                status.buffering = false;
                status.time_pos = 0.0;
                status.duration = 0.0;
                status.error = None;
            }
            lock(frame_slot).take();

            if let Err(error) = mpv.command(&["loadfile", video.as_str(), "replace"]) {
                let mut status = lock(status);
                status.loading = false;
                status.error = Some(error.to_string());
                return ControlFlow::Continue(());
            }
            let _ = mpv.set_property_flag("pause", false);
            *needs_redraw = true;
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
            if *viewport != size {
                *viewport = size;
                *needs_redraw = true;
            }
        }
        Command::Shutdown => return ControlFlow::Break(()),
    }

    ControlFlow::Continue(())
}

fn render_frame(
    mpv: &Mpv,
    render: &RenderContext,
    viewport: VideoSize,
    buffer: &mut Vec<u8>,
) -> Option<VideoFrame> {
    let source = mpv.video_size()?;
    let size = target_dimensions(source, viewport);
    if size.width == 0 || size.height == 0 {
        return None;
    }

    let stride = size.width * 4;
    let length = stride as usize * size.height as usize;
    if buffer.len() != length {
        buffer.resize(length, 0);
    }

    render
        .render(size.width, size.height, stride, buffer)
        .ok()?;
    for pixel in buffer.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }

    Some(VideoFrame {
        width: size.width,
        height: size.height,
        data: std::mem::take(buffer),
    })
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

fn update_status(mpv: &Mpv, status: &Mutex<PlayerStatus>) {
    let mut next = lock(status).clone();
    next.idle = mpv.get_property_flag("idle-active").unwrap_or(next.idle);
    next.paused = mpv.get_property_flag("pause").unwrap_or(next.paused);
    next.time_pos = mpv.get_property_double("time-pos").unwrap_or(next.time_pos);
    next.duration = mpv.get_property_double("duration").unwrap_or(next.duration);
    next.volume = mpv.get_property_double("volume").unwrap_or(next.volume);
    next.speed = mpv.get_property_double("speed").unwrap_or(next.speed);
    next.hwdec = mpv.get_property_string("hwdec-current");
    next.audio_codec = mpv.get_property_string("audio-codec-name");
    next.video_size = mpv.video_size();
    next.buffering = mpv.get_property_flag("paused-for-cache").unwrap_or(false);
    next.ended = mpv.get_property_flag("eof-reached").unwrap_or(false);
    if next.duration.is_finite() && next.duration > 0.0 {
        next.loading = false;
    }

    let mut current = lock(status);
    if *current != next {
        *current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dimensions_fits_inside_viewport() {
        assert_eq!(
            target_dimensions(size(1920, 1080), size(1280, 720)),
            size(1280, 720)
        );
        assert_eq!(
            target_dimensions(size(1920, 1080), size(1000, 1000)),
            size(1000, 563)
        );
    }

    #[test]
    fn target_dimensions_does_not_upscale() {
        assert_eq!(
            target_dimensions(size(1280, 720), size(1920, 1080)),
            size(1280, 720)
        );
    }

    #[test]
    fn target_dimensions_waits_for_viewport() {
        assert_eq!(target_dimensions(size(1920, 1080), size(0, 0)), size(0, 0));
        assert_eq!(target_dimensions(size(0, 0), size(1280, 720)), size(0, 0));
    }

    fn size(width: u32, height: u32) -> VideoSize {
        VideoSize::new(width, height)
    }
}
