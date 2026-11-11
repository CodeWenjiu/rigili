rigili_macro::mod_prv!(search_box, search_history);

use std::{cell::Cell, rc::Rc};

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh};

use super::playback::Playback;
use crate::home::{HomeTab, HomeView};
use crate::login;
use crate::mine::MineView;
use crate::search::SearchView;
use crate::space::{CloseSpace, SpaceView};
use rigili_playback::PlaybackRequest;
use search_box::SearchBox;

pub struct MainView {
    home: Entity<HomeView>,
    search: Entity<SearchView>,
    mine: Entity<MineView>,
    space: Entity<SpaceView>,
    search_input: Entity<InputState>,
    search_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    search_box: SearchBox,
    search_active: bool,
    mine_active: bool,
    space_active: bool,
    space_return: ReturnPage,
    playback: Option<Playback>,
    _search_input_sub: Subscription,
}

#[derive(Clone, Copy, Default)]
enum ReturnPage {
    #[default]
    Home,
    Search,
    Mine,
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

    fn render(self, id: impl Into<ElementId>, cx: &mut App) -> impl IntoElement {
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
            .px_0()
            .accessibility_label(label.clone())
            .on_click(move |event, window, cx| on_click(event, window, cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .text_color(color)
                    .child(Icon::new(icon).with_size(px(22.)))
                    .child(div().whitespace_nowrap().text_sm().child(label)),
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
        let mine = cx.new(|cx| MineView::new(window, cx));
        let space = cx.new(|cx| SpaceView::new(window, cx));
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索视频、番剧、用户"));
        let input_sub = cx.subscribe_in(&search_input, window, |this, _, event, _, cx| {
            this.handle_search_input(event, cx);
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

        cx.subscribe_in(&home, window, |this, _, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
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

        cx.subscribe_in(&search, window, |this, _, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&mine, window, |this, _, event: &OpenVideo, window, cx| {
            this.play(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&mine, window, |this, _, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&space, window, |this, _, event: &OpenVideo, window, cx| {
            this.play(event.clone(), window, cx);
        })
        .detach();

        cx.subscribe_in(&space, window, |this, _, _: &CloseSpace, window, cx| {
            this.close_space(window, cx);
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
            mine,
            space,
            search_input,
            search_bounds: Rc::new(Cell::new(None)),
            search_box: SearchBox::new(),
            search_active: false,
            mine_active: false,
            space_active: false,
            space_return: ReturnPage::default(),
            playback: None,
            _search_input_sub: input_sub,
        }
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        let keyword = self.search_input.read(cx).value().trim().to_owned();
        self.search_box.close();
        self.mine_active = false;
        self.space_active = false;
        if keyword.is_empty() {
            self.search_active = false;
            cx.notify();
            return;
        }

        self.search_box.record(&keyword);
        self.search_active = true;
        self.search
            .update(cx, |search, cx| search.search(keyword, cx));
        cx.notify();
    }

    fn open_space(&mut self, request: OpenSpace, window: &mut Window, cx: &mut Context<Self>) {
        if self.space_active && self.space.read(cx).mid() == Some(request.mid) {
            return;
        }
        self.space_return = if self.search_active {
            ReturnPage::Search
        } else if self.mine_active {
            ReturnPage::Mine
        } else {
            ReturnPage::Home
        };
        self.space_active = true;
        self.search_active = false;
        self.mine_active = false;
        self.space.update(cx, |space, cx| {
            space.open(request.mid, request.name, window, cx);
        });
        cx.notify();
    }

    fn close_space(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.space_active {
            return;
        }
        self.space_active = false;
        match self.space_return {
            ReturnPage::Search => self.search_active = true,
            ReturnPage::Mine => self.mine_active = true,
            ReturnPage::Home => {}
        }
        self.space_return = ReturnPage::default();
        cx.notify();
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            return;
        }

        self.search_box.suppress_next_focus_open();
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(user) = rigili_core::login::user(cx) {
            let mid = user.mid;
            let name = user.uname.clone();
            return Button::new("account-space")
                .ghost()
                .p_1()
                .rounded_full()
                .accessibility_label("我的空间")
                .tooltip("我的空间")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_space(
                        OpenSpace {
                            mid,
                            name: name.clone(),
                        },
                        window,
                        cx,
                    );
                }))
                .child(Avatar::new().src(user.face).name(user.uname).size(px(32.)))
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

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let weak = cx.entity().downgrade();

        v_flex()
            .w(px(55.))
            .h_full()
            .flex_shrink_0()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(border)
            .child(div().w_full().pt_3().child(self.render_logo(cx)))
            .child(
                v_flex()
                    .w_full()
                    .pt_3()
                    .gap_y_3()
                    .child(
                        NavItem::new("首页", IconName::House)
                            .active(!self.mine_active && !self.space_active)
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if this.search_active
                                            || this.mine_active
                                            || this.space_active
                                        {
                                            this.search_active = false;
                                            this.mine_active = false;
                                            this.space_active = false;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .render("nav-home", cx),
                    )
                    .child(
                        NavItem::new("我的", IconName::CircleUser)
                            .active(self.mine_active)
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if this.mine_active && !this.space_active {
                                            return;
                                        }
                                        this.mine_active = true;
                                        this.search_active = false;
                                        this.space_active = false;
                                        cx.notify();
                                    });
                                }
                            })
                            .render("nav-mine", cx),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .w_full()
                    .pb_3()
                    .flex()
                    .justify_center()
                    .child(self.account(cx)),
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
            .gap_2()
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
                            .px_3()
                            .py_1()
                            .accessibility_label(tab.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.search_active = false;
                                this.mine_active = false;
                                this.home.update(cx, |home, cx| home.show_tab(tab, cx));
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_base()
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
            .child(
                div()
                    .flex_shrink_0()
                    .when(self.mine_active || self.space_active, |this| {
                        this.invisible()
                    })
                    .child(self.render_home_tabs(home_tab, cx)),
            )
            .child(
                div().flex_1().min_w_0().flex().justify_center().child(
                    h_flex().w_full().max_w(px(420.)).items_center().child(
                        div()
                            .id("search-input")
                            .flex_1()
                            .min_w_0()
                            .on_prepaint({
                                let bounds = self.search_bounds.clone();
                                move |value, _, _| bounds.set(Some(value))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.open_search_box(cx);
                                    this.focus_search(window, cx);
                                }),
                            )
                            .capture_key_down(cx.listener(
                                |this, event: &KeyDownEvent, window, cx| {
                                    let handled = match event.keystroke.key.as_str() {
                                        "escape" => {
                                            let open = this.search_box_is_open();
                                            if open {
                                                this.dismiss_search_box(window, cx);
                                            }
                                            open
                                        }
                                        "down" => this.move_suggestion_selection(true, cx),
                                        "up" => this.move_suggestion_selection(false, cx),
                                        "enter" => this.pick_active_suggestion(window, cx),
                                        _ => false,
                                    };
                                    if handled {
                                        cx.stop_propagation();
                                    }
                                },
                            ))
                            .child(
                                Input::new(&self.search_input)
                                    .large()
                                    .h(px(40.))
                                    .cleanable(true)
                                    .suffix(
                                        Button::new("search-submit")
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::Search)
                                            .accessibility_label("搜索")
                                            .tooltip("搜索")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_search(cx);
                                                this.focus_search(window, cx);
                                            })),
                                    ),
                            ),
                    ),
                ),
            )
            .child(
                Button::new("window-close")
                    .ghost()
                    .icon(IconName::Close)
                    .accessibility_label("关闭窗口")
                    .tooltip("关闭")
                    .on_click(|_, window, _| window.remove_window()),
            )
            .when_some(self.render_search_panel(cx), |this, panel| {
                this.child(panel)
            })
            .into_any_element()
    }

    fn page_can_refresh(&self, cx: &App) -> bool {
        if self.space_active {
            self.space.read(cx).can_refresh()
        } else if self.mine_active {
            self.mine.read(cx).can_refresh()
        } else if self.search_active {
            self.search.read(cx).can_refresh()
        } else {
            self.home.read(cx).can_refresh()
        }
    }

    fn refresh_page(&mut self, cx: &mut Context<Self>) {
        if self.space_active {
            self.space.update(cx, |space, cx| space.refresh(cx));
        } else if self.mine_active {
            self.mine.update(cx, |mine, cx| mine.refresh(cx));
        } else if self.search_active {
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
        let foreground = cx.theme().foreground;

        h_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(self.render_sidebar(cx))
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
                            .child(if self.space_active {
                                self.space.clone().into_any_element()
                            } else if self.mine_active {
                                self.mine.clone().into_any_element()
                            } else if self.search_active {
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
