rigili_macro::mod_prv!(search_box, search_history);

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh};

use super::playback::Playback;
use crate::home::{HomeTab, HomeView};
use crate::login;
use crate::message::MessagesView;
use crate::mine::MineView;
use crate::platform::image_cache;
use crate::preview;
use crate::search::SearchView;
use crate::space::{CloseSpace, SpaceView};
use crate::tabs::TabStrip;
use rigili_playback::PlaybackRequest;
use search_box::SearchBox;

pub struct MainView {
    home: Entity<HomeView>,
    search: Entity<SearchView>,
    messages: Entity<MessagesView>,
    messages_open: bool,
    mine: Entity<MineView>,
    space: Entity<SpaceView>,
    search_box: SearchBox,
    page: Page,
    spaces: Vec<SpaceRoute>,
    playback: Option<Playback>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Home,
    Search,
    Mine,
}

#[derive(Clone)]
struct SpaceRoute {
    mid: u64,
    name: String,
}

type NavHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

const SIDEBAR_WIDTH: Pixels = px(55.);
const TOP_BAR_HEIGHT: Pixels = px(64.);

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
        app_id: Some("rigili".into()),
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

fn search_width(window: &Window) -> Pixels {
    (window.viewport_size().width - px(24.)).clamp(px(240.), px(420.))
}

