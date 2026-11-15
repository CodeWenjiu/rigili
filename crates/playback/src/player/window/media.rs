use gpui_kit::prelude::*;
use rigili_core::runtime::spawn_api;

use super::PlayerWindow;

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
