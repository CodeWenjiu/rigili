use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::prelude::*;
use rigili_bilibili::Danmaku;
use rigili_core::runtime::{spawn_api, spawn_api_task};
use tokio::task::JoinSet;

use super::PlayerWindow;

const DANMAKU_SEGMENT_SECS: u64 = 360;
const DANMAKU_MAX_SEGMENTS: u32 = 64;
const DANMAKU_SEGMENTS_IN_FLIGHT: usize = 2;
const DANMAKU_WINDOW_BEHIND: u32 = 1;
const DANMAKU_WINDOW_AHEAD: u32 = 3;
const DANMAKU_SEGMENT_RETRIES: u8 = 2;
const DANMAKU_IDLE: Duration = Duration::from_millis(200);

fn segment_at(position: f64, total: u32) -> u32 {
    let index = (position.max(0.) / DANMAKU_SEGMENT_SECS as f64) as u32 + 1;
    index.clamp(1, total.max(1))
}

fn segment_window(current: u32, total: u32) -> (u32, u32) {
    let total = total.max(1);
    let start = current.saturating_sub(DANMAKU_WINDOW_BEHIND).max(1);
    let end = current.saturating_add(DANMAKU_WINDOW_AHEAD).min(total);
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

impl PlayerWindow {
    pub(super) fn load_danmaku(
        &mut self,
        bvid: String,
        aid: u64,
        cid: u64,
        duration: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if aid == 0 {
            return;
        }
        self.player.update(cx, |player, cx| {
            player.set_danmaku_source(client.clone(), aid, bvid, cid, cx)
        });
        let demand = self.player.read(cx).danmaku_demand();
        let generation = self.generation;
        let segments =
            (duration.max(1).div_ceil(DANMAKU_SEGMENT_SECS) as u32).min(DANMAKU_MAX_SEGMENTS);

        self.danmaku_task = None;

        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Arc<Vec<Danmaku>>>();
        cx.spawn(async move |weak, cx| {
            while let Some(items) = receiver.recv().await {
                let Ok(keep) = weak.update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    this.player
                        .update(cx, |player, cx| player.set_danmaku(items, cx));
                    true
                }) else {
                    break;
                };
                if !keep {
                    break;
                }
            }
        })
        .detach();

        self.danmaku_task = Some(spawn_api_task(
            cx,
            async move {
                let mut tasks = JoinSet::new();
                let mut in_flight = HashSet::new();
                let mut failures: BTreeMap<u32, u8> = BTreeMap::new();
                let mut loaded: BTreeMap<u32, Vec<Danmaku>> = BTreeMap::new();
                let mut window = None;
                loop {
                    if sender.is_closed() {
                        break;
                    }
                    if !demand.should_load() {
                        if tasks.is_empty() {
                            tokio::time::sleep(DANMAKU_IDLE).await;
                            continue;
                        }
                    } else {
                        let current = segment_at(demand.position(), segments);
                        let wanted = segment_window(current, segments);
                        if window != Some(wanted) {
                            window = Some(wanted);
                            failures.clear();
                            loaded.retain(|index, _| (wanted.0..=wanted.1).contains(index));
                            let _ = sender.send(merge_segments(&loaded));
                            rigili_monitor::monitor!(
                                "danmaku",
                                "{cid} window {}..={} ({} segments loaded)",
                                wanted.0,
                                wanted.1,
                                loaded.len()
                            );
                        }
                        let mut missing: Vec<u32> = (wanted.0..=wanted.1)
                            .filter(|index| {
                                !loaded.contains_key(index)
                                    && !in_flight.contains(index)
                                    && failures.get(index).copied().unwrap_or(0)
                                        < DANMAKU_SEGMENT_RETRIES
                            })
                            .collect();
                        missing.sort_by_key(|index| (index.abs_diff(current), *index));
                        for index in missing {
                            if tasks.len() >= DANMAKU_SEGMENTS_IN_FLIGHT {
                                break;
                            }
                            let client = client.clone();
                            tasks.spawn(async move {
                                let started = Instant::now();
                                let result = client.danmaku_segment(cid, aid, index).await;
                                rigili_monitor::monitor!(
                                    "danmaku",
                                    "segment {index}/{segments} finished after {:?}",
                                    started.elapsed()
                                );
                                (index, result)
                            });
                            in_flight.insert(index);
                        }
                    }
                    let Some(joined) = tasks.join_next().await else {
                        tokio::time::sleep(DANMAKU_IDLE).await;
                        continue;
                    };
                    match joined {
                        Ok((index, Ok(part))) => {
                            in_flight.remove(&index);
                            if window.is_some_and(|(start, end)| (start..=end).contains(&index)) {
                                loaded.insert(index, part);
                                let _ = sender.send(merge_segments(&loaded));
                            }
                        }
                        Ok((index, Err(error))) => {
                            in_flight.remove(&index);
                            *failures.entry(index).or_default() += 1;
                            rigili_monitor::monitor!(
                                "danmaku",
                                "{cid} segment {index} failed: {error}"
                            );
                        }
                        Err(error) => {
                            in_flight.clear();
                            rigili_monitor::monitor!("danmaku", "{cid} task failed: {error}");
                        }
                    }
                }
                Ok::<(), rigili_bilibili::ApiError>(())
            },
            move |_, _, _| {},
        ));
    }
}

impl PlayerWindow {
    pub(super) fn load_previews(&self, bvid: String, cid: u64, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let generation = self.generation;
        spawn_api(
            cx,
            async move { client.videoshot(&bvid, cid).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(previews) = result {
                    this.player
                        .update(cx, |player, cx| player.set_previews(previews, cx));
                }
            },
        );
    }

    pub(super) fn load_chapters(&self, bvid: String, cid: u64, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let generation = self.generation;
        spawn_api(
            cx,
            async move { client.chapters(&bvid, cid).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(chapters) = result {
                    this.player
                        .update(cx, |player, cx| player.set_chapters(chapters, cx));
                }
            },
        );
    }
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