fn messages_panel_width(window: &Window) -> Pixels {
    let insets = px(16.);
    (window.viewport_size().width - SIDEBAR_WIDTH - insets).min(px(600.))
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = cx.new(HomeView::new);
        let search = cx.new(SearchView::new);
        let messages = cx.new(|_| MessagesView::new());
        let mine = cx.new(|cx| MineView::new(window, cx));
        let space = cx.new(|cx| SpaceView::new(window, cx));
        let search_box = SearchBox::new(window, cx);
        login::restore(window, cx);

        Self::forward_page_events(cx, &home, window);
        Self::forward_page_events(cx, &search, window);
        Self::forward_space_events(cx, &messages, window);
        Self::forward_video_events(cx, &mine, window);
        Self::forward_space_events(cx, &mine, window);
        Self::forward_video_events(cx, &space, window);
        Self::forward_space_events(cx, &space, window);
        Self::forward_close_space_events(cx, &space, window);

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
            messages,
            messages_open: false,
            mine,
            space,
            search_box,
            page: Page::default(),
            spaces: Vec::new(),
            playback: None,
        }
    }

    fn forward_page_events<T>(cx: &mut Context<Self>, page: &Entity<T>, window: &mut Window)
    where
        T: EventEmitter<OpenVideo> + EventEmitter<OpenLive> + EventEmitter<OpenSpace> + 'static,
    {
        Self::forward_video_events(cx, page, window);
        Self::forward_live_events(cx, page, window);
        Self::forward_space_events(cx, page, window);
    }

    fn forward_video_events<T>(cx: &mut Context<Self>, page: &Entity<T>, window: &mut Window)
    where
        T: EventEmitter<OpenVideo> + 'static,
    {
        cx.subscribe_in(page, window, |this, _, event: &OpenVideo, _window, cx| {
            this.play(event.clone(), cx);
        })
        .detach();
    }

    fn forward_live_events<T>(cx: &mut Context<Self>, page: &Entity<T>, window: &mut Window)
    where
        T: EventEmitter<OpenLive> + 'static,
    {
        cx.subscribe_in(page, window, |this, _, event: &OpenLive, _window, cx| {
            this.play_live(event.clone(), cx);
        })
        .detach();
    }

    fn forward_space_events<T>(cx: &mut Context<Self>, page: &Entity<T>, window: &mut Window)
    where
        T: EventEmitter<OpenSpace> + 'static,
    {
        cx.subscribe_in(page, window, |this, _, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        })
        .detach();
    }

    fn forward_close_space_events<T>(cx: &mut Context<Self>, page: &Entity<T>, window: &mut Window)
    where
        T: EventEmitter<CloseSpace> + 'static,
    {
        cx.subscribe_in(page, window, |this, _, _: &CloseSpace, window, cx| {
            this.close_space(window, cx);
        })
        .detach();
    }

    fn submit_search(&mut self, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        self.messages_open = false;
        let keyword = self.search_box.value(cx).trim().to_owned();
        self.search_box.close();
        self.spaces.clear();
        if keyword.is_empty() {
            self.page = Page::Home;
            cx.notify();
            return;
        }

        self.search_box.record(&keyword);
        self.page = Page::Search;
        self.search
            .update(cx, |search, cx| search.search(keyword, cx));
        cx.notify();
    }

    fn open_space(&mut self, request: OpenSpace, window: &mut Window, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        self.messages_open = false;
        if self
            .spaces
            .last()
            .is_some_and(|route| route.mid == request.mid)
        {
            return;
        }
        let OpenSpace { mid, name } = request;
        self.spaces.push(SpaceRoute {
            mid,
            name: name.clone(),
        });
        self.space.update(cx, |space, cx| {
            space.open(mid, name, window, cx);
        });
        cx.notify();
    }

    fn close_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        if self.spaces.pop().is_none() {
            return;
        }
        if let Some(previous) = self.spaces.last().cloned() {
            self.space.update(cx, |space, cx| {
                space.open(previous.mid, previous.name, window, cx);
            });
        }
        cx.notify();
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_box.focus_handle(cx).is_focused(window) {
            return;
        }

        self.search_box.suppress_next_focus_open();
        self.search_box.focus(window, cx);
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

    fn render_messages_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .left(SIDEBAR_WIDTH + px(8.))
            .top(TOP_BAR_HEIGHT + px(8.))
            .bottom(px(8.))
            .w(messages_panel_width(window))
            .occlude()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_xl()
            .child(self.messages.clone())
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
                    .data(include_bytes!("../../../assets/rigili-mark.svg"))
                    .with_size(px(36.))
                    .text_color(cx.theme().sidebar_primary),
            )
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let weak = cx.entity().downgrade();

        v_flex()
            .w(SIDEBAR_WIDTH)
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
                            .active(matches!(self.page, Page::Home | Page::Search))
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        preview::stop_all(cx);
                                        this.messages_open = false;
                                        if this.page != Page::Home || !this.spaces.is_empty() {
                                            this.page = Page::Home;
                                            this.spaces.clear();
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .render("nav-home", cx),
                    )
                    .child(
                        NavItem::new("我的", IconName::CircleUser)
                            .active(self.page == Page::Mine)
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        preview::stop_all(cx);
                                        this.messages_open = false;
                                        if this.page == Page::Mine && this.spaces.is_empty() {
                                            return;
                                        }
                                        this.page = Page::Mine;
                                        this.spaces.clear();
                                        cx.notify();
                                    });
                                }
                            })
                            .render("nav-mine", cx),
                    ),
            )
            .child(div().flex_1())
            .child(
                v_flex()
                    .w_full()
                    .pb_3()
                    .items_center()
                    .gap_1()
                    .child(self.account(cx))
                    .child(
                        NavItem::new("消息", IconName::Mail)
                            .active(self.messages_open)
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.messages_open = !this.messages_open;
                                        cx.notify();
                                    });
                                }
                            })
                            .render("nav-messages", cx),
                    ),
            )
            .into_any_element()
    }

    fn render_home_tabs(&self, selected: HomeTab, cx: &mut Context<Self>) -> AnyElement {
        TabStrip::new("top-tab")
            .text_base()
            .gap(px(8.))
            .padding_x(px(12.))
            .divider(false)
            .items_end(false)
            .render(
                cx,
                &HomeTab::ALL,
                selected,
                HomeTab::label,
                |_| None,
                |this, tab, cx| {
                    this.messages_open = false;
                    this.page = Page::Home;
                    this.spaces.clear();
                    this.home.update(cx, |home, cx| home.show_tab(tab, cx));
                    cx.notify();
                },
            )
    }

    fn render_top_bar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let search_width = search_width(window);
        let foreground = cx.theme().foreground;
        let home_tab = self.home.read(cx).current_tab();

        h_flex()
            .h(TOP_BAR_HEIGHT)
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
                    .when(
                        matches!(self.page, Page::Mine) || !self.spaces.is_empty(),
                        |this| this.invisible(),
                    )
                    .child(self.render_home_tabs(home_tab, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_center()
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
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
                    }))
                    .child({
                        let weak = cx.entity().downgrade();
                        let focus = self.search_box.focus_handle(cx);
                        Popover::new("search-popover")
                            .offset(px(6.))
                            .open(self.search_box_is_open())
                            .appearance(false)
                            .track_focus(&focus)
                            .trigger_style(StyleRefinement::default().w_full().max_w(search_width))
                            .on_open_change({
                                let weak = weak.clone();
                                move |open, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.search_popover_open_changed(*open, window, cx)
                                    });
                                }
                            })
                            .trigger(
                                Input::new(self.search_box.input())
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
                            )
                            .content({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    weak.read_with(cx, |this, cx| {
                                        this.render_search_panel(&weak, search_width, cx)
                                    })
                                    .unwrap_or_else(|_| div().into_any_element())
                                }
                            })
                    }),
            )
            .child(
                Button::new("window-close")
                    .ghost()
                    .icon(IconName::Close)
                    .accessibility_label("关闭窗口")
                    .tooltip("关闭")
                    .on_click(|_, window, _| window.remove_window()),
            )
            .into_any_element()
    }

    fn page_can_refresh(&self, cx: &App) -> bool {
        if !self.spaces.is_empty() {
            return self.space.read(cx).can_refresh();
        }
        match self.page {
            Page::Mine => self.mine.read(cx).can_refresh(),
            Page::Search => self.search.read(cx).can_refresh(),
            Page::Home => self.home.read(cx).can_refresh(),
        }
    }

    fn refresh_page(&mut self, cx: &mut Context<Self>) {
        if !self.spaces.is_empty() {
            self.space.update(cx, |space, cx| space.refresh(cx));
            return;
        }
        match self.page {
            Page::Mine => self.mine.update(cx, |mine, cx| mine.refresh(cx)),
            Page::Search => self.search.update(cx, |search, cx| search.refresh(cx)),
            Page::Home => self.home.update(cx, |home, cx| home.refresh(cx)),
        }
    }

    fn play(&mut self, video: OpenVideo, cx: &mut Context<Self>) {
        self.open_playback(
            PlaybackRequest::video(video.title, video.bvid, video.cid),
            cx,
        );
    }

    fn play_live(&mut self, live: OpenLive, cx: &mut Context<Self>) {
        self.open_playback(PlaybackRequest::live(live.title, live.room_id), cx);
    }

    fn open_playback(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        if let Some(playback) = self.playback.as_ref() {
            if playback.load(request.clone(), cx).is_ok() {
                return;
            }
            self.playback = None;
        }

        match Playback::open(request, cx) {
            Ok(playback) => self.playback = Some(playback),
            Err(error) => eprintln!("failed to open player window: {error}"),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        h_flex()
            .relative()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .image_cache(image_cache::get(cx))
            .child(self.render_sidebar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_top_bar(window, cx))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .child(if !self.spaces.is_empty() {
                                self.space.clone().into_any_element()
                            } else {
                                match self.page {
                                    Page::Mine => self.mine.clone().into_any_element(),
                                    Page::Search => self.search.clone().into_any_element(),
                                    Page::Home => self.home.clone().into_any_element(),
                                }
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
            .when(self.messages_open, |this| {
                this.child(
                    div()
                        .id("messages-backdrop")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.messages_open = false;
                            cx.notify();
                        })),
                )
            })
            .when(self.messages_open, |this| {
                this.child(self.render_messages_panel(window, cx))
            })
    }
}
