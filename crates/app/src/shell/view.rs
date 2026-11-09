use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::sidebar::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::playback::Playback;
use crate::home::{HomeEvent, HomeView};
use crate::login;
use crate::playback::PlaybackRequest;

pub struct MainView {
    home: Entity<HomeView>,
    playback: Option<Playback>,
}

pub fn open_main(cx: &mut App) -> Result<()> {
    gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        cx.new(|cx| MainView::new(window, cx))
    })
    .map(|_| ())
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = cx.new(HomeView::new);
        login::restore(window, cx);

        cx.subscribe_in(&home, window, |this, _, event: &HomeEvent, window, cx| {
            let HomeEvent::Open { bvid, cid, title } = event;
            this.play(
                PlaybackRequest {
                    title: title.clone(),
                    bvid: bvid.clone(),
                    cid: *cid,
                },
                window,
                cx,
            );
        })
        .detach();

        cx.on_window_closed({
            let weak = cx.entity().downgrade();
            move |cx, window_id| {
                let _ = weak.update(cx, |this, cx| {
                    let closed = this
                        .playback
                        .as_ref()
                        .is_some_and(|playback| playback.is_for(window_id));
                    if closed {
                        this.playback = None;
                        cx.notify();
                    }
                });
            }
        })
        .detach();

        Self {
            home,
            playback: None,
        }
    }

    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(user) = login::user(cx) {
            return h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(
                    Avatar::new()
                        .src(user.face.clone())
                        .name(user.uname.clone())
                        .xsmall(),
                )
                .child(div().text_sm().truncate().child(user.uname.clone()))
                .into_any_element();
        }

        Button::new("sign-in")
            .ghost()
            .icon(IconName::CircleUser)
            .label("登录")
            .on_click(cx.listener(|_, _, window, cx| {
                login::open(window, cx);
            }))
            .into_any_element()
    }

    fn play(&mut self, request: PlaybackRequest, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(playback) = self.playback.as_ref() {
            if playback.load(request.clone(), cx).is_ok() {
                return;
            }
            self.playback = None;
        }

        let main_window = window.window_handle();
        match Playback::open(request, main_window, cx) {
            Ok(playback) => self.playback = Some(playback),
            Err(error) => eprintln!("failed to open player window: {error}"),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;

        h_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(
                Sidebar::new("app-sidebar")
                    .w(px(160.))
                    .collapsible(false)
                    .border_r_1()
                    .border_color(border)
                    .header(
                        v_flex()
                            .py_2()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child("gpuili"),
                            )
                            .child(div().text_xs().text_color(muted).child("哔哩哔哩")),
                    )
                    .child(
                        SidebarMenu::new().child(
                            SidebarMenuItem::new("热门")
                                .icon(Icon::new(IconName::Flame))
                                .active(true),
                        ),
                    )
                    .footer(self.account(cx)),
            )
            .child(div().flex_1().min_w_0().h_full().child(self.home.clone()))
    }
}
