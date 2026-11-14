use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::info::{InfoEvent, InfoPanel, InfoTab};
use super::request::{PlaybackRequest, PlaybackSource, resolve, resolve_streams};
use super::surface::PlayerView;
use crate::shortcuts::{
    Reload, SeekBack, SeekForward, SpeedDown, SpeedUp, ToggleComments, ToggleFullscreen,
    ToggleInfo, ToggleMute, TogglePlayback,
};
use rigili_core::api_error_message;
use rigili_core::api_task_error_message;
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::{ApiTaskError, spawn_api};

const INFO_MIN_WINDOW: Pixels = px(1000.);

pub struct PlayerWindow {
    player: Entity<PlayerView>,
    info: Entity<InfoPanel>,
    title: SharedString,
    backend: Option<Backend>,
    request: Option<PlaybackRequest>,
    loading: bool,
    is_live: bool,
    error: Option<String>,
    show_info: bool,
    generation: u64,
    _info_sub: Subscription,
}

impl PlayerWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>, request: PlaybackRequest) -> Self {
        let player = cx.new(|cx| PlayerView::new(window, cx));
        let info = cx.new(|_| InfoPanel::new());
        let info_sub = cx.subscribe(&info, |this, _, event: &InfoEvent, cx| match event {
            InfoEvent::SelectPage(cid) => this.select_page(*cid, cx),
            InfoEvent::PlayRelated { bvid, cid, title } => this.start(
                PlaybackRequest::video(title.clone(), bvid.clone(), Some(*cid)),
                cx,
            ),
            InfoEvent::ToggleFollow { mid, follow } => this.set_follow(*mid, *follow, cx),
            InfoEvent::ToggleLike { aid, like } => this.set_like(*aid, *like, cx),
            InfoEvent::AddCoin { aid } => this.add_coin(*aid, cx),
            InfoEvent::Reload => this.refresh_info(cx),
        });
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            player,
            info,
            title: request.title.clone().into(),
            backend,
            request: None,
            loading: false,
            is_live: false,
            error,
            show_info: true,
            generation: 0,
            _info_sub: info_sub,
        };
        if view.backend.is_some() {
            view.start(request, cx);
        }
        view
    }

    pub fn play(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.start(request, cx);
    }

    pub(crate) fn toggle_panel(&mut self, tab: InfoTab, cx: &mut Context<Self>) {
        let current = self.info.read(cx).tab();
        self.show_info = panel_visibility(self.show_info, current, tab);
        if self.show_info {
            self.info.update(cx, |info, cx| info.show_tab(tab, cx));
        }
        cx.notify();
    }

    fn start(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };

        self.generation += 1;
        let generation = self.generation;
        let source = request.source.clone();
        self.is_live = matches!(source, PlaybackSource::Live { .. });
        self.title = request.title.clone().into();
        let title = self.title.clone();
        self.player
            .update(cx, |player, cx| player.set_title(title, cx));
        self.request = Some(request);
        self.loading = true;
        self.error = None;
        self.info.update(cx, |info, cx| info.clear(cx));
        self.info.update(cx, |info, cx| info.set_busy(true, cx));
        cx.notify();

        match source {
            PlaybackSource::Video { bvid, cid } => spawn_api(
                cx,
                async move { resolve(backend, bvid, cid).await },
                move |this, result, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.loading = false;
                    match result {
                        Ok(resolved) => {
                            let cid = resolved.cid;
                            let bvid = resolved.info.bvid.clone();
                            let aid = resolved.info.aid;
                            let owner_mid = resolved.info.owner.mid;
                            let followers = resolved.followers;
                            let owner_likes = resolved.owner_likes;
                            let related = resolved.related;
                            let title: SharedString = resolved.info.title.clone().into();
                            this.title = title.clone();
                            this.player
                                .update(cx, |player, cx| player.set_title(title, cx));
                            if let Some(PlaybackRequest {
                                source:
                                    PlaybackSource::Video {
                                        cid: request_cid, ..
                                    },
                                ..
                            }) = this.request.as_mut()
                            {
                                *request_cid = Some(cid);
                            }
                            this.info.update(cx, |info, cx| {
                                info.set(resolved.info, cid, cx);
                                info.set_owner_counts(followers, owner_likes, cx);
                                info.set_related(related, cx);
                            });
                            this.player.update(cx, |player, cx| {
                                player.load_urls(
                                    resolved.streams.video,
                                    resolved.streams.audio,
                                    false,
                                    cx,
                                )
                            });
                            this.load_previews(bvid.clone(), cid, cx);
                            this.load_chapters(bvid.clone(), cid, cx);
                            this.load_follow_status(owner_mid, cx);
                            this.load_interactions(aid, cx);
                            this.load_watch_later(aid, cx);
                        }
                        Err(error) => {
                            this.info.update(cx, |info, cx| info.set_busy(false, cx));
                            this.handle_error(error, cx);
                        }
                    }
                    cx.notify();
                },
            ),
            PlaybackSource::Live { room_id } => spawn_api(
                cx,
                async move { backend.live_play_url(room_id).await },
                move |this, result, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.loading = false;
                    match result {
                        Ok(url) => {
                            this.info.update(cx, |info, cx| info.clear(cx));
                            this.player
                                .update(cx, |player, cx| player.load_urls(url, None, true, cx));
                        }
                        Err(error) => this.handle_error(error, cx),
                    }
                    cx.notify();
                },
            ),
        }
    }

    fn select_page(&mut self, cid: u64, cx: &mut Context<Self>) {
        if self.is_live || self.loading {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let Some(PlaybackSource::Video {
            bvid,
            cid: current_cid,
        }) = self.request.as_ref().map(|request| request.source.clone())
        else {
            return;
        };
        if current_cid == Some(cid) {
            return;
        }

        if let Some(PlaybackRequest {
            source: PlaybackSource::Video {
                cid: request_cid, ..
            },
            ..
        }) = self.request.as_mut()
        {
            *request_cid = Some(cid);
        }
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        self.info.update(cx, |info, cx| info.set_busy(true, cx));
        cx.notify();

        let preview_bvid = bvid.clone();
        let chapters_bvid = bvid.clone();
        spawn_api(
            cx,
            async move { resolve_streams(&backend, &bvid, cid).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(streams) => {
                        this.info.update(cx, |info, cx| info.select(cid, cx));
                        this.player.update(cx, |player, cx| {
                            player.load_urls(streams.video, streams.audio, false, cx)
                        });
                        this.load_previews(preview_bvid, cid, cx);
                        this.load_chapters(chapters_bvid, cid, cx);
                    }
                    Err(error) => {
                        this.info.update(cx, |info, cx| info.set_busy(false, cx));
                        this.handle_error(error, cx);
                    }
                }
                cx.notify();
            },
        );
    }

    fn load_follow_status(&self, mid: u64, cx: &mut Context<Self>) {
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

    fn load_interactions(&self, aid: u64, cx: &mut Context<Self>) {
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

    fn load_watch_later(&self, aid: u64, cx: &mut Context<Self>) {
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

    fn refresh_info(&mut self, cx: &mut Context<Self>) {
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

    fn set_follow(&mut self, mid: u64, follow: bool, cx: &mut Context<Self>) {
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

    fn set_like(&mut self, aid: u64, like: bool, cx: &mut Context<Self>) {
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

    fn add_coin(&mut self, aid: u64, cx: &mut Context<Self>) {
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

    fn load_previews(&self, bvid: String, cid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = self.generation;
        spawn_api(
            cx,
            async move { backend.videoshot(&bvid, cid).await },
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

    fn load_chapters(&self, bvid: String, cid: u64, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let generation = self.generation;
        spawn_api(
            cx,
            async move { backend.chapters(&bvid, cid).await },
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

    fn handle_error(&mut self, error: ApiTaskError, cx: &mut Context<Self>) {
        self.error = Some(rigili_core::api_task_error_message(error, cx));
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.request.clone() {
            self.start(request, cx);
        }
    }
}

impl Render for PlayerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let fullscreen = window.is_fullscreen();
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let info_loaded = !self.is_live && self.info.read(cx).is_loaded();
        let wide_enough = window.viewport_size().width >= INFO_MIN_WINDOW;
        let show_info = self.show_info && !fullscreen && info_loaded && wide_enough;
        let title = self.title.clone();
        let weak = cx.entity().downgrade();

        if window.window_title() != title.as_ref() {
            window.set_window_title(&title);
        }

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .key_context(crate::shortcuts::PLAYER)
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_pause(cx));
            }))
            .on_action(cx.listener(|this, _: &SeekBack, _, cx| {
                this.player.update(cx, |player, cx| player.seek_by(-5., cx));
            }))
            .on_action(cx.listener(|this, _: &SeekForward, _, cx| {
                this.player.update(cx, |player, cx| player.seek_by(5., cx));
            }))
            .on_action(cx.listener(|this, _: &SpeedDown, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(false, cx));
            }))
            .on_action(cx.listener(|this, _: &SpeedUp, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(true, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleMute, window, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_mute(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_fullscreen(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleInfo, _, cx| {
                this.toggle_panel(InfoTab::Info, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleComments, _, cx| {
                this.toggle_panel(InfoTab::Comments, cx);
            }))
            .on_action(cx.listener(|this, _: &Reload, _, cx| {
                this.info.update(cx, |info, cx| info.reload_current(cx));
            }))
            .when(!fullscreen, |this| {
                this.child(TitleBar::new().child(
                    h_flex().w_full().items_center().justify_end().pr_6().when(
                        !client_decorated,
                        |this| {
                            this.child(
                                Button::new("window-close")
                                    .ghost()
                                    .icon(IconName::Close)
                                    .accessibility_label("关闭窗口")
                                    .tooltip("关闭")
                                    .on_click(|_, window, _| window.remove_window()),
                            )
                        },
                    ),
                ))
            })
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.player.clone())
                            .when(self.loading, |this| {
                                this.child(overlay(
                                    v_flex()
                                        .items_center()
                                        .gap_3()
                                        .child(Spinner::new().large())
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(muted)
                                                .child("正在获取视频流…"),
                                        ),
                                    cx,
                                ))
                            })
                            .children(self.error.clone().map(|error| {
                                overlay(
                                    v_flex()
                                        .items_center()
                                        .gap_3()
                                        .child(div().text_sm().text_color(danger).child(error))
                                        .child(
                                            Button::new("retry-playback")
                                                .primary()
                                                .label("重试")
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak
                                                            .update(cx, |this, cx| this.retry(cx));
                                                    }
                                                }),
                                        ),
                                    cx,
                                )
                            })),
                    )
                    .when(show_info, |this| this.child(self.info.clone())),
            )
    }
}

