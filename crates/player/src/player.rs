use std::{
    ffi::c_int,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::mpv::{Mpv, MpvError, RenderContext};

const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;

const MPV_RENDER_UPDATE_FRAME: u64 = 1;

const EVENT_TIMEOUT: f64 = 0.005;
const STATUS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStatus {
    pub idle: bool,
    pub paused: bool,
    pub loading: bool,
    pub ended: bool,
    pub buffering: bool,
    pub time_pos: f64,
    pub duration: f64,
    pub volume: f64,
    pub speed: f64,
    pub hwdec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_size: Option<(u32, u32)>,
    pub error: Option<String>,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            idle: true,
            paused: false,
            loading: false,
            ended: false,
            buffering: false,
            time_pos: 0.0,
            duration: 0.0,
            volume: 100.0,
            speed: 1.0,
            hwdec: None,
            audio_codec: None,
            video_size: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub struct Player {
    inner: Arc<PlayerInner>,
}

struct PlayerInner {
    sender: Mutex<Sender<Command>>,
    frame: Arc<Mutex<Option<VideoFrame>>>,
    status: Arc<Mutex<PlayerStatus>>,
    viewport: Mutex<(u32, u32)>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

enum Command {
    Load { video: String, audio: Option<String> },
    TogglePause,
    Seek(f64),
    SetVolume(f64),
    SetSpeed(f64),
    SetViewport { width: u32, height: u32 },
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
            .name("gpuili-mpv".into())
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
                viewport: Mutex::new((0, 0)),
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

    pub fn set_viewport(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let mut viewport = lock(&self.inner.viewport);
        if *viewport == (width, height) {
            return;
        }
        *viewport = (width, height);
        drop(viewport);

        self.send(Command::SetViewport { width, height });
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run_worker(
    receiver: Receiver<Command>,
    frame: Arc<Mutex<Option<VideoFrame>>>,
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

    let mut viewport = (0u32, 0u32);
    let mut pending_audio = None;
    let mut buffer = Vec::new();
    let mut needs_redraw = true;
    let mut last_status = Instant::now();

    loop {
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if !apply_command(
                        command,
                        &mpv,
                        &mut viewport,
                        &mut needs_redraw,
                        &frame,
                        &status,
                        &mut pending_audio,
                    ) {
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
            && render_frame(&mpv, &render, viewport, &mut buffer, &frame)
        {
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
    viewport: &mut (u32, u32),
    needs_redraw: &mut bool,
    frame: &Mutex<Option<VideoFrame>>,
    status: &Mutex<PlayerStatus>,
    pending_audio: &mut Option<String>,
) -> bool {
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
            lock(frame).take();

            if let Err(error) = mpv.command(&["loadfile", video.as_str(), "replace"]) {
                let mut status = lock(status);
                status.loading = false;
                status.error = Some(error.to_string());
                return true;
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
        Command::SetViewport { width, height } => {
            let next = (width, height);
            if *viewport != next {
                *viewport = next;
                *needs_redraw = true;
            }
        }
        Command::Shutdown => return false,
    }

    true
}

fn render_frame(
    mpv: &Mpv,
    render: &RenderContext,
    viewport: (u32, u32),
    buffer: &mut Vec<u8>,
    frame: &Mutex<Option<VideoFrame>>,
) -> bool {
    let Some(source) = mpv.video_size() else {
        return false;
    };
    let (width, height) = target_dimensions(source, viewport);
    if width == 0 || height == 0 {
        return false;
    }

    let stride = width * 4;
    let length = stride as usize * height as usize;
    if buffer.len() != length {
        buffer.resize(length, 0);
    }

    if render.render(width, height, stride, buffer).is_err() {
        return false;
    }
    for pixel in buffer.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }

    let data = std::mem::take(buffer);
    *lock(frame) = Some(VideoFrame {
        width,
        height,
        data,
    });
    true
}

fn target_dimensions(source: (u32, u32), viewport: (u32, u32)) -> (u32, u32) {
    if source.0 == 0 || source.1 == 0 || viewport.0 < 2 || viewport.1 < 2 {
        return (0, 0);
    }

    let scale = (viewport.0 as f64 / source.0 as f64)
        .min(viewport.1 as f64 / source.1 as f64)
        .min(1.0);
    let width = ((source.0 as f64 * scale).round() as u32).max(1);
    let height = ((source.1 as f64 * scale).round() as u32).max(1);
    (width, height)
}

fn update_status(mpv: &Mpv, status: &Mutex<PlayerStatus>) {
    let mut next = lock(status).clone();
    next.idle = mpv.get_property_flag("idle-active").unwrap_or(next.idle);
    next.paused = mpv.get_property_flag("pause").unwrap_or(next.paused);
    next.time_pos = mpv
        .get_property_double("time-pos")
        .unwrap_or(next.time_pos);
    next.duration = mpv
        .get_property_double("duration")
        .unwrap_or(next.duration);
    next.volume = mpv.get_property_double("volume").unwrap_or(next.volume);
    next.speed = mpv.get_property_double("speed").unwrap_or(next.speed);
    next.hwdec = mpv.get_property_string("hwdec-current");
    next.audio_codec = mpv.get_property_string("audio-codec-name");
    next.video_size = mpv.video_size();
    next.buffering = mpv
        .get_property_flag("paused-for-cache")
        .unwrap_or(false);
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
            target_dimensions((1920, 1080), (1280, 720)),
            (1280, 720)
        );
        assert_eq!(target_dimensions((1920, 1080), (1000, 1000)), (1000, 563));
    }

    #[test]
    fn target_dimensions_does_not_upscale() {
        assert_eq!(
            target_dimensions((1280, 720), (1920, 1080)),
            (1280, 720)
        );
    }

    #[test]
    fn target_dimensions_waits_for_viewport() {
        assert_eq!(target_dimensions((1920, 1080), (0, 0)), (0, 0));
        assert_eq!(target_dimensions((0, 0), (1280, 720)), (0, 0));
    }

    #[test]
    #[ignore = "requires GPUILI_TEST_VIDEO and a working libmpv"]
    fn plays_a_local_file() {
        let path = std::env::var("GPUILI_TEST_VIDEO").expect("GPUILI_TEST_VIDEO");
        let audio = std::env::var("GPUILI_TEST_AUDIO").ok();
        let player = Player::new();
        player.set_viewport(1280, 720);
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
        assert!(player.status().error.is_none(), "{:?}", player.status().error);

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

    fn wait_until(
        player: &Player,
        timeout: Duration,
        condition: impl Fn(&PlayerStatus) -> bool,
    ) {
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
