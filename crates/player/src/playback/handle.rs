use std::{
    path::Path,
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    OsdOverlay, VideoSize,
    mpv::RenderWake,
    native::{NATIVE_SLOTS, NativeFrame},
};

use super::{
    status::{PlayerStatus, StatusSlot, VideoFrame},
    sync::{lock, mutate_status},
    worker::run_worker,
};

#[derive(Clone)]
pub struct Player {
    inner: Arc<PlayerInner>,
}

struct PlayerInner {
    sender: Mutex<Sender<Command>>,
    frame: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<StatusSlot>,
    viewport: Mutex<VideoSize>,
    wake: Arc<RenderWake>,
    worker: Mutex<Option<JoinHandle<()>>>,
    native: Arc<NativeSlot>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderBackend {
    GpuPreferred,
    Software,
}

pub(super) enum Command {
    Load {
        video: String,
        audio: Option<String>,
        start: Option<f64>,
        paused: bool,
    },
    TogglePause,
    SetAudio {
        audio: String,
    },
    Stop,
    Seek(f64),
    SetVolume(f64),
    SetMute(bool),
    SetSpeed(f64),
    SetVideoEnabled(bool),
    SetRendering(bool),
    SetViewport(VideoSize),
    SetOsd(Option<OsdOverlay>),
    Redraw,
    Shutdown,
}

impl Player {
    pub fn new(fonts: Option<&Path>) -> Self {
        Self::spawn(RenderBackend::GpuPreferred, fonts)
    }

    /// Creates a player that always renders on the CPU, for small previews.
    pub fn new_software(fonts: Option<&Path>) -> Self {
        Self::spawn(RenderBackend::Software, fonts)
    }

    fn spawn(backend: RenderBackend, fonts: Option<&Path>) -> Self {
        let fonts = fonts.map(Path::to_path_buf);
        let (sender, receiver) = mpsc::channel();
        let frame = Arc::new(Mutex::new(None));
        let status = Arc::new(StatusSlot::new(Arc::new(PlayerStatus::default())));
        let wake = Arc::new(RenderWake::default());
        let native = Arc::new(NativeSlot::default());

        let worker_frame = Arc::clone(&frame);
        let worker_status = Arc::clone(&status);
        let worker_wake = Arc::clone(&wake);
        let worker_native = Arc::clone(&native);
        let worker = thread::Builder::new()
            .name("rigili-mpv".into())
            .spawn(move || {
                run_worker(
                    receiver,
                    backend,
                    worker_wake,
                    worker_frame,
                    worker_status,
                    worker_native,
                    fonts,
                )
            });

        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                mutate_status(&status, |status| {
                    status.error = Some(format!("无法启动播放器线程：{error}"));
                });
                None
            }
        };

