use gpui_kit::prelude::*;
use rigili_core::api_task_error_message;
use rigili_core::runtime::spawn_api;

use super::PlayerWindow;
use crate::request::PlaybackSource;

impl PlayerWindow {
    pub(super) fn load_follow_status(&self, mid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = self.generation;
        let info = self.info.clone();
        spawn_api(
            cx,
            async move { backend.relation_status(mid).await },
            move |this, result, cx| {
                if this.generation != generation {
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

    pub(super) fn load_interactions(&self, aid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = self.generation;
        let info = self.info.clone();
        spawn_api(
            cx,
            async move { backend.archive_relation(aid).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(relation) = result {
                    info.update(cx, |info, cx| info.set_interactions(relation, cx));
                }
            },
        );
    }

    pub(super) fn load_watch_later(&self, aid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = self.generation;
        let info = self.info.clone();
        spawn_api(
            cx,
            async move { backend.watch_later().await },
            move |this, result, cx| {
                if this.generation != generation {
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
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let Some(PlaybackSource::Video { bvid, .. }) =
            self.request.as_ref().map(|request| request.source.clone())
        else {
            return;
        };
        let generation = self.generation;
        let info = self.info.clone();
        spawn_api(
            cx,
            async move { backend.video_detail(&bvid).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(detail) => info.update(cx, |info, cx| {
                        info.refresh(
                            detail.view,
                            detail.related,
                            detail.card.follower,
                            detail.card.like_num,
                            cx,
                        )
                    }),
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        info.update(cx, |info, cx| info.set_refresh_error(message, cx));
                    }
                }
            },
        );
    }

    pub(super) fn set_follow(&mut self, mid: u64, follow: bool, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let info = self.info.clone();
        info.update(cx, |info, cx| info.set_follow_busy(true, cx));
        spawn_api(
            cx,
            async move { backend.modify_relation(mid, follow).await },
            move |_, result, cx| {
                info.update(cx, |info, cx| match result {
                    Ok(()) => info.set_followed(follow, cx),
                    Err(_) => info.set_follow_busy(false, cx),
                });
            },
        );
    }

    pub(super) fn set_like(&mut self, aid: u64, like: bool, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let info = self.info.clone();
        info.update(cx, |info, cx| info.set_like_busy(true, cx));
        spawn_api(
            cx,
            async move { backend.set_like(aid, like).await },
            move |_, result, cx| {
                info.update(cx, |info, cx| match result {
                    Ok(()) => info.set_liked(like, cx),
                    Err(_) => info.set_like_busy(false, cx),
                });
            },
        );
    }

    pub(super) fn add_coin(&mut self, aid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let info = self.info.clone();
        info.update(cx, |info, cx| info.set_coin_busy(true, cx));
        spawn_api(
            cx,
            async move { backend.add_coin(aid).await },
            move |_, result, cx| {
                info.update(cx, |info, cx| match result {
                    Ok(()) => info.grant_coin(cx),
                    Err(_) => info.set_coin_busy(false, cx),
                });
            },
        );
    }
}
