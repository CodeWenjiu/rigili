use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::sidebar::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::playback::Playback;
use crate::grid::OpenVideo;
use crate::home::HomeView;
use crate::login;
use crate::playback::PlaybackRequest;
use crate::search::SearchView;

pub struct MainView {
    home: Entity<HomeView>,
    search: Entity<SearchView>,
    search_input: Entity<InputState>,
    search_active: bool,
    playback: Option<Playback>,
    _search_input_sub: Subscription,
}

pub fn open_main(cx: &mut App) -> Result<()> {
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Rigili".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| MainView::new(window, cx))
    })
    .map(|_| ())
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = cx.new(HomeView::new);
        let search = cx.new(SearchView::new);
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索视频"));
        let input_sub = cx.subscribe_in(&search_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.submit_search(cx);
            }
        });
        login::restore(window, cx);

        cx.subscribe_in(&home, window, |this, _, event: &OpenVideo, window, cx| {
            this.play(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&search, window, |this, _, event: &OpenVideo, window, cx| {
            this.play(event.clone(), window, cx);
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
            search,
            search_input,
            search_active: false,
            playback: None,
            _search_input_sub: input_sub,
        }
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        let keyword = self.search_input.read(cx).value().trim().to_owned();
        if keyword.is_empty() {
            self.search_active = false;
            cx.notify();
            return;
        }

        self.search_active = true;
        self.search
            .update(cx, |search, cx| search.search(keyword, cx));
        cx.notify();
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(user) = login::user(cx) {
            return Avatar::new()
                .src(user.face)
                .name(user.uname)
                .size(px(32.))
                .into_any_element();
        }

        Button::new("sign-in")
            .ghost()
            .small()
            .icon(IconName::CircleUser)
            .accessibility_label("登录")
            .tooltip("登录")
            .on_click(cx.listener(|_, _, window, cx| {
                login::open(window, cx);
            }))
            .into_any_element()
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        h_flex()
            .h_16()
            .w_full()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(border)
            .bg(background)
            .text_color(foreground)
            .px_6()
            .child(
                h_flex()
                    .w_full()
                    .max_w(rems(40.))
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search_input)
                                .large()
                                .cleanable(true)
                                .prefix(Icon::new(IconName::Search).text_color(muted)),
                        ),
                    )
                    .child(
                        Button::new("search-submit")
                            .large()
                            .primary()
                            .label("搜索")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_search(cx);
                                this.focus_search(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn play(&mut self, video: OpenVideo, window: &mut Window, cx: &mut Context<Self>) {
        let request = PlaybackRequest {
            title: video.title,
            bvid: video.bvid,
            cid: video.cid,
        };
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
        let weak = cx.entity().downgrade();

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(self.render_top_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        Sidebar::new("app-sidebar")
                            .collapsible(true)
                            .collapsed(true)
                            .border_r_1()
                            .border_color(border)
                            .child(
                                SidebarMenu::new().child(
                                    SidebarMenuItem::new("热门")
                                        .icon(Icon::new(IconName::Flame))
                                        .active(true)
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    if this.search_active {
                                                        this.search_active = false;
                                                        cx.notify();
                                                    }
                                                });
                                            }
                                        }),
                                ),
                            )
                            .footer(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .child(self.account(cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(if self.search_active {
                                self.search.clone().into_any_element()
                            } else {
                                self.home.clone().into_any_element()
                            }),
                    ),
            )
    }
}
