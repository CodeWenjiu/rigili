rigili_macro::mod_prv!(info, media);

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::history::History;
use super::surface::PlayerView;
use crate::info::{InfoEvent, InfoPanel, InfoSelections, InfoTab};
use crate::request::{PlaybackRequest, PlaybackSource, Streams, resolve, resolve_streams};
use crate::shortcuts::{
    HistoryBack, HistoryForward, Reload, SeekBack, SpeedDown, SpeedUp, SwitchToMain,
    ToggleComments, ToggleDanmaku, ToggleFullscreen, ToggleInfo, ToggleMute, TogglePlayback,
};
use rigili_bilibili::Client;
use rigili_core::api_error_message;
use rigili_core::client;
use rigili_core::runtime::{ApiTaskError, spawn_api};
use rigili_core::{FocusMain, OpenSpace};

const INFO_MIN_WINDOW: Pixels = px(1000.);

pub struct PlayerWindow {
    player: Entity<PlayerView>,
    info: Entity<InfoPanel>,
    title: SharedString,
    client: Option<Arc<Client>>,
    request: Option<PlaybackRequest>,
    history: History,
    loading: bool,
    is_live: bool,
    error: Option<String>,
    show_info: bool,
    fullscreen_info: bool,
    generation: u64,
    danmaku_task: Option<Task<()>>,
    _info_sub: Subscription,
}

impl EventEmitter<OpenSpace> for PlayerWindow {}
impl EventEmitter<FocusMain> for PlayerWindow {}

impl PlayerWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>, request: PlaybackRequest) -> Self {
        let player = cx.new(|cx| PlayerView::new(window, cx));
        let selections = InfoSelections::new(window, cx);
        let info = cx.new(|_| InfoPanel::new(selections));
        let info_sub = cx.subscribe(&info, |this, _, event: &InfoEvent, cx| match event {
            InfoEvent::SelectPage(cid) => this.select_page(*cid, cx),
            InfoEvent::PlayRelated { bvid, cid, title } => this.navigate(
                PlaybackRequest::video(title.clone(), bvid.clone(), Some(*cid)),
                cx,
            ),
            InfoEvent::ToggleFollow { mid, follow } => this.set_follow(*mid, *follow, cx),
            InfoEvent::ToggleLike { aid, like } => this.set_like(*aid, *like, cx),
            InfoEvent::OpenSpace { mid, name } => cx.emit(OpenSpace {
                mid: *mid,
                name: name.clone(),
            }),
            InfoEvent::Reload => this.refresh_info(cx),
        });
        let (client, error) = match client::get(cx) {
            Ok(client) => (Some(client), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            player,
            info,
            title: request.title.clone().into(),
            client,
            request: None,
            history: History::default(),
            loading: false,
            is_live: false,
            error,
            show_info: true,
            fullscreen_info: false,
            generation: 0,
            danmaku_task: None,
            _info_sub: info_sub,
        };
        if view.client.is_some() {
            view.history.open(request.source.clone());
            view.start(request, cx);
        }
        view
    }

    pub fn play(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.navigate(request, cx);
    }

