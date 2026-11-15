rigili_macro::mod_prv!(chrome, search_box, search_history);

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh};

use super::playback::Playback;
use crate::app::platform::image_cache;
use crate::app::shortcuts::{FocusPlayer, FocusSearch, MAIN, ReloadPage};
use crate::pages::account::login;
use crate::pages::account::message::MessagesView;
use crate::pages::account::mine::MineView;
use crate::pages::content::home::HomeView;
use crate::pages::content::search::SearchView;
use crate::pages::content::space::{CloseSpace, SpaceView};
use crate::shared::media::preview;
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
    focus: FocusHandle,
    spaces: Vec<SpaceRoute>,
    playback: Option<Playback>,
    window_handle: AnyWindowHandle,
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

const SIDEBAR_WIDTH: Pixels = px(55.);
const TOP_BAR_HEIGHT: Pixels = px(64.);

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

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = cx.new(HomeView::new);
        let search = cx.new(SearchView::new);
        let messages = cx.new(|_| MessagesView::new());
        let mine = cx.new(|cx| MineView::new(window, cx));
        let space = cx.new(|cx| SpaceView::new(window, cx));
        let search_box = SearchBox::new(window, cx);
        login::restore(window, cx);

        let focus = cx.focus_handle();
        let deferred_focus = focus.clone();
        window.defer(cx, move |window, cx| deferred_focus.focus(window, cx));

        Self::forward_events(cx, &home, window, |this, event: &OpenVideo, _, cx| {
            this.play(event.clone(), cx);
        });
        Self::forward_events(cx, &home, window, |this, event: &OpenLive, _, cx| {
            this.play_live(event.clone(), cx);
        });
        Self::forward_events(cx, &home, window, |this, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        });
        Self::forward_events(cx, &search, window, |this, event: &OpenVideo, _, cx| {
            this.play(event.clone(), cx);
        });
        Self::forward_events(cx, &search, window, |this, event: &OpenLive, _, cx| {
            this.play_live(event.clone(), cx);
        });
        Self::forward_events(
            cx,
            &search,
            window,
            |this, event: &OpenSpace, window, cx| {
                this.open_space(event.clone(), window, cx);
            },
        );
        Self::forward_events(
            cx,
            &messages,
            window,
            |this, event: &OpenSpace, window, cx| {
                this.open_space(event.clone(), window, cx);
            },
        );
        Self::forward_events(cx, &mine, window, |this, event: &OpenVideo, _, cx| {
            this.play(event.clone(), cx);
        });
        Self::forward_events(cx, &mine, window, |this, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        });
        Self::forward_events(cx, &space, window, |this, event: &OpenVideo, _, cx| {
            this.play(event.clone(), cx);
        });
        Self::forward_events(cx, &space, window, |this, event: &OpenSpace, window, cx| {
            this.open_space(event.clone(), window, cx);
        });
        Self::forward_events(cx, &space, window, |this, _: &CloseSpace, window, cx| {
            this.close_space(window, cx);
        });

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
            focus,
            spaces: Vec::new(),
            playback: None,
            window_handle: window.window_handle(),
        }
    }

    fn forward_events<T, E>(
        cx: &mut Context<Self>,
        page: &Entity<T>,
        window: &mut Window,
        handler: impl Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    ) where
        T: EventEmitter<E> + 'static,
        E: 'static,
    {
        cx.subscribe_in(page, window, move |this, _, event, window, cx| {
            handler(this, event, window, cx);
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

    pub(super) fn open_space(
        &mut self,
        request: OpenSpace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn focus_search_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_search_box(cx);
        self.search_box.focus(window, cx);
        cx.notify();
    }

    fn focus_player(&mut self, cx: &mut Context<Self>) {
        if let Some(playback) = &self.playback {
            playback.activate(cx);
        }
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

        match Playback::open(request, self.window_handle, cx.entity().downgrade(), cx) {
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
            .key_context(MAIN)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ReloadPage, _, cx| {
                if this.page_can_refresh(cx) {
                    this.refresh_page(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.focus_search_shortcut(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPlayer, _, cx| {
                this.focus_player(cx);
            }))
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
