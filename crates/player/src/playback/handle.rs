use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};

use crate::VideoSize;

use super::{
    status::{PlayerStatus, VideoFrame},
    sync::lock,
    worker::run_worker,
};

#[derive(Clone)]
pub struct Player {
    inner: Arc<PlayerInner>,
}

struct PlayerInner {
    sender: Mutex<Sender<Command>>,
    frame: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<Mutex<PlayerStatus>>,
    viewport: Mutex<VideoSize>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

pub(super) enum Command {
    Load {
        video: String,
        audio: Option<String>,
    },
    TogglePause,
    Seek(f64),
    SetVolume(f64),
    SetSpeed(f64),
    SetViewport(VideoSize),
    Shutdown,
}

impl Player {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let frame = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(PlayerStatus::default()));

        let worker_frame = Arc::clone(&frame);
        let worker_status = Arc::clone(&status);
        let worker = thread::Builder::new()
            .name("rigili-mpv".into())
            .spawn(move || run_worker(receiver, worker_frame, worker_status));

        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                lock(&status).error = Some(format!("无法启动播放器线程：{error}"));
                None
            }
        };

        Self {
            inner: Arc::new(PlayerInner {
                sender: Mutex::new(sender),
                frame,
                status,
                viewport: Mutex::new(VideoSize::default()),
                worker: Mutex::new(worker),
            }),
        }
    }

    pub fn load(&self, video: &str, audio: Option<&str>) {
        self.send(Command::Load {
            video: video.to_owned(),
            audio: audio.map(str::to_owned),
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

    pub fn status(&self) -> PlayerStatus {
        lock(&self.inner.status).clone()
    }

    fn send(&self, command: Command) {
        if let Err(error) = lock(&self.inner.sender).send(command) {
            lock(&self.inner.status).error = Some(format!("播放器线程不可用：{error}"));
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PlayerInner {
    fn drop(&mut self) {
        let _ = lock(&self.sender).send(Command::Shutdown);
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    #[ignore = "requires RIGILI_TEST_VIDEO and a working libmpv"]
    fn plays_a_local_file() {
        let path = std::env::var("RIGILI_TEST_VIDEO").expect("RIGILI_TEST_VIDEO");
        let audio = std::env::var("RIGILI_TEST_AUDIO").ok();
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
