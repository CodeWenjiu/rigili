rigili_macro::mod_prv!(chrome, search_box);

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_core::{OpenLive, OpenSpace, OpenVideo, Refresh};

use super::playback::Playback;
use crate::app::preferences;
use crate::app::shortcuts::{
    Back, CARD, FocusPlayer, FocusSearch, MAIN, NextContentTab, NextSubTab, OpenSelectedCard,
    OpenSelectedCardInNewTab, PreviousContentTab, PreviousSubTab, ReloadPage, SelectCardDown,
    SelectCardLeft, SelectCardRight, SelectCardUp, ShowHome, ShowMine, ToggleCardFocus,
    ToggleMessages, ToggleMySpace, open_help,
};
use crate::pages::account::login;
use crate::pages::account::message::MessagesView;
use crate::pages::account::mine::MineView;
use crate::pages::content::home::HomeView;
use crate::pages::content::search::SearchView;
use crate::pages::content::space::{CloseSpace, SpaceView};
use crate::shared::browse::selection::{CardMotion, CardNav};
use crate::shared::media::preview;
use crate::shared::ui::tabs::{SubTabNav, TabNav};
use rigili_core::ShowShortcuts;
use rigili_core::image_cache;
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
    card_focus: FocusHandle,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchHost {
    Global,
    Mine,
    Space,
}

fn search_host_for(page: Page, space_open: bool) -> SearchHost {
    if space_open {
        SearchHost::Space
    } else if page == Page::Mine {
        SearchHost::Mine
    } else {
        SearchHost::Global
    }
}

#[derive(Clone)]
struct SpaceRoute {
    mid: u64,
    name: String,
}

