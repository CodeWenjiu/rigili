use std::sync::Arc;
use std::time::Duration;

use rigili_bilibili::{ApiError, Client, Danmaku};
use tokio::sync::mpsc::UnboundedSender;

use super::DanmakuDemand;

const FLUSH_INTERVAL: Duration = Duration::from_millis(400);
// A comment is assigned the playback time it should appear at, slightly ahead
// of the moment it arrived, so the batching below never hands the layer an
// already expired item.
const LIVE_DELAY: f64 = 1.2;
const RETAIN_BEHIND: f64 = 5.;
const MAX_ITEMS: usize = 600;

pub(crate) struct Feed {
    client: Arc<Client>,
    room_id: u64,
    demand: Arc<DanmakuDemand>,
}

impl Feed {
    pub(crate) fn new(client: Arc<Client>, room_id: u64, demand: Arc<DanmakuDemand>) -> Self {
        Self {
            client,
            room_id,
            demand,
        }
    }

    pub(crate) async fn run(
        self,
        sender: UnboundedSender<Arc<Vec<Danmaku>>>,
    ) -> Result<(), ApiError> {
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let watch = self
            .client
            .clone()
            .watch_live_danmaku(self.room_id, live_sender);
        tokio::pin!(watch);
        let mut items: Vec<Danmaku> = Vec::new();
        let mut flush = tokio::time::interval(FLUSH_INTERVAL);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                result = &mut watch => return result,
                batch = live_receiver.recv() => {
                    let Some(batch) = batch else {
                        return Ok(());
                    };
                    let time = self.demand.position() + LIVE_DELAY;
                    items.extend(batch.into_iter().map(|item| Danmaku {
                        id: 0,
                        time,
                        mode: item.mode,
                        font_size: item.font_size,
                        color: item.color,
                        text: item.text,
                    }));
                    if items.len() > MAX_ITEMS {
                        items.drain(..items.len() - MAX_ITEMS);
                    }
                }
                _ = flush.tick() => {
                    let position = self.demand.position();
                    items.retain(|item| item.time >= position - RETAIN_BEHIND);
                    if items.is_empty() {
                        continue;
                    }
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
                    if sender.send(Arc::new(sorted)).is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}
