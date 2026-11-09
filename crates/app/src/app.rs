use gpui_kit::assets::IconName;
use gpui_kit::component::sidebar::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::{HomeView, PlayerView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Player,
}

pub struct MainView {
    home: Entity<HomeView>,
    player: Entity<PlayerView>,
    route: Route,
}

impl MainView {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        initial_video: Option<String>,
        initial_audio: Option<String>,
    ) -> Self {
        let route = if initial_video.is_some() {
            Route::Player
        } else {
            Route::Home
        };
        let home = cx.new(HomeView::new);
        let player = cx.new(|cx| PlayerView::new(window, cx, initial_video, initial_audio));

        Self {
            home,
            player,
            route,
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
        let weak = cx.entity().downgrade();
        let route = self.route;
        let home = self.home.clone();
        let player = self.player.clone();

        h_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(
                Sidebar::new("app-sidebar")
                    .w(px(200.))
                    .collapsible(false)
                    .border_r_1()
                    .border_color(border)
                    .header(
                        v_flex()
                            .px_3()
                            .py_4()
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
                        SidebarMenu::new()
                            .child(nav_item(
                                "热门",
                                IconName::Flame,
                                route == Route::Home,
                                Route::Home,
                                weak.clone(),
                            ))
                            .child(nav_item(
                                "播放器",
                                IconName::Play,
                                route == Route::Player,
                                Route::Player,
                                weak,
                            )),
                    ),
            )
            .child(div().flex_1().min_w_0().h_full().child(match route {
                Route::Home => home.into_any_element(),
                Route::Player => player.into_any_element(),
            }))
    }
}

fn nav_item(
    label: &'static str,
    icon: IconName,
    active: bool,
    route: Route,
    weak: WeakEntity<MainView>,
) -> SidebarMenuItem {
    SidebarMenuItem::new(label)
        .icon(Icon::new(icon))
        .active(active)
        .on_click(move |_, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.route = route;
                cx.notify();
            });
        })
}