fn my_space_is_on_top(spaces: &[SpaceRoute], mid: u64) -> bool {
    spaces.last().is_some_and(|route| route.mid == mid)
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
        let focus = cx.focus_handle();
        let card_focus = cx.focus_handle();
        let home = cx.new(|cx| HomeView::new(card_focus.clone(), window, cx));
        let search = cx.new(|cx| SearchView::new(card_focus.clone(), window, cx));
        let messages = cx.new(|_| MessagesView::new());
        let mine = cx.new(|cx| MineView::new(card_focus.clone(), window, cx));
        let space = cx.new(|cx| SpaceView::new(card_focus.clone(), window, cx));
        let search_box = SearchBox::new(window, cx);
        login::restore(window, cx);

        cx.on_focus(&card_focus, window, |_, _, cx| cx.notify())
            .detach();
        cx.on_blur(&card_focus, window, |_, _, cx| cx.notify())
            .detach();
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

        let main_window = window.window_handle();
        cx.on_window_closed({
            let weak = cx.entity().downgrade();
            move |cx, window_id| {
                if window_id == main_window.window_id() {
                    cx.quit();
                    return;
                }
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
            card_focus,
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

    fn submit_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        self.messages_open = false;
        let keyword = self.search_box.value(cx).trim().to_owned();
        self.dismiss_search_box(window, cx);
        self.focus.focus(window, cx);
        self.spaces.clear();
        if keyword.is_empty() {
            self.page = Page::Home;
            cx.notify();
            return;
        }

        preferences::record_search(cx, &keyword);
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
        self.search_box.close();
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
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_my_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(user) = rigili_core::login::user(cx) else {
            return;
        };
        if my_space_is_on_top(&self.spaces, user.mid) {
            self.close_space(window, cx);
            return;
        }
        self.open_space(
            OpenSpace {
                mid: user.mid,
                name: user.uname,
            },
            window,
            cx,
        );
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
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn focus_search_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        self.messages_open = false;
        match self.search_host() {
            SearchHost::Global => {
                self.open_search_box(cx);
                self.search_box.focus(window, cx);
            }
            SearchHost::Mine => {
                self.search_box.close();
                self.mine
                    .update(cx, |mine, cx| mine.focus_search(window, cx));
            }
            SearchHost::Space => {
                self.search_box.close();
                self.space
                    .update(cx, |space, cx| space.focus_search(window, cx));
            }
        }
        cx.notify();
    }

    fn search_host(&self) -> SearchHost {
        search_host_for(self.page, !self.spaces.is_empty())
    }

    fn blur_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
        let focus = self.focus.clone();
        window.defer(cx, move |window, cx| {
            if window.focused(cx).is_none() {
                focus.focus(window, cx);
            }
        });
    }

    fn show_home(&mut self, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        let messages = self.messages_open;
        self.messages_open = false;
        if messages || self.page != Page::Home || !self.spaces.is_empty() {
            self.page = Page::Home;
            self.spaces.clear();
            cx.notify();
        }
    }

    fn show_mine(&mut self, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        self.search_box.close();
        let messages = self.messages_open;
        self.messages_open = false;
        if !messages && self.page == Page::Mine && self.spaces.is_empty() {
            return;
        }
        self.page = Page::Mine;
        self.spaces.clear();
        cx.notify();
    }

    fn toggle_messages(&mut self, cx: &mut Context<Self>) {
        if !self.messages_open {
            preview::stop_all(cx);
        }
        self.messages_open = !self.messages_open;
        cx.notify();
    }

    fn step_content_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        let changed = if self.messages_open {
            self.messages
                .update(cx, |messages, cx| messages.step_tab(delta, cx))
        } else if !self.spaces.is_empty() {
            self.space.update(cx, |space, cx| space.step_tab(delta, cx))
        } else {
            match self.page {
                Page::Home => self.home.update(cx, |home, cx| home.step_tab(delta, cx)),
                Page::Search => self
                    .search
                    .update(cx, |search, cx| search.step_tab(delta, cx)),
                Page::Mine => self.mine.update(cx, |mine, cx| mine.step_tab(delta, cx)),
            }
        };
        if changed {
            preview::stop_all(cx);
        }
    }

    fn step_sub_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.messages_open {
            return;
        }
        let changed = if !self.spaces.is_empty() {
            self.space
                .update(cx, |space, cx| space.step_sub_tab(delta, cx))
        } else if self.page == Page::Mine {
            self.mine
                .update(cx, |mine, cx| mine.step_sub_tab(delta, cx))
        } else {
            false
        };
        if changed {
            preview::stop_all(cx);
        }
    }

    fn page_card_count(&self, cx: &App) -> usize {
        if self.messages_open || self.search_box_is_open() {
            return 0;
        }
        if !self.spaces.is_empty() {
            return self.space.read(cx).card_count();
        }
        match self.page {
            Page::Home => self.home.read(cx).card_count(),
            Page::Search => self.search.read(cx).card_count(),
            Page::Mine => self.mine.read(cx).card_count(),
        }
    }

    fn toggle_card_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.card_focus.is_focused(window) {
            self.focus.focus(window, cx);
            return;
        }
        if self.page_card_count(cx) == 0 {
            return;
        }
        if self.spaces.is_empty() {
            match self.page {
                Page::Home => self.home.update(cx, |home, cx| home.ensure_card(cx)),
                Page::Search => self.search.update(cx, |search, cx| search.ensure_card(cx)),
                Page::Mine => self.mine.update(cx, |mine, cx| mine.ensure_card(cx)),
            }
        } else {
            self.space.update(cx, |space, cx| space.ensure_card(cx));
        }
        self.card_focus.focus(window, cx);
    }

    fn step_card(&mut self, motion: CardMotion, cx: &mut Context<Self>) {
        if self.messages_open {
            return;
        }
        if self.spaces.is_empty() {
            match self.page {
                Page::Home => self.home.update(cx, |home, cx| home.step_card(motion, cx)),
                Page::Search => self
                    .search
                    .update(cx, |search, cx| search.step_card(motion, cx)),
                Page::Mine => self.mine.update(cx, |mine, cx| mine.step_card(motion, cx)),
            }
        } else {
            self.space
                .update(cx, |space, cx| space.step_card(motion, cx));
        }
    }

    fn open_card(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        if self.messages_open {
            return;
        }
        if self.spaces.is_empty() {
            match self.page {
                Page::Home => self.home.update(cx, |home, cx| home.open_card(new_tab, cx)),
                Page::Search => self
                    .search
                    .update(cx, |search, cx| search.open_card(new_tab, cx)),
                Page::Mine => self.mine.update(cx, |mine, cx| mine.open_card(new_tab, cx)),
            }
        } else {
            self.space
                .update(cx, |space, cx| space.open_card(new_tab, cx));
        }
    }

    fn focus_player(&mut self, cx: &mut Context<Self>) {
        if let Some(playback) = &self.playback {
            playback.activate(cx);
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_box_is_open() {
            self.dismiss_search_box(window, cx);
        } else if self.card_focus.is_focused(window) {
            self.focus.focus(window, cx);
        } else if self.messages_open {
            self.messages_open = false;
            self.focus.focus(window, cx);
            cx.notify();
        } else if !self.spaces.is_empty() {
            self.close_space(window, cx);
            self.focus.focus(window, cx);
        } else if matches!(self.page, Page::Search | Page::Mine) {
            self.show_home(cx);
            self.focus.focus(window, cx);
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

    fn refresh_target_can_refresh(&self, cx: &App) -> bool {
        if self.messages_open {
            return self.messages.read(cx).can_refresh();
        }
        self.page_can_refresh(cx)
    }

    fn refresh_page(&mut self, cx: &mut Context<Self>) {
        if self.messages_open {
            self.messages
                .update(cx, |messages, cx| messages.refresh(cx));
            return;
        }
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
        let request = match video.episode {
            Some(episode) => PlaybackRequest::episode(video.title, video.bvid, video.cid, episode),
            None => PlaybackRequest::video(video.title, video.bvid, video.cid),
        };
        self.open_playback(request, video.new_tab, cx);
    }

    fn play_live(&mut self, live: OpenLive, cx: &mut Context<Self>) {
        self.open_playback(
            PlaybackRequest::live(live.title, live.room_id),
            live.new_tab,
            cx,
        );
    }

    fn open_playback(&mut self, request: PlaybackRequest, new_tab: bool, cx: &mut Context<Self>) {
        preview::stop_all(cx);
        if let Some(playback) = self.playback.as_ref() {
            if playback.load(request.clone(), new_tab, cx).is_ok() {
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
                if this.refresh_target_can_refresh(cx) {
                    this.refresh_page(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.focus_search_shortcut(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPlayer, _, cx| {
                this.focus_player(cx);
            }))
            .on_action(cx.listener(|this, _: &ShowHome, window, cx| {
                this.show_home(cx);
                this.focus.focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowMine, window, cx| {
                this.show_mine(cx);
                this.focus.focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleMySpace, window, cx| {
                this.toggle_my_space(window, cx);
                this.focus.focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleMessages, window, cx| {
                this.toggle_messages(cx);
                this.focus.focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousContentTab, _, cx| {
                this.step_content_tab(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextContentTab, _, cx| {
                this.step_content_tab(1, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousSubTab, _, cx| {
                this.step_sub_tab(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextSubTab, _, cx| {
                this.step_sub_tab(1, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCardFocus, window, cx| {
                this.toggle_card_focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectCardLeft, _, cx| {
                this.step_card(CardMotion::Left, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectCardRight, _, cx| {
                this.step_card(CardMotion::Right, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectCardUp, _, cx| {
                this.step_card(CardMotion::Up, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectCardDown, _, cx| {
                this.step_card(CardMotion::Down, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSelectedCard, _, cx| {
                this.open_card(false, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSelectedCardInNewTab, _, cx| {
                this.open_card(true, cx);
            }))
            .on_action(cx.listener(|this, _: &Back, window, cx| {
                this.back(window, cx);
            }))
            .on_action(cx.listener(|_, _: &ShowShortcuts, window, cx| {
                open_help(window, cx);
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
                            .track_focus(&self.card_focus)
                            .when(self.card_focus.is_focused(window), |this| {
                                this.key_context(CARD)
                            })
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

#[cfg(test)]
mod tests {
    use super::{Page, SearchHost, SpaceRoute, my_space_is_on_top, search_host_for};

    fn route(mid: u64) -> SpaceRoute {
        SpaceRoute {
            mid,
            name: String::new(),
        }
    }

    #[test]
    fn toggles_only_when_my_space_is_on_top() {
        assert!(!my_space_is_on_top(&[], 1));
        assert!(my_space_is_on_top(&[route(1)], 1));
        assert!(!my_space_is_on_top(&[route(1), route(2)], 1));
        assert!(my_space_is_on_top(&[route(2), route(1)], 1));
    }

    #[test]
    fn routes_search_by_context() {
        assert_eq!(search_host_for(Page::Home, false), SearchHost::Global);
        assert_eq!(search_host_for(Page::Search, false), SearchHost::Global);
        assert_eq!(search_host_for(Page::Mine, false), SearchHost::Mine);
        assert_eq!(search_host_for(Page::Home, true), SearchHost::Space);
        assert_eq!(search_host_for(Page::Search, true), SearchHost::Space);
        assert_eq!(search_host_for(Page::Mine, true), SearchHost::Space);
    }
}
