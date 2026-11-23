use std::sync::Arc;

use gpui_kit::prelude::*;
use rigili_bilibili::Danmaku;
use rigili_core::runtime::{ApiTaskError, spawn_api, spawn_api_task};

use crate::player::danmaku::{feed, live};

use super::PlayerWindow;

pub(super) struct DanmakuRequest {
    pub(super) tab_id: u64,
    pub(super) generation: u64,
    pub(super) bvid: String,
    pub(super) aid: u64,
    pub(super) cid: u64,
    pub(super) duration: u64,
}

impl PlayerWindow {
    pub(super) fn load_danmaku_for(&mut self, request: DanmakuRequest, cx: &mut Context<Self>) {
        let DanmakuRequest {
            tab_id,
            generation,
            bvid,
            aid,
            cid,
            duration,
        } = request;
        let Some(client) = self.client.clone() else {
            return;
        };
        if aid == 0 || !self.is_active(tab_id, generation) {
            return;
        }
        self.player.update(cx, |player, cx| {
            player.set_danmaku_source(client.clone(), aid, bvid, cid, cx)
        });
        let demand = self.player.read(cx).danmaku_demand();

        self.tab_mut().danmaku_task = None;

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        self.forward_danmaku(tab_id, generation, receiver, cx);
        self.tab_mut().danmaku_task = Some(spawn_api_task(
            cx,
            feed::Feed::new(client, aid, cid, duration, demand).run(sender),
            move |_, _, _| {},
        ));
    }

    pub(super) fn load_live_danmaku_for(
        &mut self,
        tab_id: u64,
        generation: u64,
        room_id: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.is_active(tab_id, generation) {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let demand = self.player.read(cx).danmaku_demand();
        self.tab_mut().danmaku_task = None;

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        self.forward_danmaku(tab_id, generation, receiver, cx);
        self.tab_mut().danmaku_task = Some(spawn_api_task(
            cx,
            live::Feed::new(client, room_id, demand).run(sender),
            move |_, _, _| {},
        ));
    }

    fn forward_danmaku(
        &mut self,
        tab_id: u64,
        generation: u64,
        mut receiver: tokio::sync::mpsc::UnboundedReceiver<Arc<Vec<Danmaku>>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            while let Some(items) = receiver.recv().await {
                let Ok(keep) = weak.update(cx, |this, cx| {
                    if !this.is_active(tab_id, generation) {
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
    }

    pub(super) fn load_previews_for(
        &self,
        tab_id: u64,
        generation: u64,
        bvid: String,
        cid: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        spawn_api(
            cx,
            async move { client.videoshot(&bvid, cid).await },
            move |this, result, cx| {
                if !this.is_active(tab_id, generation) {
                    return;
                }
                if let Ok(previews) = result {
                    this.player
                        .update(cx, |player, cx| player.set_previews(previews, cx));
                }
            },
        );
    }

    pub(super) fn load_subtitle_for(
        &mut self,
        tab_id: u64,
        generation: u64,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(tab_index) = self.tab_index(tab_id) else {
            return;
        };
        let tab = &mut self.tabs[tab_index];
        if tab
            .subtitle_files
            .get(index)
            .and_then(Option::as_ref)
            .is_some()
            || tab.subtitle_loading == Some(index)
        {
            return;
        }
        let Some(track) = tab.subtitles.get(index).cloned() else {
            return;
        };
        tab.subtitle_loading = Some(index);

        let task = spawn_api_task(
            cx,
            async move { client.subtitle(&track.url()).await },
            move |this, result, cx| {
                if !this.generation_matches(tab_id, generation) {
                    return;
                }
                if let Some(tab_index) = this.tab_index(tab_id)
                    && this.tabs[tab_index].subtitle_loading == Some(index)
                {
                    this.tabs[tab_index].subtitle_loading = None;
                }
                let file = match result {
                    Ok(file) => file,
                    Err(ApiTaskError::Api(error)) => {
                        rigili_monitor::monitor!(
                            "subtitle",
                            "tab {tab_id}: subtitle {index} failed: {error}"
                        );
                        return;
                    }
                    Err(ApiTaskError::Join(error)) => {
                        rigili_monitor::monitor!(
                            "subtitle",
                            "tab {tab_id}: subtitle task failed: {error}"
                        );
                        return;
                    }
                };
                if let Some(tab_index) = this.tab_index(tab_id) {
                    let tab = &mut this.tabs[tab_index];
                    if tab.subtitle_files.len() <= index {
                        tab.subtitle_files.resize(index + 1, None);
                    }
                    tab.subtitle_files[index] = Some(file.clone());
                }
                if this.is_active(tab_id, generation) {
                    this.player
                        .update(cx, |player, cx| player.set_subtitle_file(index, file, cx));
                }
                cx.notify();
            },
        );
        if let Some(tab_index) = self.tab_index(tab_id) {
            self.tabs[tab_index].subtitle_task = Some(task);
        }
    }
}
