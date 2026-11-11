use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::info::{InfoEvent, InfoPanel};
use super::request::{PlaybackRequest, PlaybackSource, resolve, resolve_streams};
use super::surface::PlayerView;
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::{ApiTaskError, spawn_api};

const INFO_MIN_WINDOW: Pixels = px(1000.);

pub struct PlayerWindow {
    player: Entity<PlayerView>,
    info: Entity<InfoPanel>,
    title: SharedString,
    main_window: Option<AnyWindowHandle>,
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
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        request: PlaybackRequest,
        main_window: Option<AnyWindowHandle>,
    ) -> Self {
        let player = cx.new(|cx| PlayerView::new(window, cx));
        let info = cx.new(|_| InfoPanel::new());
        let info_sub = cx.subscribe(&info, |this, _, event: &InfoEvent, cx| {
            let InfoEvent::SelectPage(cid) = event;
            this.select_page(*cid, cx);
        });
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error)),
        };
        let mut view = Self {
            player,
            info,
            title: request.title.clone().into(),
            main_window,
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

    fn start(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };

        self.generation += 1;
        let generation = self.generation;
        let source = request.source.clone();
        self.is_live = matches!(source, PlaybackSource::Live { .. });
        self.title = request.title.clone().into();
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
                            this.title = resolved.info.title.clone().into();
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
                            this.info
                                .update(cx, |info, cx| info.set(resolved.info, cid, cx));
                            this.player.update(cx, |player, cx| {
                                player.load_urls(
                                    resolved.streams.video,
                                    resolved.streams.audio,
                                    false,
                                    cx,
                                )
                            });
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
        self.error = Some(crate::error::api_task_error_message(error, cx));
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
        let info_loaded = !self.is_live && self.info.read(cx).is_loaded();
        let wide_enough = window.viewport_size().width >= INFO_MIN_WINDOW;
        let show_info = self.show_info && !fullscreen && info_loaded && wide_enough;
        let info_hint = if self.is_live {
            "直播不提供视频信息"
        } else if !info_loaded {
            "视频信息加载中"
        } else if !wide_enough {
            "窗口加宽后可显示视频信息"
        } else if self.show_info {
            "隐藏视频信息"
        } else {
            "显示视频信息"
        };
        let main_window = self.main_window;
        let title = self.title.clone();
        let weak = cx.entity().downgrade();

        if window.window_title() != title.as_ref() {
            window.set_window_title(&title);
        }

        let toggle_info = Button::new("toggle-info")
            .ghost()
            .xsmall()
            .icon(IconName::Info)
            .accessibility_label("视频信息")
            .tooltip(info_hint)
            .selected(self.show_info)
            .disabled(!info_loaded || !wide_enough)
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_info = !this.show_info;
                cx.notify();
            }));

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_3()
                        .when_some(main_window, |this, main| {
                            this.child(
                                Button::new("back-to-main")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ArrowLeft)
                                    .label("回到主界面")
                                    .tooltip("回到主界面")
                                    .on_click(move |_, _, cx| {
                                        let _ = main
                                            .update(cx, |_, window, _| window.activate_window());
                                    }),
                            )
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(muted)
                                .truncate()
                                .child(title),
                        )
                        .when(!fullscreen, |this| this.child(toggle_info)),
                ),
            )
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
                                )
                            })),
                    )
                    .when(show_info, |this| this.child(self.info.clone())),
            )
    }
}

fn overlay(content: impl IntoElement) -> impl IntoElement {
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(content)
}

pub fn open(
    request: PlaybackRequest,
    main_window: Option<AnyWindowHandle>,
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
        cx.new(|cx| PlayerWindow::new(window, cx, request.clone(), main_window))
    })
}
