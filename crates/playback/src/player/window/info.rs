use gpui_kit::prelude::*;
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;

use super::PlayerWindow;
use crate::request::PlaybackSource;

impl PlayerWindow {
    pub(super) fn load_follow_status_for(
        &self,
        tab_id: u64,
        generation: u64,
        mid: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(info) = self.tab_info(tab_id) else {
            return;
        };
        spawn_api(
            cx,
            async move { client.relation_status(mid).await },
            move |this, result, cx| {
                if !this.generation_matches(tab_id, generation) {
                    return;
                }
                if let Ok(status) = result {
                    info.update(cx, |info, cx| {
                        info.set_follow_status(status.is_followed(), cx)
                    });
                }
            },
        );
    }

    pub(super) fn load_interactions_for(
        &self,
        tab_id: u64,
        generation: u64,
        aid: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(info) = self.tab_info(tab_id) else {
            return;
        };
        spawn_api(
            cx,
            async move { client.archive_relation(aid).await },
            move |this, result, cx| {
                if !this.generation_matches(tab_id, generation) {
                    return;
                }
                if let Ok(relation) = result {
                    info.update(cx, |info, cx| info.set_interactions(relation, cx));
                }
            },
        );
    }

    pub(super) fn load_watch_later_for(
        &self,
        tab_id: u64,
        generation: u64,
        aid: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(info) = self.tab_info(tab_id) else {
            return;
        };
        spawn_api(
            cx,
            async move { client.watch_later().await },
            move |this, result, cx| {
                if !this.generation_matches(tab_id, generation) {
                    return;
                }
                if let Ok(later) = result {
                    let added = later.into_items().iter().any(|item| item.aid == aid);
                    info.update(cx, |info, cx| info.set_watch_later(added, cx));
                }
            },
        );
    }

    pub(super) fn refresh_info(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let (tab_id, generation, bvid, cid, info) = {
            let tab = self.tab();
            let PlaybackSource::Video { bvid, cid } = tab.request.source.clone() else {
                return;
            };
            (tab.id, tab.generation, bvid, cid, tab.info.clone())
        };
        spawn_api(
            cx,
            async move { client.video_detail(&bvid).await },
            move |this, result, cx| {
                if !this.generation_matches(tab_id, generation) {
                    return;
                }
                match result {
                    Ok(detail) => {
                        let pages = detail.view.pages.clone();
                        info.update(cx, |info, cx| {
                            info.refresh(
                                detail.view,
                                detail.related,
                                detail.card.follower,
                                detail.card.like_num,
                                cx,
                            )
                        });
                        if let Some(index) = this.tab_index(tab_id) {
                            this.tabs[index].pages = pages.clone();
                        }
                        if this.is_active(tab_id, generation)
                            && let Some(cid) = cid
                        {
                            this.player
                                .update(cx, |player, cx| player.set_pages(pages, cid, cx));
                        }
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        info.update(cx, |info, cx| info.set_refresh_error(message, cx));
                    }
                }
            },
        );
    }

    pub(super) fn set_follow(&mut self, mid: u64, follow: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let info = self.tab().info.clone();
        info.update(cx, |info, cx| info.set_follow_busy(true, cx));
        spawn_api(
            cx,
            async move { client.modify_relation(mid, follow).await },
            move |_, result, cx| {
                info.update(cx, |info, cx| match result {
                    Ok(()) => info.set_followed(follow, cx),
                    Err(_) => info.set_follow_busy(false, cx),
                });
            },
        );
    }

    pub(super) fn set_like(&mut self, aid: u64, like: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let info = self.tab().info.clone();
        info.update(cx, |info, cx| info.set_like_busy(true, cx));
        spawn_api(
            cx,
            async move { client.set_like(aid, like).await },
            move |_, result, cx| {
                info.update(cx, |info, cx| match result {
                    Ok(()) => info.set_liked(like, cx),
                    Err(_) => info.set_like_busy(false, cx),
                });
            },
        );
    }
}