    fn navigate(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.history.open(request.source.clone());
        self.start(request, cx);
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        if let Some(source) = self.history.back() {
            self.start_source(source, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        if let Some(source) = self.history.forward() {
            self.start_source(source, cx);
        }
    }

    fn start_source(&mut self, source: PlaybackSource, cx: &mut Context<Self>) {
        let request = match source {
            PlaybackSource::Video { bvid, cid } => PlaybackRequest::video(String::new(), bvid, cid),
            PlaybackSource::Live { room_id } => {
                PlaybackRequest::live(format!("直播间 {room_id}"), room_id)
            }
        };
        self.start(request, cx);
    }

    pub(crate) fn toggle_panel(&mut self, tab: InfoTab, window: &Window, cx: &mut Context<Self>) {
        let current = self.info.read(cx).tab();
        let visible = if window.is_fullscreen() {
            &mut self.fullscreen_info
        } else {
            &mut self.show_info
        };
        *visible = panel_visibility(*visible, current, tab);
        if *visible {
            self.info.update(cx, |info, cx| info.show_tab(tab, cx));
        }
        cx.notify();
    }

    fn start(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };

        self.generation += 1;
        self.danmaku_task = None;
        self.player
            .update(cx, |player, cx| player.clear_danmaku_source(cx));
        let generation = self.generation;
        let source = request.source.clone();
        let preferred = crate::settings::current(cx).preferred_quality;
        self.is_live = matches!(source, PlaybackSource::Live { .. });
        if !request.title.is_empty() {
            self.title = request.title.clone().into();
            let title = self.title.clone();
            self.player
                .update(cx, |player, cx| player.set_title(title, cx));
        }
        self.request = Some(request);
        self.loading = true;
        self.error = None;
        self.info.update(cx, |info, cx| info.clear(cx));
        self.info.update(cx, |info, cx| info.set_busy(true, cx));
        self.player
            .update(cx, |player, cx| player.clear_danmaku(cx));
        cx.notify();

        match source {
            PlaybackSource::Video { bvid, cid } => spawn_api(
                cx,
                async move { resolve(client, bvid, cid, preferred).await },
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
                            let duration = resolved
                                .info
                                .pages
                                .iter()
                                .find(|page| page.cid == cid)
                                .map(|page| page.duration)
                                .unwrap_or(resolved.info.duration);
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
                                player.load_streams(resolved.streams, false, cx)
                            });
                            this.load_previews(bvid.clone(), cid, cx);
                            this.load_chapters(bvid.clone(), cid, cx);
                            this.load_danmaku(bvid, aid, cid, duration, cx);
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
                async move { client.live_play_url(room_id).await },
                move |this, result, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.loading = false;
                    match result {
                        Ok(url) => {
                            this.info.update(cx, |info, cx| info.clear(cx));
                            this.player.update(cx, |player, cx| {
                                player.load_streams(Streams::single(url), true, cx)
                            });
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
        let Some(client) = self.client.clone() else {
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
        self.danmaku_task = None;
        self.player
            .update(cx, |player, cx| player.clear_danmaku_source(cx));
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        self.info.update(cx, |info, cx| info.set_busy(true, cx));
        cx.notify();

        let preview_bvid = bvid.clone();
        let chapters_bvid = bvid.clone();
        let danmaku_bvid = bvid.clone();
        let preferred = crate::settings::current(cx).preferred_quality;
        let aid = self.info.read(cx).aid().unwrap_or_default();
        let duration = self.info.read(cx).page_duration(cid).unwrap_or_default();
        spawn_api(
            cx,
            async move { resolve_streams(&client, &bvid, cid, preferred).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(streams) => {
                        this.history.remember_cid(cid);
                        this.info.update(cx, |info, cx| info.select(cid, cx));
                        this.player
                            .update(cx, |player, cx| player.load_streams(streams, false, cx));
                        this.load_previews(preview_bvid, cid, cx);
                        this.load_chapters(chapters_bvid, cid, cx);
                        this.load_danmaku(danmaku_bvid, aid, cid, duration, cx);
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

    fn handle_error(&mut self, error: ApiTaskError, cx: &mut Context<Self>) {
        self.error = Some(rigili_core::api_task_error_message(error, cx));
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.request.clone() {
            self.start(request, cx);
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(source) = self.request.as_ref().map(|request| request.source.clone()) else {
            return;
        };
        if matches!(source, PlaybackSource::Live { .. }) {
            if let Some(request) = self.request.clone() {
                self.start(request, cx);
            }
            return;
        }
        self.info.update(cx, |info, cx| info.reload_current(cx));
        self.reload_stream(cx);
    }

    fn reload_stream(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(PlaybackSource::Video { bvid, cid }) =
            self.request.as_ref().map(|request| request.source.clone())
        else {
            return;
        };
        let Some(cid) = cid else {
            return;
        };
        let preferred = self.player.read(cx).selected_quality();
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { resolve_streams(&client, &bvid, cid, Some(preferred)).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(streams) => this
                        .player
                        .update(cx, |player, cx| player.reload_streams(streams, cx)),
                    Err(error) => this.handle_error(error, cx),
                }
                cx.notify();
            },
        );
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
        let wide_enough = window.viewport_size().width >= INFO_MIN_WINDOW;
        let panel_open = if fullscreen {
            self.fullscreen_info
        } else {
            self.show_info
        };
        let show_info = panel_open && !self.is_live && (fullscreen || wide_enough);
        let title = self.title.clone();
        let weak = cx.entity().downgrade();
        let can_go_back = self.history.can_back();
        let can_go_forward = self.history.can_forward();

        if window.window_title() != title.as_ref() {
            window.set_window_title(&title);
        }

        v_flex()
            .image_cache(rigili_core::image_cache::get(cx))
            .id("player-window")
            .size_full()
            .bg(background)
            .text_color(foreground)
            .key_context(crate::shortcuts::PLAYER)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.info.update(cx, |info, cx| {
                        info.clear_spanning_selection(window, cx);
                    });
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_pause(cx));
            }))
            .on_action(cx.listener(|this, _: &SeekBack, _, cx| {
                this.player.update(cx, |player, cx| player.seek_by(-5., cx));
            }))
            .on_action(cx.listener(|this, _: &HistoryBack, _, cx| {
                this.go_back(cx);
            }))
            .on_action(cx.listener(|this, _: &HistoryForward, _, cx| {
                this.go_forward(cx);
            }))
            .on_action(cx.listener(|this, _: &SpeedDown, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(false, cx));
            }))
            .on_action(cx.listener(|this, _: &SpeedUp, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(true, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_mute(cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_fullscreen(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleInfo, window, cx| {
                this.toggle_panel(InfoTab::Info, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleComments, window, cx| {
                this.toggle_panel(InfoTab::Comments, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleDanmaku, _, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_danmaku(cx));
            }))
            .on_action(cx.listener(|this, _: &Reload, _, cx| {
                this.reload(cx);
            }))
            .on_action(cx.listener(|_, _: &SwitchToMain, _, cx| {
                cx.emit(FocusMain);
            }))
            .when(!fullscreen, |this| {
                this.child(
                    TitleBar::new().child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .pr_6()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("player-back")
                                            .ghost()
                                            .icon(IconName::ChevronLeft)
                                            .accessibility_label("上一个视频")
                                            .tooltip("上一个视频")
                                            .disabled(!can_go_back)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_back(cx)),
                                            ),
                                    )
                                    .child(
                                        Button::new("player-forward")
                                            .ghost()
                                            .icon(IconName::ChevronRight)
                                            .accessibility_label("下一个视频")
                                            .tooltip("下一个视频")
                                            .disabled(!can_go_forward)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_forward(cx)),
                                            ),
                                    ),
                            )
                            .when(!client_decorated, |this| {
                                this.child(
                                    Button::new("window-close")
                                        .ghost()
                                        .icon(IconName::Close)
                                        .accessibility_label("关闭窗口")
                                        .tooltip("关闭")
                                        .on_click(|_, window, _| window.remove_window()),
                                )
                            }),
                    ),
                )
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
    options.app_id = Some("rigili".into());
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
        assert!(!panel_visibility(true, InfoTab::Info, InfoTab::Info));
        assert!(panel_visibility(true, InfoTab::Comments, InfoTab::Info));
        assert!(!panel_visibility(
            true,
            InfoTab::Comments,
            InfoTab::Comments
        ));
    }
}