        Self {
            inner: Arc::new(PlayerInner {
                sender: Mutex::new(sender),
                frame,
                status,
                viewport: Mutex::new(VideoSize::default()),
                wake,
                worker: Mutex::new(worker),
                native,
            }),
        }
    }

    pub fn load(&self, video: &str, audio: Option<&str>, start: Option<f64>) {
        self.send(Command::Load {
            video: video.to_owned(),
            audio: audio.map(str::to_owned),
            start: start.map(|start| start.max(0.0)),
            paused: false,
        });
    }

    /// Reloads the video stream while keeping the current audio and position.
    pub fn switch_video(&self, video: &str, audio: Option<&str>, start: f64, paused: bool) {
        self.send(Command::Load {
            video: video.to_owned(),
            audio: audio.map(str::to_owned),
            start: Some(start.max(0.0)),
            paused,
        });
    }

    pub fn toggle_pause(&self) {
        self.send(Command::TogglePause);
    }

    pub fn set_audio(&self, audio: &str) {
        self.send(Command::SetAudio {
            audio: audio.to_owned(),
        });
    }

    /// Unloads the current file so mpv releases its demuxer and decoder buffers.
    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    pub fn seek(&self, seconds: f64) {
        self.send(Command::Seek(seconds.max(0.0)));
    }

    pub fn set_volume(&self, volume: f64) {
        self.send(Command::SetVolume(volume.clamp(0.0, 100.0)));
    }

    pub fn set_muted(&self, muted: bool) {
        self.send(Command::SetMute(muted));
    }

    /// Disables the video track so a background tab only decodes audio.
    pub fn set_video_enabled(&self, enabled: bool) {
        self.send(Command::SetVideoEnabled(enabled));
    }

    /// Stops rendering frames for a player that is heard but not seen.
    pub fn set_rendering(&self, enabled: bool) {
        self.send(Command::SetRendering(enabled));
    }

    pub fn set_speed(&self, speed: f64) {
        self.send(Command::SetSpeed(speed.clamp(0.25, 4.0)));
    }

    pub fn set_viewport(&self, size: VideoSize) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        let mut viewport = lock(&self.inner.viewport);
        if *viewport == size {
            return;
        }
        *viewport = size;
        drop(viewport);

        self.send(Command::SetViewport(size));
    }

    /// Replaces the external ASS overlay drawn over the video, or clears it.
    pub fn set_osd(&self, overlay: Option<OsdOverlay>) {
        self.send(Command::SetOsd(overlay));
    }

    pub fn take_frame(&self) -> Option<VideoFrame> {
        lock(&self.inner.frame).take()
    }

    /// Switches the worker between CPU readback and direct GPU output.
    pub fn set_native_output(&self, enabled: bool) {
        self.inner.native.request(enabled);
        self.inner.wake.signal();
    }

    /// Asks the worker to render the current frame again, e.g. after the
    /// surface was detached and reattached while playback stayed paused.
    pub fn redraw(&self) {
        self.send(Command::Redraw);
    }

    /// Takes the newest frame rendered for direct GPU output.
    pub fn take_native_frames(&self) -> Vec<NativeFrame> {
        self.inner.native.take()
    }

    /// Tells the worker the compositor released a native frame.
    pub fn native_frame_released(&self, slot: u8) {
        self.inner.native.release(slot);
        self.inner.wake.signal();
    }

    pub fn native_output_failed(&self) -> bool {
        self.inner.native.failed()
    }

    pub fn status(&self) -> Arc<PlayerStatus> {
        lock(&self.inner.status).clone()
    }

    fn send(&self, command: Command) {
        if let Err(error) = lock(&self.inner.sender).send(command) {
            mutate_status(&self.inner.status, |status| {
                status.error.get_or_insert_with(|| {
                    format!("播放器线程已退出（{error}），请从终端启动查看详细错误")
                });
            });
        }
        self.inner.wake.signal();
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Drop for PlayerInner {
    fn drop(&mut self) {
        self.native.fail();
        let _ = lock(&self.sender).send(Command::Shutdown);
        self.wake.signal();
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

pub(crate) struct NativeSlot {
    state: Mutex<NativeState>,
    condvar: Condvar,
}

struct NativeState {
    requested: bool,
    failed: bool,
    free: [bool; NATIVE_SLOTS],
    frames: Vec<NativeFrame>,
}

impl Default for NativeSlot {
    fn default() -> Self {
        Self {
            state: Mutex::new(NativeState {
                requested: false,
                failed: false,
                free: [true; NATIVE_SLOTS],
                frames: Vec::new(),
            }),
            condvar: Condvar::new(),
        }
    }
}

impl NativeSlot {
    pub(crate) fn request(&self, enabled: bool) {
        let mut state = lock(&self.state);
        state.requested = enabled;
        state.failed = false;
        state.free = [true; NATIVE_SLOTS];
        state.frames.clear();
        drop(state);
        self.condvar.notify_all();
    }

    pub(crate) fn enabled(&self) -> bool {
        let state = lock(&self.state);
        state.requested && !state.failed
    }

    pub(crate) fn failed(&self) -> bool {
        lock(&self.state).failed
    }

    /// Waits for a texture the compositor is no longer reading.
    pub(crate) fn acquire(&self) -> Option<usize> {
        let mut state = lock(&self.state);
        let mut slot = free_slot(&state);
        if slot.is_none() && state.requested && !state.failed {
            let (guard, _) = self
                .condvar
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = guard;
            slot = free_slot(&state);
        }
        let slot = slot?;
        state.free[slot] = false;
        Some(slot)
    }

    pub(crate) fn publish(&self, frame: NativeFrame) {
        lock(&self.state).frames.push(frame);
    }

    pub(crate) fn take(&self) -> Vec<NativeFrame> {
        std::mem::take(&mut lock(&self.state).frames)
    }

    pub(crate) fn release(&self, slot: u8) {
        let mut state = lock(&self.state);
        if let Some(free) = state.free.get_mut(slot as usize) {
            *free = true;
        }
        drop(state);
        self.condvar.notify_all();
    }

    pub(crate) fn fail(&self) {
        let mut state = lock(&self.state);
        state.failed = true;
        state.free = [true; NATIVE_SLOTS];
        state.frames.clear();
        drop(state);
        self.condvar.notify_all();
    }
}

fn free_slot(state: &NativeState) -> Option<usize> {
    if !state.requested || state.failed {
        return None;
    }
    state.free.iter().position(|free| *free)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    #[ignore = "requires RIGILI_TEST_VIDEO and a working libmpv"]
    fn plays_a_local_file() {
        let path = std::env::var("RIGILI_TEST_VIDEO").expect("RIGILI_TEST_VIDEO");
        let audio = std::env::var("RIGILI_TEST_AUDIO")
            .ok()
            .filter(|path| !path.is_empty());
        let player = Player::new(None);
        player.set_viewport(VideoSize::new(1280, 720));
        player.load(&path, audio.as_deref(), None);

        wait_until(&player, Duration::from_secs(5), |status| {
            status.duration > 0.0
        });
        if audio.is_some() {
            wait_until(&player, Duration::from_secs(3), |status| {
                status.audio_codec.is_some()
            });
        }

        let frame = player.take_frame().expect("no video frame");
        assert!(frame.width > 0 && frame.height > 0);
        assert_eq!(
            frame.data.len(),
            frame.width as usize * frame.height as usize * 4
        );
        assert!(
            player.status().error.is_none(),
            "{:?}",
            player.status().error
        );

        player.seek(5.0);
        wait_until(&player, Duration::from_secs(3), |status| {
            status.time_pos >= 4.5
        });

        player.set_speed(2.0);
        wait_until(&player, Duration::from_secs(2), |status| {
            (status.speed - 2.0).abs() < 0.01
        });

        player.set_volume(35.0);
        wait_until(&player, Duration::from_secs(2), |status| {
            (status.volume - 35.0).abs() < 0.5
        });

        player.toggle_pause();
        wait_until(&player, Duration::from_secs(2), |status| status.paused);
        player.toggle_pause();
        wait_until(&player, Duration::from_secs(2), |status| !status.paused);

        player.switch_video(&path, audio.as_deref(), 3.0, false);
        wait_until(&player, Duration::from_secs(5), |status| {
            status.duration > 0.0 && status.time_pos > 2.0
        });
        if audio.is_some() {
            wait_until(&player, Duration::from_secs(3), |status| {
                status.audio_codec.is_some()
            });
        }

        let status = player.status();
        println!("{status:?}");
        assert!(status.error.is_none(), "{:?}", status.error);
    }

    fn wait_until(player: &Player, timeout: Duration, condition: impl Fn(&PlayerStatus) -> bool) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let status = player.status();
            if status.error.is_some() {
                panic!("{:?}", status.error);
            }
            if condition(&status) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for player state: {:?}", player.status());
    }

    #[test]
    #[ignore = "requires RIGILI_TEST_VIDEO, RIGILI_TEST_AUDIO and a working libmpv"]
    fn background_audio_keeps_playing_and_resumes_video() {
        let video = std::env::var("RIGILI_TEST_VIDEO").expect("RIGILI_TEST_VIDEO");
        let audio = std::env::var("RIGILI_TEST_AUDIO").expect("RIGILI_TEST_AUDIO");
        let player = Player::new(None);
        player.set_viewport(VideoSize::new(1280, 720));
        player.load(&video, Some(&audio), None);
        wait_until(&player, Duration::from_secs(10), |status| {
            status.duration > 0.0 && status.time_pos > 0.0
        });

        player.set_video_enabled(false);
        player.set_rendering(false);
        let before = player.status().time_pos;
        thread::sleep(Duration::from_millis(800));
        let status = player.status();
        assert!(status.error.is_none(), "{:?}", status.error);
        assert!(
            status.time_pos > before,
            "audio did not advance: {before} -> {}",
            status.time_pos
        );

        player.set_video_enabled(true);
        player.set_rendering(true);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(frame) = player.take_frame() {
                assert!(frame.width > 0 && frame.height > 0);
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("no frame after resuming video");
    }
}
