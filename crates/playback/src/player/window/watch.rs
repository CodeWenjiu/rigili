use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Client, EpisodeId, WatchReport};
use rigili_core::runtime::Tokio;
use rigili_player::PlaybackPhase;

use super::PlayerWindow;
use crate::request::PlaybackSource;

const FIRST_REPORT: Duration = Duration::from_secs(3);
const REPORT_INTERVAL: Duration = Duration::from_secs(10);
const MIN_PLAYED: f64 = 1.;
const MIN_RESUME: f64 = 3.;
const RESUME_TAIL: f64 = 10.;

struct Tick {
    client: Arc<Client>,
    epoch: u64,
    aid: u64,
    bvid: String,
    cid: u64,
    episode: Option<EpisodeId>,
    played: u64,
    duration: u64,
}

struct Session {
    epoch: u64,
    bvid: String,
    cid: u64,
    episode: Option<EpisodeId>,
    started_at: u64,
    sent_at: Instant,
    last: u64,
    max: u64,
}

impl Session {
    fn new(tick: &Tick) -> Self {
        Self {
            epoch: tick.epoch,
            bvid: tick.bvid.clone(),
            cid: tick.cid,
            episode: tick.episode,
            started_at: unix_now(),
            sent_at: Instant::now(),
            last: 0,
            max: 0,
        }
    }

    fn follows(&self, tick: &Tick) -> bool {
        self.epoch == tick.epoch
            && self.bvid == tick.bvid
            && self.cid == tick.cid
            && self.episode == tick.episode
    }
}

impl PlayerWindow {
    /// The site records watch history only through this heartbeat; nothing else
    /// the player does tells the server that a video was watched.
    pub(super) fn spawn_watch_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.watch_task = Some(cx.spawn_in(window, async move |this, async_cx| {
            let mut interval = FIRST_REPORT;
            let mut session: Option<Session> = None;
            loop {
                async_cx.background_executor().timer(interval).await;
                interval = REPORT_INTERVAL;
                let Ok(Some(tick)) = this.update(async_cx, |this, cx| this.watch_tick(cx)) else {
                    continue;
                };
                if !session
                    .as_ref()
                    .is_some_and(|session| session.follows(&tick))
                {
                    session = Some(Session::new(&tick));
                }
                let Some(session) = session.as_mut() else {
                    continue;
                };
                // Nothing moved since the last report: the video is paused, or
                // it ended, and repeating the same position every interval only
                // inflates the watch time the server records.
                if tick.played == session.last {
                    session.sent_at = Instant::now();
                    continue;
                }

                let report = WatchReport {
                    aid: tick.aid,
                    bvid: tick.bvid,
                    cid: tick.cid,
                    episode: tick.episode,
                    played: tick.played,
                    duration: tick.duration,
                    started_at: session.started_at,
                    realtime: session.sent_at.elapsed().as_secs(),
                    last: session.last,
                    max: tick.played.max(session.max),
                };
                session.last = report.played;
                session.max = report.max;
                session.sent_at = Instant::now();
                let client = tick.client;

                // The request has to run on the Tokio runtime; awaiting reqwest
                // straight from this task panics on the missing reactor.
                let request = Tokio::spawn(async_cx, async move {
                    match client.report_history(&report).await {
                        Ok(()) => rigili_monitor::monitor!(
                            "history",
                            "{} cid {} at {}s",
                            report.bvid,
                            report.cid,
                            report.played
                        ),
                        Err(error) => rigili_monitor::monitor!(
                            "history",
                            "{} cid {} failed: {error}",
                            report.bvid,
                            report.cid
                        ),
                    }
                });
                let _ = request.await;
            }
        }));
    }

    fn watch_tick(&self, cx: &App) -> Option<Tick> {
        let tab = self.tab();
        if tab.is_live || tab.loading || self.session != Some(tab.id) {
            return None;
        }
        let client = self.client.clone()?;
        let PlaybackSource::Video { bvid, cid, episode } = tab.request.source.clone() else {
            return None;
        };
        let status = self.player.read(cx).status();
        if status.error.is_some()
            || matches!(status.phase, PlaybackPhase::Idle | PlaybackPhase::Loading)
            || status.time_pos < MIN_PLAYED
            || status.duration < 1.
        {
            return None;
        }
        Some(Tick {
            client,
            epoch: self.epoch,
            aid: tab.info.read(cx).aid()?,
            bvid,
            cid: cid?,
            episode,
            played: status.time_pos.round() as u64,
            duration: status.duration.round() as u64,
        })
    }
}

/// Where to start this part, from the stop position the site reports. The
/// value is milliseconds into the part; a position outside it means the viewer
/// barely started or already finished, and both want the beginning.
pub(super) fn resume_position(last_play_time: f64, duration: f64) -> Option<f64> {
    let position = (last_play_time > 0. && duration > 0.)
        .then(|| last_play_time / 1000.)
        .filter(|seconds| *seconds >= MIN_RESUME && *seconds < duration - RESUME_TAIL);
    rigili_monitor::monitor!(
        "resume",
        "{last_play_time}ms of {duration:.0}s -> {position:?}"
    );
    position
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::resume_position;

    #[test]
    fn resumes_inside_the_part_only() {
        assert_eq!(resume_position(0., 600.), None);
        assert_eq!(resume_position(2_000., 600.), None);
        assert_eq!(resume_position(5_000., 600.), Some(5.));
        assert_eq!(resume_position(300_000., 600.), Some(300.));
        assert_eq!(resume_position(595_000., 600.), None);
        assert_eq!(resume_position(300_000., 0.), None);
    }

    #[test]
    fn a_position_outside_the_part_is_not_a_resume_point() {
        assert_eq!(resume_position(300., 600.), None);
    }
}
