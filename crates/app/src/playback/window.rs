use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::request::{PlaybackRequest, PlaybackSource, resolve};
use super::surface::PlayerView;
use crate::backend::Backend;
use crate::platform::runtime::Tokio;

pub struct PlayerWindow {
    player: Entity<PlayerView>,
    title: SharedString,
    main_window: Option<AnyWindowHandle>,
    backend: Option<Backend>,
    request: Option<PlaybackRequest>,
    loading: bool,
    error: Option<String>,
}

impl PlayerWindow {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        request: PlaybackRequest,
        main_window: Option<AnyWindowHandle>,
    ) -> Self {
        let player = cx.new(|cx| PlayerView::new(window, cx));
        let (backend, error) = match Backend::new() {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let mut view = Self {
            player,
            title: request.title.clone().into(),
            main_window,
            backend,
            request: None,
            loading: false,
            error,
        };
        if view.backend.is_some() {
            view.start(request, cx);
        }
        view
    }

    pub fn play(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.title = request.title.clone().into();
        self.start(request, cx);
    }

    fn start(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.error = None;

        let (bvid, cid) = match &request.source {
            PlaybackSource::Video { bvid, cid } => (bvid.clone(), *cid),
            PlaybackSource::Stream { video, audio } => {
                let (video, audio) = (video.clone(), audio.clone());
                self.loading = false;
                self.request = None;
                self.player
                    .update(cx, |player, cx| player.load_urls(video, audio, cx));
                cx.notify();
                return;
            }
        };

        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.request = Some(request);
        self.loading = true;
        cx.notify();

        let task = Tokio::spawn(&*cx, async move { resolve(backend, bvid, cid).await });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(Ok((title, streams))) => {
                        if let Some(title) = title {
                            this.title = title.into();
                        }
                        this.player.update(cx, |player, cx| {
                            player.load_urls(streams.video, streams.audio, cx)
                        });
                    }
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(error) => this.error = Some(format!("加载任务失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
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
        let main_window = self.main_window;
        let title = self.title.clone();
        let weak = cx.entity().downgrade();

        if window.window_title() != title.as_ref() {
            window.set_window_title(&title);
        }

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
                        .child(div().text_sm().text_color(muted).truncate().child(title)),
                ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.player.clone())
                    .when(self.loading, |this| {
                        this.child(overlay(
                            v_flex()
                                .items_center()
                                .gap_3()
                                .child(Spinner::new().large())
                                .child(div().text_sm().text_color(muted).child("正在获取视频流…")),
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
                                                let _ = weak.update(cx, |this, cx| this.retry(cx));
                                            }
                                        }),
                                ),
                        )
                    })),
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