fn overlay(content: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .p_6()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .child(content),
        )
}

fn panel_visibility(visible: bool, current: InfoTab, target: InfoTab) -> bool {
    !(visible && current == target)
}

pub fn open(
    request: PlaybackRequest,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<PlayerWindow>)> {
    let title = request.title.clone();
    let mut options = TitleBar::window_options();
    options.titlebar = Some(TitlebarOptions {
        title: Some(title.into()),
        ..TitleBar::title_bar_options()
    });
    options.window_bounds = Some(WindowBounds::Windowed(Bounds::new(
        point(px(96.), px(96.)),
        size(px(1280.), px(800.)),
    )));
    options.window_min_size = Some(size(px(640.), px(360.)));

    gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| PlayerWindow::new(window, cx, request.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::{InfoTab, panel_visibility};

    #[test]
    fn toggles_only_the_active_panel_tab() {
        assert!(panel_visibility(false, InfoTab::Info, InfoTab::Info));
        assert!(panel_visibility(false, InfoTab::Comments, InfoTab::Info));
        assert!(!panel_visibility(true, InfoTab::Info, InfoTab::Info));
        assert!(panel_visibility(true, InfoTab::Comments, InfoTab::Info));

        assert!(panel_visibility(false, InfoTab::Info, InfoTab::Comments));
        assert!(panel_visibility(
            false,
            InfoTab::Comments,
            InfoTab::Comments
        ));
        assert!(panel_visibility(true, InfoTab::Info, InfoTab::Comments));
        assert!(!panel_visibility(
            true,
            InfoTab::Comments,
            InfoTab::Comments
        ));
    }
}
