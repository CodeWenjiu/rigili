use std::{
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    VideoSize,
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
    Seek(f64),
    SetVolume(f64),
    SetMute(bool),
    SetSpeed(f64),
    SetViewport(VideoSize),
    Shutdown,
}

impl Player {
    pub fn new() -> Self {
        Self::spawn(RenderBackend::GpuPreferred)
    }

    /// Creates a player that always renders on the CPU, for small previews.
    pub fn new_software() -> Self {
        Self::spawn(RenderBackend::Software)
    }

    fn spawn(backend: RenderBackend) -> Self {
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

    pub fn load(&self, video: &str, audio: Option<&str>) {
        self.send(Command::Load {
            video: video.to_owned(),
            audio: audio.map(str::to_owned),
            start: None,
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

    pub fn seek(&self, seconds: f64) {
        self.send(Command::Seek(seconds.max(0.0)));
    }

    pub fn set_volume(&self, volume: f64) {
        self.send(Command::SetVolume(volume.clamp(0.0, 100.0)));
    }

    pub fn set_muted(&self, muted: bool) {
        self.send(Command::SetMute(muted));
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

    pub fn take_frame(&self) -> Option<VideoFrame> {
        lock(&self.inner.frame).take()
    }

    /// Switches the worker between CPU readback and direct GPU output.
    pub fn set_native_output(&self, enabled: bool) {
        self.inner.native.request(enabled);
        self.inner.wake.signal();
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
                status.error = Some(format!("播放器线程不可用：{error}"));
            });
        }
        self.inner.wake.signal();
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
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
        let player = Player::new();
        player.set_viewport(VideoSize::new(1280, 720));
        player.load(&path, audio.as_deref());

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

        player.load(&path, audio.as_deref());
        wait_until(&player, Duration::from_secs(5), |status| {
            status.duration > 0.0 && status.time_pos < 1.0
        });

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
}
