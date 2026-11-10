use std::rc::Rc;

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
use crate::home::{HomeTab, HomeView, OpenLive};
use crate::login;
use crate::playback::PlaybackRequest;
use crate::refresh::Refresh;
use crate::search::SearchView;

pub struct MainView {
    home: Entity<HomeView>,
    search: Entity<SearchView>,
    search_input: Entity<InputState>,
    search_active: bool,
    playback: Option<Playback>,
    _search_input_sub: Subscription,
}

type NavHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone)]
struct NavItem {
    label: SharedString,
    icon: IconName,
    active: bool,
    on_click: NavHandler,
}

impl NavItem {
    fn new(label: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            label: label.into(),
            icon,
            active: false,
            on_click: Rc::new(|_: &ClickEvent, _: &mut Window, _: &mut App| {}),
        }
    }

    fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    fn on_click(mut self, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Rc::new(on_click);
        self
    }
}

impl Collapsible for NavItem {
    fn is_collapsed(&self) -> bool {
        false
    }

    fn collapsed(self, _: bool) -> Self {
        self
    }
}

impl SidebarItem for NavItem {
    fn render(self, id: impl Into<ElementId>, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let active = self.active;
        let icon = self.icon;
        let label = self.label;
        let on_click = self.on_click;
        let color = if active {
            cx.theme().sidebar_accent_foreground
        } else {
            cx.theme().sidebar_foreground
        };

        Button::new(id)
            .ghost()
            .w_full()
            .h(px(60.))
            .accessibility_label(label.clone())
            .on_click(move |event, window, cx| on_click(event, window, cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .text_color(color)
                    .child(Icon::new(icon).with_size(px(22.)))
                    .child(div().text_sm().child(label)),
            )
    }
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
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索视频、番剧、用户"));
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

        cx.subscribe_in(&home, window, |this, _, event: &OpenLive, window, cx| {
            this.play_live(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&search, window, |this, _, event: &OpenVideo, window, cx| {
            this.play(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&search, window, |this, _, event: &OpenLive, window, cx| {
            this.play_live(event.clone(), window, cx);
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

    fn render_logo(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h(px(40.))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::default()
                    .data(include_bytes!("../../assets/rigili-mark.svg"))
                    .with_size(px(36.))
                    .text_color(cx.theme().sidebar_primary),
            )
            .into_any_element()
    }

    fn render_home_tabs(&self, selected: HomeTab, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .children(HomeTab::ALL.map(|tab| {
                let active = tab == selected;
                let group = SharedString::from(format!("top-tab-{}", tab.index()));

                v_flex()
                    .group(group.clone())
                    .items_center()
                    .gap_1()
                    .rounded(radius)
                    .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                    .child(
                        Button::new(("top-tab", tab.index()))
                            .text()
                            .px_2()
                            .py_1()
                            .accessibility_label(tab.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.search_active = false;
                                this.home.update(cx, |home, cx| home.show_tab(tab, cx));
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { primary } else { muted })
                                    .group_hover(group, move |this| this.text_color(primary))
                                    .child(tab.label()),
                            ),
                    )
                    .child(div().h(px(2.)).w(px(24.)).rounded(px(1.)).bg(if active {
                        primary
                    } else {
                        hsla(0., 0., 0., 0.)
                    }))
            }))
            .into_any_element()
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let home_tab = self.home.read(cx).current_tab();

        h_flex()
            .h_16()
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(border)
            .bg(background)
            .text_color(foreground)
            .px_6()
            .child(self.render_home_tabs(home_tab, cx))
            .child(
                div().flex_1().min_w_0().flex().justify_center().child(
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
                ),
            )
            .into_any_element()
    }

    fn page_can_refresh(&self, cx: &App) -> bool {
        if self.search_active {
            self.search.read(cx).can_refresh()
        } else {
            self.home.read(cx).can_refresh()
        }
    }

    fn refresh_page(&mut self, cx: &mut Context<Self>) {
        if self.search_active {
            self.search.update(cx, |search, cx| search.refresh(cx));
        } else {
            self.home.update(cx, |home, cx| home.refresh(cx));
        }
    }

    fn play(&mut self, video: OpenVideo, window: &mut Window, cx: &mut Context<Self>) {
        self.open_playback(
            PlaybackRequest::video(video.title, video.bvid, video.cid),
            window,
            cx,
        );
    }

    fn play_live(&mut self, live: OpenLive, window: &mut Window, cx: &mut Context<Self>) {
        self.open_playback(PlaybackRequest::live(live.title, live.room_id), window, cx);
    }

    fn open_playback(
        &mut self,
        request: PlaybackRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        h_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(
                Sidebar::new("app-sidebar")
                    .collapsible(false)
                    .w(px(88.))
                    .border_r_1()
                    .border_color(border)
                    .header(self.render_logo(cx))
                    .child(
                        NavItem::new("首页", IconName::House)
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
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_top_bar(cx))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .child(if self.search_active {
                                self.search.clone().into_any_element()
                            } else {
                                self.home.clone().into_any_element()
                            })
                            .when(self.page_can_refresh(cx), |this| {
                                this.child(
                                    div().absolute().right_6().bottom_6().occlude().child(
                                        Button::new("page-refresh")
                                            .secondary()
                                            .rounded(ButtonRounded::Large)
                                            .w(px(44.))
                                            .h(px(44.))
                                            .icon(IconName::RefreshCw)
                                            .accessibility_label("刷新")
                                            .tooltip("刷新")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_page(cx);
                                            })),
                                    ),
                                )
                            }),
                    ),
            )
    }
}
