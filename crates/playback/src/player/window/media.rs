use std::sync::Arc;

use gpui_kit::prelude::*;
use rigili_bilibili::Danmaku;
use rigili_core::runtime::{spawn_api, spawn_api_task};

use crate::player::danmaku::feed;

use super::PlayerWindow;

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
            feed::Feed::new(client, aid, cid, duration, demand).run(sender),
            move |_, _, _| {},
        ));
    }

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
}
