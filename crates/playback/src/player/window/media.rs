use gpui_kit::prelude::*;
use rigili_core::runtime::{spawn_api, spawn_api_task};
use tokio::task::JoinSet;

use super::PlayerWindow;

const DANMAKU_SEGMENT_SECS: u64 = 360;
const DANMAKU_MAX_SEGMENTS: u32 = 64;
const DANMAKU_SEGMENTS_IN_FLIGHT: usize = 2;

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
        self.danmaku_task = Some(spawn_api_task(
            cx,
            async move {
                let mut tasks = JoinSet::new();
                let mut next = 1;
                let mut items = Vec::new();
                let mut waiting = false;
                loop {
                    while tasks.len() < DANMAKU_SEGMENTS_IN_FLIGHT
                        && next <= segments
                        && demand.should_load()
                    {
                        let client = client.clone();
                        let index = next;
                        tasks.spawn(async move {
                            let started = std::time::Instant::now();
                            let result = client.danmaku_segment(cid, aid, index).await;
                            rigili_monitor::monitor!(
                                "danmaku",
                                "segment {index}/{segments} finished after {:?}",
                                started.elapsed()
                            );
                            result
                        });
                        next += 1;
                    }
                    if tasks.is_empty() {
                        if next > segments {
                            break;
                        }
                        if !demand.should_load() {
                            if !waiting {
                                rigili_monitor::monitor!(
                                    "danmaku",
                                    "{cid} waiting for playback to resume"
                                );
                                waiting = true;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            continue;
                        }
                    }
                    waiting = false;
                    let Some(result) = tasks.join_next().await else {
                        break;
                    };
                    match result {
                        Ok(Ok(mut part)) => items.append(&mut part),
                        Ok(Err(error)) => {
                            rigili_monitor::monitor!("danmaku", "{cid} segment failed: {error}")
                        }
                        Err(error) => {
                            rigili_monitor::monitor!("danmaku", "{cid} task failed: {error}")
                        }
                    }
                }
                items.sort_by(|a, b| a.time.total_cmp(&b.time));
                rigili_monitor::monitor!("danmaku", "{cid} loaded {} items", items.len());
                Ok::<_, rigili_bilibili::ApiError>(items)
            },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(items) = result {
                    this.player
                        .update(cx, |player, cx| player.set_danmaku(items, cx));
                }
            },
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
