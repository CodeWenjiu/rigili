use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use rigili_bilibili::{ApiError, Client, Danmaku};
use tokio::{sync::mpsc::UnboundedSender, task::JoinSet};

use super::DanmakuDemand;

const SEGMENT_SECS: u64 = 360;
const MAX_SEGMENTS: u32 = 64;
const SEGMENTS_IN_FLIGHT: usize = 2;
const WINDOW_BEHIND: u32 = 1;
const WINDOW_AHEAD: u32 = 3;
const SEGMENT_RETRIES: u8 = 2;
const IDLE: Duration = Duration::from_millis(200);

pub(crate) struct Feed {
    client: Arc<Client>,
    aid: u64,
    cid: u64,
    total: u32,
    demand: Arc<DanmakuDemand>,
}

impl Feed {
    pub(crate) fn new(
        client: Arc<Client>,
        aid: u64,
        cid: u64,
        duration: u64,
        demand: Arc<DanmakuDemand>,
    ) -> Self {
        let total = (duration.max(1).div_ceil(SEGMENT_SECS) as u32).min(MAX_SEGMENTS);
        Self {
            client,
            aid,
            cid,
            total,
            demand,
        }
    }

    pub(crate) async fn run(
        self,
        sender: UnboundedSender<Arc<Vec<Danmaku>>>,
    ) -> Result<(), ApiError> {
        let mut tasks = JoinSet::new();
        let mut in_flight = HashSet::new();
        let mut failures: BTreeMap<u32, u8> = BTreeMap::new();
        let mut loaded: BTreeMap<u32, Vec<Danmaku>> = BTreeMap::new();
        let mut loaded_window = None;
        loop {
            if sender.is_closed() {
                break;
            }
            if !self.demand.should_load() {
                if tasks.is_empty() {
                    tokio::time::sleep(IDLE).await;
                    continue;
                }
            } else {
                let current = segment_at(self.demand.position(), self.total);
                let wanted_window = segment_window(current, self.total);
                if loaded_window != Some(wanted_window) {
                    loaded_window = Some(wanted_window);
                    failures.clear();
                    loaded.retain(|index, _| (wanted_window.0..=wanted_window.1).contains(index));
                    let _ = sender.send(merge_segments(&loaded));
                    rigili_monitor::monitor!(
                        "danmaku",
                        "{} window {}..={} ({} segments loaded)",
                        self.cid,
                        wanted_window.0,
                        wanted_window.1,
                        loaded.len()
                    );
                }
                let mut missing: Vec<u32> = (wanted_window.0..=wanted_window.1)
                    .filter(|index| {
                        !loaded.contains_key(index)
                            && !in_flight.contains(index)
                            && failures.get(index).copied().unwrap_or(0) < SEGMENT_RETRIES
                    })
                    .collect();
                missing.sort_by_key(|index| (index.abs_diff(current), *index));
                for index in missing {
                    if tasks.len() >= SEGMENTS_IN_FLIGHT {
                        break;
                    }
                    let client = self.client.clone();
                    let cid = self.cid;
                    let aid = self.aid;
                    let total = self.total;
                    tasks.spawn(async move {
                        let started = Instant::now();
                        let result = client.danmaku_segment(cid, aid, index).await;
                        rigili_monitor::monitor!(
                            "danmaku",
                            "segment {index}/{total} finished after {:?}",
                            started.elapsed()
                        );
                        (index, result)
                    });
                    in_flight.insert(index);
                }
            }
            let Some(joined) = tasks.join_next().await else {
                tokio::time::sleep(IDLE).await;
                continue;
            };
            match joined {
                Ok((index, Ok(part))) => {
                    in_flight.remove(&index);
                    if loaded_window.is_some_and(|(start, end)| (start..=end).contains(&index)) {
                        loaded.insert(index, part);
                        let _ = sender.send(merge_segments(&loaded));
                    }
                }
                Ok((index, Err(error))) => {
                    in_flight.remove(&index);
                    *failures.entry(index).or_default() += 1;
                    rigili_monitor::monitor!(
                        "danmaku",
                        "{} segment {index} failed: {error}",
                        self.cid
                    );
                }
                Err(error) => {
                    in_flight.clear();
                    rigili_monitor::monitor!("danmaku", "{} task failed: {error}", self.cid);
                }
            }
        }
        Ok(())
    }
}

fn segment_at(position: f64, total: u32) -> u32 {
    let index = (position.max(0.) / SEGMENT_SECS as f64) as u32 + 1;
    index.clamp(1, total.max(1))
}

fn segment_window(current: u32, total: u32) -> (u32, u32) {
    let total = total.max(1);
    let start = current.saturating_sub(WINDOW_BEHIND).max(1);
    let end = current.saturating_add(WINDOW_AHEAD).min(total);
    (start, end.max(start))
}

fn merge_segments(segments: &BTreeMap<u32, Vec<Danmaku>>) -> Arc<Vec<Danmaku>> {
    let mut items = Vec::with_capacity(segments.values().map(Vec::len).sum());
    for part in segments.values() {
        items.extend(part.iter().cloned());
    }
    items.sort_by(|a, b| a.time.total_cmp(&b.time));
    Arc::new(items)
}

#[cfg(test)]
mod tests {
    use super::{merge_segments, segment_at, segment_window};

    #[test]
    fn windows_segments_around_playback() {
        assert_eq!(segment_window(1, 11), (1, 4));
        assert_eq!(segment_window(5, 11), (4, 8));
        assert_eq!(segment_window(11, 11), (10, 11));
        assert_eq!(segment_window(1, 0), (1, 1));
    }

    #[test]
    fn locates_the_segment_of_a_position() {
        assert_eq!(segment_at(0., 11), 1);
        assert_eq!(segment_at(359.9, 11), 1);
        assert_eq!(segment_at(360., 11), 2);
        assert_eq!(segment_at(3600., 11), 11);
        assert_eq!(segment_at(-1., 11), 1);
    }

    #[test]
    fn merges_segments_in_time_order() {
        use std::collections::BTreeMap;

        use rigili_bilibili::{Danmaku, DanmakuMode};

        let item = |time| Danmaku {
            id: 0,
            time,
            mode: DanmakuMode::Scroll,
            font_size: 25,
            color: 0x00ff_ffff,
            text: String::new(),
        };
        let mut segments = BTreeMap::new();
        segments.insert(2, vec![item(6.0), item(9.0)]);
        segments.insert(1, vec![item(0.0), item(1.0)]);

        let merged = merge_segments(&segments);
        let times: Vec<f64> = merged.iter().map(|item| item.time).collect();
        assert_eq!(times, [0.0, 1.0, 6.0, 9.0]);
    }
}
