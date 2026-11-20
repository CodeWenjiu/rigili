rigili_macro::mod_prv!(info, media, watch);

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab as TabButton, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_player::{PlaybackPhase, Player};

use super::history::History;
use super::surface::{PlayerEvent, PlayerView};
use crate::info::{InfoEvent, InfoPanel, InfoSelections, InfoTab};
use crate::request::{
    PlaybackRequest, PlaybackSource, Resolved, Streams, resolve, resolve_streams,
};
use crate::shortcuts::{
    CloseTab, HistoryBack, HistoryForward, NextPage, NextTab, PreviousPage, PreviousTab, Reload,
    SeekBack, SpeedDown, SpeedUp, SwitchToMain, ToggleComments, ToggleDanmaku, ToggleFullscreen,
    ToggleInfo, ToggleMute, TogglePlayback,
};
use rigili_bilibili::{Chapter, Client, VideoPage};
use rigili_core::client;
use rigili_core::runtime::{ApiTaskError, spawn_api};
use rigili_core::{FocusMain, OpenSpace};
use rigili_core::{api_error_message, api_task_error_message};

const INFO_MIN_WINDOW: Pixels = px(1000.);

// Play URLs carry a time-limited CDN token, so a cached one is only trusted
// for a short while; past that a tab pays the resolve cost again.
const STREAMS_TTL: Duration = Duration::from_secs(600);

// A warm tab keeps mpv alive with the stream paused, which buys an instant
// switch at the cost of one decoder and demuxer cache per tab.
const WARM_TTL: Duration = Duration::from_secs(600);
const MAX_WARM_TABS: usize = 2;

pub struct PlayerWindow {
    player: Entity<PlayerView>,
    client: Option<Arc<Client>>,
    tabs: Vec<Tab>,
    active: usize,
    next_tab_id: u64,
    show_info: bool,
    /// The tab whose stream is currently loaded in the shared player.
    session: Option<u64>,
    /// Bumped whenever the shared player is handed to another tab, so the
    /// watch heartbeat does not count time spent on a different video.
    epoch: u64,
    watch_task: Option<Task<()>>,
    _player_sub: Subscription,
    _focus_sub: Subscription,
}

struct Tab {
    id: u64,
    request: PlaybackRequest,
    history: History,
    title: SharedString,
    is_live: bool,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    generation: u64,
    danmaku_task: Option<Task<()>>,
    info: Entity<InfoPanel>,
    _info_sub: Subscription,
    resume: Option<f64>,
    paused: bool,
    pages: Vec<VideoPage>,
    chapters: Vec<Chapter>,
    streams: Option<CachedStreams>,
    /// Paused player kept alive while this tab is in the background, so
    /// switching back does not reopen the stream.
    player: Option<Player>,
    last_used: Instant,
    warm_since: Option<Instant>,
}

struct CachedStreams {
    streams: Streams,
    at: Instant,
}

impl Tab {
    fn is_warm(&self) -> bool {
        self.warm_since
            .is_some_and(|since| since.elapsed() < WARM_TTL)
            && self.loaded
            && self.streams.is_some()
    }
}

struct StreamTarget {
    tab_id: u64,
    generation: u64,
    bvid: String,
    cid: u64,
    resume: Option<f64>,
    paused: bool,
}

impl EventEmitter<OpenSpace> for PlayerWindow {}
impl EventEmitter<FocusMain> for PlayerWindow {}

impl PlayerWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>, request: PlaybackRequest) -> Self {
        let player = cx.new(|cx| PlayerView::new(window, cx));
        let player_sub = cx.subscribe(&player, |this, _, event: &PlayerEvent, cx| match event {
            PlayerEvent::SelectPage(cid) => this.select_page(*cid, cx),
        });
        // Key bindings are looked up along the focused node's context stack, so
        // once the focused element unmounts (the auto-hiding bar, a comment row
        // scrolled away) the stack is empty and every player shortcut silently
        // stops matching until something focuses an element again.
        let focus_sub = cx.on_focus_lost(window, |this, window, cx| {
            rigili_monitor::monitor!("player", "focus lost, returning to the video surface");
            this.player
                .update(cx, |player, cx| player.focus_player(window, cx));
        });
        let (client, error) = match client::get(cx) {
            Ok(client) => (Some(client), None),
            Err(error) => (None, Some(api_error_message(error))),
        };
        let mut view = Self {
            player,
            client,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 0,
            show_info: true,
            session: None,
            epoch: 0,
            watch_task: None,
            _player_sub: player_sub,
            _focus_sub: focus_sub,
        };
        let mut tab = view.make_tab(window, cx, request.clone());
        tab.history.open(request.source.clone());
        tab.error = error;
        view.tabs.push(tab);
        if view.client.is_some() {
            view.start_active(request, cx);
        }
        view.spawn_watch_report(window, cx);
        view
    }

    fn make_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        request: PlaybackRequest,
    ) -> Tab {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.wrapping_add(1);
        let selections = InfoSelections::new(window, cx);
        let info = cx.new(|_| InfoPanel::new(selections));
        let info_sub = cx.subscribe_in(
            &info,
            window,
            move |this, _, event: &InfoEvent, window, cx| {
                this.on_info_event(id, event, window, cx);
            },
        );
        let is_live = matches!(request.source, PlaybackSource::Live { .. });
        Tab {
            id,
            history: History::default(),
            title: if request.title.is_empty() {
                "加载中…".into()
            } else {
                request.title.clone().into()
            },
            request,
            is_live,
            loaded: false,
            loading: false,
            error: None,
            generation: 0,
            danmaku_task: None,
            info,
            _info_sub: info_sub,
            resume: None,
            paused: false,
            pages: Vec::new(),
            chapters: Vec::new(),
            streams: None,
            player: None,
            last_used: Instant::now(),
            warm_since: None,
        }
    }

    fn on_info_event(
        &mut self,
        tab_id: u64,
        event: &InfoEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs[self.active].id != tab_id {
            return;
        }
        match event {
            InfoEvent::SelectPage(cid) => self.select_page(*cid, cx),
            InfoEvent::PlayRelated {
                bvid,
                cid,
                title,
                new_tab,
            } => {
                let request = PlaybackRequest::video(title.clone(), bvid.clone(), Some(*cid));
                self.play(request, *new_tab, window, cx);
            }
            InfoEvent::ToggleFollow { mid, follow } => self.set_follow(*mid, *follow, cx),
            InfoEvent::ToggleLike { aid, like } => self.set_like(*aid, *like, cx),
            InfoEvent::OpenSpace { mid, name } => cx.emit(OpenSpace {
                mid: *mid,
                name: name.clone(),
            }),
            InfoEvent::Reload => self.refresh_info(cx),
        }
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    fn tab_info(&self, id: u64) -> Option<Entity<InfoPanel>> {
        self.tab_index(id)
            .map(|index| self.tabs[index].info.clone())
    }

    fn is_active(&self, id: u64, generation: u64) -> bool {
        let tab = self.tab();
        tab.id == id && tab.generation == generation
    }

    fn generation_matches(&self, id: u64, generation: u64) -> bool {
        self.tab_index(id)
            .is_some_and(|index| self.tabs[index].generation == generation)
    }

    pub fn play(
        &mut self,
        request: PlaybackRequest,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if new_tab {
            self.open_tab(request, window, cx);
        } else {
            self.navigate(request, cx);
        }
    }

    fn open_tab(&mut self, request: PlaybackRequest, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| same_source(&tab.request.source, &request.source))
        {
            if index != self.active {
                self.select_tab(index, cx);
            }
            return;
        }
        let previous = self.active;
        self.stash_session(cx);
        let outgoing = self
            .player
            .update(cx, |view, cx| view.swap_player(Self::new_player(), cx));
        self.keep_warm(previous, outgoing);
        let tab = self.make_tab(window, cx, request.clone());
        let index = (previous + 1).min(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active = index;
        self.tabs[index].history.open(request.source.clone());
        self.start_active(request, cx);
        self.evict_warm();
        cx.notify();
    }

    fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        let previous = self.active;
        self.stash_session(cx);
        self.activate(index, Some(previous), cx);
    }

    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        if index != self.active {
            self.tabs.remove(index);
            if index < self.active {
                self.active -= 1;
            }
            cx.notify();
            return;
        }
        self.stash_session(cx);
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            window.remove_window();
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.activate(self.active, None, cx);
    }

    fn activate(&mut self, index: usize, previous: Option<usize>, cx: &mut Context<Self>) {
        let warm = self.tabs[index].is_warm();
        let incoming = self.tabs[index]
            .player
            .take()
            .unwrap_or_else(Self::new_player);
        let outgoing = self
            .player
            .update(cx, |view, cx| view.swap_player(incoming, cx));
        if let Some(previous) = previous {
            self.keep_warm(previous, outgoing);
        }
        self.active = index;
        self.tabs[index].last_used = Instant::now();
        self.tabs[index].warm_since = None;
        if warm {
            self.resume_warm(cx);
        } else {
            self.restore_active(cx);
        }
        self.evict_warm();
        cx.notify();
    }

    fn keep_warm(&mut self, index: usize, player: Player) {
        let tab = &mut self.tabs[index];
        if tab.is_live {
            return;
        }
        tab.player = Some(player);
        tab.warm_since = Some(Instant::now());
        tab.last_used = Instant::now();
    }

    fn resume_warm(&mut self, cx: &mut Context<Self>) {
        let status = self.player.read(cx).status();
        if status.error.is_some()
            || matches!(status.phase, PlaybackPhase::Idle | PlaybackPhase::Ended)
        {
            self.restore_active(cx);
            return;
        }
        let data = {
            let tab = self.tab();
            match (&tab.request.source, tab.streams.as_ref()) {
                (
                    PlaybackSource::Video {
                        bvid,
                        cid: Some(cid),
                    },
                    Some(cached),
                ) => Some((
                    tab.id,
                    tab.generation,
                    tab.title.clone(),
                    cached.streams.clone(),
                    tab.pages.clone(),
                    bvid.clone(),
                    *cid,
                    tab.chapters.clone(),
                    tab.paused,
                )),
                _ => None,
            }
        };
        let Some((tab_id, generation, title, streams, pages, bvid, cid, chapters, paused)) = data
        else {
            self.restore_active(cx);
            return;
        };
        self.tab_mut().error = None;
        rigili_monitor::monitor!("player", "tab {tab_id}: warm resume");
        self.session = Some(tab_id);
        self.player.update(cx, |player, cx| {
            player.adopt_streams(&streams, cx);
            player.set_title(title, cx);
            player.set_pages(pages, cid, cx);
            player.set_chapters(chapters, cx);
            player.clear_danmaku_source(cx);
            player.clear_danmaku(cx);
            player.set_paused(paused, cx);
        });
        self.load_media_for(tab_id, generation, bvid, cid, cx);
    }

    fn evict_warm(&mut self) {
        while self.tabs.iter().filter(|tab| tab.player.is_some()).count() > MAX_WARM_TABS {
            let Some(index) = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(index, tab)| *index != self.active && tab.player.is_some())
                .min_by_key(|(_, tab)| tab.last_used)
                .map(|(index, _)| index)
            else {
                break;
            };
            rigili_monitor::monitor!("player", "tab {}: evicted warm player", self.tabs[index].id);
            self.tabs[index].player = None;
            self.tabs[index].warm_since = None;
        }
    }

    fn new_player() -> Player {
        Player::new(rigili_core::fonts::osd_dir().as_deref())
    }

    fn stash_session(&mut self, cx: &mut Context<Self>) {
        let status = self.player.read(cx).status().clone();
        let session = self.session;
        let active = self.active;
        if session == Some(self.tabs[active].id) {
            let player = self.player.clone();
            let view = player.read(cx);
            if let Some(cached) = self.tabs[active].streams.as_mut() {
                view.sync_streams(&mut cached.streams);
            }
        }
        let interrupted = {
            let tab = &mut self.tabs[active];
            if session == Some(tab.id) {
                tab.resume = stash_resume(status.phase, status.time_pos, tab.resume);
                tab.paused = status.paused;
                // A stream that is currently flowing was valid a moment ago, so
                // the tab can reuse it when it comes back instead of resolving.
                if !status.paused
                    && matches!(
                        status.phase,
                        PlaybackPhase::Playing | PlaybackPhase::Buffering
                    )
                    && let Some(cached) = tab.streams.as_mut()
                {
                    cached.at = Instant::now();
                }
            }
            tab.danmaku_task = None;
            let interrupted = tab.loading;
            tab.loading = false;
            if interrupted {
                tab.loaded = false;
            }
            interrupted
        };
        if interrupted {
            let info = self.tabs[active].info.clone();
            info.update(cx, |info, cx| info.set_busy(false, cx));
        }
        if !status.paused {
            self.player.update(cx, |player, cx| player.toggle_pause(cx));
        }
        self.session = None;
        self.epoch = self.epoch.wrapping_add(1);
    }

    fn navigate(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        self.tab_mut().history.open(request.source.clone());
        self.start_active(request, cx);
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        let source = self.tab_mut().history.back();
        if let Some(source) = source {
            self.start_source(source, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        let source = self.tab_mut().history.forward();
        if let Some(source) = source {
            self.start_source(source, cx);
        }
    }

    fn start_source(&mut self, source: PlaybackSource, cx: &mut Context<Self>) {
        let request = match source {
            PlaybackSource::Video { bvid, cid } => PlaybackRequest::video(String::new(), bvid, cid),
            PlaybackSource::Live { room_id } => {
                PlaybackRequest::live(format!("直播间 {room_id}"), room_id)
            }
        };
        self.start_active(request, cx);
    }

    pub(crate) fn toggle_panel(&mut self, tab: InfoTab, cx: &mut Context<Self>) {
        let current = self.tab().info.read(cx).tab();
        self.show_info = panel_visibility(self.show_info, current, tab);
        if self.show_info {
            self.tab()
                .info
                .update(cx, |info, cx| info.show_tab(tab, cx));
        }
        cx.notify();
    }

    fn start_active(&mut self, request: PlaybackRequest, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let source = request.source.clone();
        let is_live = matches!(source, PlaybackSource::Live { .. });
        let (tab_id, generation, title, info) = {
            let tab = self.tab_mut();
            tab.generation = tab.generation.wrapping_add(1);
            tab.danmaku_task = None;
            tab.loaded = false;
            tab.loading = true;
            tab.error = None;
            tab.resume = None;
            tab.paused = false;
            tab.pages.clear();
            tab.chapters.clear();
            tab.streams = None;
            tab.is_live = is_live;
            tab.request = request;
            if !tab.request.title.is_empty() {
                tab.title = tab.request.title.clone().into();
            }
            (tab.id, tab.generation, tab.title.clone(), tab.info.clone())
        };
        self.session = None;
        self.player.update(cx, |player, cx| {
            player.clear_danmaku_source(cx);
            player.clear_pages(cx);
            player.clear_danmaku(cx);
            player.set_title(title, cx);
        });
        info.update(cx, |info, cx| {
            info.clear(cx);
            info.set_busy(true, cx);
        });
        cx.notify();

        match source {
            PlaybackSource::Video { bvid, cid } => {
                let preferred = crate::settings::current(cx).preferred_quality;
                spawn_api(
                    cx,
                    async move { resolve(client, bvid, cid, preferred).await },
                    move |this, result, cx| {
                        if !this.is_active(tab_id, generation) {
                            return;
                        }
                        this.tab_mut().loading = false;
                        match result {
                            Ok(resolved) => this.apply_resolved(tab_id, generation, resolved, cx),
                            Err(error) => this.fail_active(tab_id, generation, error, cx),
                        }
                        cx.notify();
                    },
                );
            }
            PlaybackSource::Live { room_id } => {
                self.load_live(tab_id, generation, room_id, false, cx)
            }
        }
    }

    fn restore_active(&mut self, cx: &mut Context<Self>) {
        if !self.tab().loaded {
            let request = self.tab().request.clone();
            self.start_active(request, cx);
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let (tab_id, generation, request, resume, paused, pages, chapters, title, cached) = {
            let tab = self.tab_mut();
            tab.danmaku_task = None;
            tab.error = None;
            let cached = tab.streams.as_ref().and_then(|cached| {
                let age = cached.at.elapsed();
                (age < STREAMS_TTL).then(|| (cached.streams.clone(), age))
            });
            tab.loading = cached.is_none();
            (
                tab.id,
                tab.generation,
                tab.request.clone(),
                tab.resume,
                tab.paused,
                tab.pages.clone(),
                tab.chapters.clone(),
                tab.title.clone(),
                cached,
            )
        };
        self.session = None;
        let cid = match &request.source {
            PlaybackSource::Video { cid, .. } => *cid,
            PlaybackSource::Live { .. } => None,
        };
        self.player.update(cx, |player, cx| {
            player.clear_danmaku_source(cx);
            player.clear_danmaku(cx);
            player.set_title(title, cx);
            match cid {
                Some(cid) => player.set_pages(pages, cid, cx),
                None => player.clear_pages(cx),
            }
            player.set_chapters(chapters, cx);
        });
        cx.notify();

        if let Some((mut streams, age)) = cached
            && let PlaybackSource::Video {
                bvid,
                cid: Some(cid),
            } = request.source.clone()
        {
            rigili_monitor::monitor!("stream", "tab {tab_id}: reusing streams cached {age:?} ago");
            apply_preferred_quality(&mut streams, crate::settings::current(cx).preferred_quality);
            let target = StreamTarget {
                tab_id,
                generation,
                bvid,
                cid,
                resume,
                paused,
            };
            self.play_streams(target, streams, cx);
            return;
        }

        match request.source.clone() {
            PlaybackSource::Video {
                bvid,
                cid: Some(cid),
            } => {
                let preferred = crate::settings::current(cx).preferred_quality;
                let request_bvid = bvid.clone();
                spawn_api(
                    cx,
                    async move { resolve_streams(&client, &request_bvid, cid, preferred).await },
                    move |this, result, cx| {
                        if !this.is_active(tab_id, generation) {
                            return;
                        }
                        this.tab_mut().loading = false;
                        match result {
                            Ok(streams) => {
                                this.tab_mut().streams = Some(CachedStreams {
                                    streams: streams.clone(),
                                    at: Instant::now(),
                                });
                                let target = StreamTarget {
                                    tab_id,
                                    generation,
                                    bvid,
                                    cid,
                                    resume,
                                    paused,
                                };
                                this.play_streams(target, streams, cx);
                            }
                            Err(error) => this.fail_active(tab_id, generation, error, cx),
                        }
                        cx.notify();
                    },
                );
            }
            PlaybackSource::Live { room_id } => {
                self.load_live(tab_id, generation, room_id, paused, cx)
            }
            PlaybackSource::Video { cid: None, .. } => self.start_active(request, cx),
        }
    }

    fn play_streams(&mut self, target: StreamTarget, streams: Streams, cx: &mut Context<Self>) {
        let StreamTarget {
            tab_id,
            generation,
            bvid,
            cid,
            resume,
            paused,
        } = target;
        self.session = Some(tab_id);
        self.player.update(cx, |player, cx| {
            player.load_streams_at(streams, false, resume, paused, cx)
        });
        self.load_media_for(tab_id, generation, bvid, cid, cx);
    }

    fn load_media_for(
        &mut self,
        tab_id: u64,
        generation: u64,
        bvid: String,
        cid: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(info) = self.tab_info(tab_id) else {
            return;
        };
        let (aid, duration) = {
            let info = info.read(cx);
            (
                info.aid().unwrap_or_default(),
                info.page_duration(cid).unwrap_or_default(),
            )
        };
        self.load_previews_for(tab_id, generation, bvid.clone(), cid, cx);
        self.load_danmaku_for(
            media::DanmakuRequest {
                tab_id,
                generation,
                bvid,
                aid,
                cid,
                duration,
            },
            cx,
        );
    }

    fn load_live(
        &mut self,
        tab_id: u64,
        generation: u64,
        room_id: u64,
        paused: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        spawn_api(
            cx,
            async move { client.live_play_url(room_id).await },
            move |this, result, cx| {
                if !this.is_active(tab_id, generation) {
                    return;
                }
                this.tab_mut().loading = false;
                match result {
                    Ok(url) => {
                        this.session = Some(tab_id);
                        {
                            let tab = this.tab_mut();
                            tab.loaded = true;
                            tab.info.update(cx, |info, cx| info.clear(cx));
                        }
                        this.player.update(cx, |player, cx| {
                            player.load_streams_at(Streams::single(url), true, None, paused, cx)
                        });
                    }
                    Err(error) => this.fail_active(tab_id, generation, error, cx),
                }
                cx.notify();
            },
        );
    }

    fn apply_resolved(
        &mut self,
        tab_id: u64,
        generation: u64,
        resolved: Resolved,
        cx: &mut Context<Self>,
    ) {
        if !self.is_active(tab_id, generation) {
            return;
        }
        let Resolved {
            info: video,
            related,
            followers,
            owner_likes,
            cid,
            streams,
            player_info,
        } = resolved;
        let bvid = video.bvid.clone();
        let aid = video.aid;
        let duration = video
            .pages
            .iter()
            .find(|page| page.cid == cid)
            .map(|page| page.duration)
            .unwrap_or(video.duration);
        let start = watch::resume_position(player_info.last_play_time, duration as f64);
        let chapters = player_info.chapters();
        let pages = video.pages.clone();
        let title: SharedString = video.title.clone().into();
        let owner_mid = video.owner.mid;
        {
            let tab = self.tab_mut();
            tab.loaded = true;
            tab.loading = false;
            tab.title = title.clone();
            tab.request.source = PlaybackSource::Video {
                bvid: bvid.clone(),
                cid: Some(cid),
            };
            tab.pages = pages.clone();
            tab.chapters = chapters.clone();
            tab.streams = Some(CachedStreams {
                streams: streams.clone(),
                at: Instant::now(),
            });
            tab.is_live = false;
            tab.info.update(cx, |info, cx| {
                info.set(video, cid, cx);
                info.set_owner_counts(followers, owner_likes, cx);
                info.set_related(related, cx);
            });
        }
        self.session = Some(tab_id);
        self.player.update(cx, |player, cx| {
            player.set_title(title, cx);
            player.set_pages(pages, cid, cx);
            player.set_chapters(chapters, cx);
            player.load_streams(streams, false, start, cx);
        });
        self.load_previews_for(tab_id, generation, bvid.clone(), cid, cx);
        self.load_danmaku_for(
            media::DanmakuRequest {
                tab_id,
                generation,
                bvid,
                aid,
                cid,
                duration,
            },
            cx,
        );
        self.load_follow_status_for(tab_id, generation, owner_mid, cx);
        self.load_interactions_for(tab_id, generation, aid, cx);
        self.load_watch_later_for(tab_id, generation, aid, cx);
    }

    fn fail_active(
        &mut self,
        tab_id: u64,
        generation: u64,
        error: ApiTaskError,
        cx: &mut Context<Self>,
    ) {
        if !self.is_active(tab_id, generation) {
            return;
        }
        let message = api_task_error_message(error, cx);
        let info = {
            let tab = self.tab_mut();
            tab.loading = false;
            tab.error = Some(message);
            tab.info.clone()
        };
        info.update(cx, |info, cx| info.set_busy(false, cx));
    }

    fn step_page(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(cid) = self.player.read(cx).adjacent_page(delta) else {
            return;
        };
        self.select_page(cid, cx);
    }

    fn select_page(&mut self, cid: u64, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let (bvid, current_cid) = {
            let tab = self.tab();
            if tab.is_live || tab.loading {
                return;
            }
            let PlaybackSource::Video { bvid, cid } = tab.request.source.clone() else {
                return;
            };
            (bvid, cid)
        };
        if current_cid == Some(cid) {
            return;
        }

        let (tab_id, generation, info, duration) = {
            let tab = self.tab_mut();
            tab.request.source = PlaybackSource::Video {
                bvid: bvid.clone(),
                cid: Some(cid),
            };
            tab.generation = tab.generation.wrapping_add(1);
            tab.danmaku_task = None;
            tab.loading = true;
            tab.error = None;
            tab.paused = false;
            tab.resume = None;
            tab.streams = None;
            let duration = tab.info.read(cx).page_duration(cid).unwrap_or_default();
            (tab.id, tab.generation, tab.info.clone(), duration)
        };
        self.session = None;
        self.player
            .update(cx, |player, cx| player.clear_danmaku_source(cx));
        info.update(cx, |info, cx| info.set_busy(true, cx));
        cx.notify();

        let media_bvid = bvid.clone();
        let preferred = crate::settings::current(cx).preferred_quality;
        spawn_api(
            cx,
            async move {
                let (streams, player_info) = tokio::join!(
                    resolve_streams(&client, &bvid, cid, preferred),
                    client.player_info(&bvid, cid)
                );
                streams.map(|streams| (streams, player_info.unwrap_or_default()))
            },
            move |this, result, cx| {
                if !this.is_active(tab_id, generation) {
                    return;
                }
                this.tab_mut().loading = false;
                match result {
                    Ok((streams, player_info)) => {
                        let start =
                            watch::resume_position(player_info.last_play_time, duration as f64);
                        let chapters = player_info.chapters();
                        {
                            let tab = this.tab_mut();
                            tab.chapters = chapters.clone();
                            tab.history.remember_cid(cid);
                            tab.streams = Some(CachedStreams {
                                streams: streams.clone(),
                                at: Instant::now(),
                            });
                        }
                        info.update(cx, |info, cx| info.select(cid, cx));
                        this.session = Some(tab_id);
                        this.player.update(cx, |player, cx| {
                            player.set_selected_page(cid, cx);
                            player.set_chapters(chapters, cx);
                            player.load_streams(streams, false, start, cx)
                        });
                        this.load_media_for(tab_id, generation, media_bvid, cid, cx);
                    }
                    Err(error) => {
                        info.update(cx, |info, cx| info.set_busy(false, cx));
                        this.fail_active(tab_id, generation, error, cx);
                    }
                }
                cx.notify();
            },
        );
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let request = self.tab().request.clone();
        self.start_active(request, cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let (loading, is_live) = {
            let tab = self.tab();
            (tab.loading, tab.is_live)
        };
        if loading {
            return;
        }
        if is_live {
            let request = self.tab().request.clone();
            self.start_active(request, cx);
            return;
        }
        self.tab()
            .info
            .update(cx, |info, cx| info.reload_current(cx));
        self.reload_stream(cx);
    }

    fn reload_stream(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let (tab_id, generation, bvid, cid) = {
            let tab = self.tab_mut();
            let PlaybackSource::Video {
                bvid,
                cid: Some(cid),
            } = tab.request.source.clone()
            else {
                return;
            };
            tab.danmaku_task = None;
            tab.loading = true;
            tab.error = None;
            tab.streams = None;
            (tab.id, tab.generation, bvid, cid)
        };
        let preferred = self.player.read(cx).selected_quality();
        self.session = Some(tab_id);
        self.player
            .update(cx, |player, cx| player.clear_danmaku_source(cx));
        cx.notify();

        spawn_api(
            cx,
            async move { resolve_streams(&client, &bvid, cid, Some(preferred)).await },
            move |this, result, cx| {
                if !this.is_active(tab_id, generation) {
                    return;
                }
                this.tab_mut().loading = false;
                match result {
                    Ok(streams) => {
                        this.tab_mut().streams = Some(CachedStreams {
                            streams: streams.clone(),
                            at: Instant::now(),
                        });
                        this.session = Some(tab_id);
                        this.player
                            .update(cx, |player, cx| player.reload_streams(streams, cx));
                    }
                    Err(error) => this.fail_active(tab_id, generation, error, cx),
                }
                cx.notify();
            },
        );
    }
}

impl Render for PlayerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let fullscreen = window.is_fullscreen();
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let wide_enough = window.viewport_size().width >= INFO_MIN_WINDOW;
        let (title, can_go_back, can_go_forward, is_live, loading, error, info) = {
            let tab = self.tab();
            (
                tab.title.clone(),
                tab.history.can_back(),
                tab.history.can_forward(),
                tab.is_live,
                tab.loading,
                tab.error.clone(),
                tab.info.clone(),
            )
        };
        let show_info = self.show_info && !is_live && (fullscreen || wide_enough);
        let weak = cx.entity().downgrade();

        if window.window_title() != title.as_ref() {
            window.set_window_title(title.as_ref());
        }

        let tabs = TabBar::new("player-tabs")
            .selected_index(self.active)
            .max_width(px(200.))
            .menu(true)
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                TabButton::new().label(tab.title.clone()).suffix(
                    Button::new(("player-tab-close", tab.id))
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .accessibility_label("关闭标签页")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close_tab(index, window, cx);
                        })),
                )
            }))
            .on_click(cx.listener(|this, index: &usize, _, cx| this.select_tab(*index, cx)));

        v_flex()
            .image_cache(rigili_core::image_cache::get(cx))
            .id("player-window")
            .size_full()
            .bg(background)
            .text_color(foreground)
            .key_context(crate::shortcuts::PLAYER)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && (this.player.read(cx).text_input_focused(window, cx)
                        || this.tab().info.read(cx).text_input_focused(window, cx))
                {
                    window.blur(cx);
                    this.player
                        .update(cx, |player, cx| player.focus_player(window, cx));
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.player
                        .update(cx, |player, cx| player.focus_player(window, cx));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.tab().info.update(cx, |info, cx| {
                        info.clear_spanning_selection(window, cx);
                    });
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_pause(cx));
            }))
            .on_action(cx.listener(|this, _: &SeekBack, _, cx| {
                this.player.update(cx, |player, cx| player.seek_by(-5., cx));
            }))
            .on_action(cx.listener(|this, _: &HistoryBack, _, cx| {
                this.go_back(cx);
            }))
            .on_action(cx.listener(|this, _: &HistoryForward, _, cx| {
                this.go_forward(cx);
            }))
            .on_action(cx.listener(|this, _: &SpeedDown, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(false, cx));
            }))
            .on_action(cx.listener(|this, _: &SpeedUp, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(true, cx));
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                this.step_page(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                this.step_page(1, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_mute(cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_fullscreen(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleInfo, _, cx| {
                this.toggle_panel(InfoTab::Info, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleComments, _, cx| {
                this.toggle_panel(InfoTab::Comments, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleDanmaku, _, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_danmaku(cx));
            }))
            .on_action(cx.listener(|this, _: &Reload, _, cx| {
                this.reload(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_tab(this.active, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| {
                if this.tabs.len() > 1 {
                    this.select_tab((this.active + 1) % this.tabs.len(), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
                if this.tabs.len() > 1 {
                    this.select_tab((this.active + this.tabs.len() - 1) % this.tabs.len(), cx);
                }
            }))
            .on_action(cx.listener(|_, _: &SwitchToMain, _, cx| {
                cx.emit(FocusMain);
            }))
            .on_action(
                cx.listener(|_, _: &rigili_core::ShowShortcuts, window, cx| {
                    let rows = crate::shortcuts::rows(cx);
                    rigili_core::open_shortcut_help(window, cx, "播放器快捷键", rows);
                }),
            )
            .when(!fullscreen, |this| {
                this.child(
                    TitleBar::new().child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .pr_6()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("player-back")
                                            .ghost()
                                            .icon(IconName::ChevronLeft)
                                            .accessibility_label("上一个视频")
                                            .tooltip("上一个视频")
                                            .disabled(!can_go_back)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_back(cx)),
                                            ),
                                    )
                                    .child(
                                        Button::new("player-forward")
                                            .ghost()
                                            .icon(IconName::ChevronRight)
                                            .accessibility_label("下一个视频")
                                            .tooltip("下一个视频")
                                            .disabled(!can_go_forward)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_forward(cx)),
                                            ),
                                    ),
                            )
                            .child(div().flex_1().min_w_0().child(tabs))
                            .when(!client_decorated, |this| {
                                this.child(
                                    Button::new("window-close")
                                        .ghost()
                                        .icon(IconName::Close)
                                        .accessibility_label("关闭窗口")
                                        .tooltip("关闭")
                                        .on_click(|_, window, _| window.remove_window()),
                                )
                            }),
                    ),
                )
            })
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
                            .when(loading, |this| {
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
                                    cx,
                                ))
                            })
                            .children(error.clone().map(|error| {
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
                                    cx,
                                )
                            })),
                    )
                    .when(show_info, |this| this.child(info)),
            )
    }
}

fn overlay(content: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .p_6()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .child(content),
        )
}

fn panel_visibility(visible: bool, current: InfoTab, target: InfoTab) -> bool {
    !(visible && current == target)
}

fn same_source(a: &PlaybackSource, b: &PlaybackSource) -> bool {
    match (a, b) {
        (PlaybackSource::Video { bvid: a, .. }, PlaybackSource::Video { bvid: b, .. }) => a == b,
        (PlaybackSource::Live { room_id: a }, PlaybackSource::Live { room_id: b }) => a == b,
        _ => false,
    }
}

fn stash_resume(phase: PlaybackPhase, time_pos: f64, resume: Option<f64>) -> Option<f64> {
    match phase {
        PlaybackPhase::Ended => Some(0.),
        PlaybackPhase::Playing | PlaybackPhase::Buffering => Some(time_pos.max(0.)),
        _ => resume,
    }
}

fn apply_preferred_quality(streams: &mut Streams, preferred: Option<u32>) {
    let Some((url, id)) = preferred
        .and_then(|preferred| {
            streams
                .qualities
                .iter()
                .find(|quality| quality.id == preferred)
        })
        .map(|quality| (quality.url.clone(), quality.id))
    else {
        return;
    };
    streams.video = url;
    streams.video_id = id;
}

pub fn open(
    request: PlaybackRequest,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<PlayerWindow>)> {
    let title = request.title.clone();
    let mut options = TitleBar::window_options();
    options.app_id = Some("rigili".into());
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
        cx.new(|cx| PlayerWindow::new(window, cx, request.clone()))
    })
}

#[cfg(test)]
mod tests {
    use rigili_player::PlaybackPhase;

    use super::{
        InfoTab, PlaybackSource, apply_preferred_quality, panel_visibility, same_source,
        stash_resume,
    };
    use crate::request::{Streams, VideoQuality};

    #[test]
    fn identifies_tabs_by_video_or_room() {
        let video = PlaybackSource::Video {
            bvid: "BV1".into(),
            cid: Some(1),
        };
        let same_video = PlaybackSource::Video {
            bvid: "BV1".into(),
            cid: Some(2),
        };
        let other_video = PlaybackSource::Video {
            bvid: "BV2".into(),
            cid: Some(1),
        };
        let live = PlaybackSource::Live { room_id: 7 };
        assert!(same_source(&video, &same_video));
        assert!(!same_source(&video, &other_video));
        assert!(!same_source(&video, &live));
        assert!(same_source(&live, &PlaybackSource::Live { room_id: 7 }));
    }

    #[test]
    fn stashes_the_playback_position_only_while_playing() {
        assert_eq!(stash_resume(PlaybackPhase::Playing, 12.5, None), Some(12.5));
        assert_eq!(stash_resume(PlaybackPhase::Ended, 99., Some(3.)), Some(0.));
        assert_eq!(stash_resume(PlaybackPhase::Loading, 0., Some(3.)), Some(3.));
        assert_eq!(stash_resume(PlaybackPhase::Idle, 0., None), None);
    }

    #[test]
    fn applies_the_preferred_quality_to_cached_streams() {
        let quality = |id: u32, url: &str| VideoQuality {
            id,
            label: format!("{id}P"),
            short_label: format!("{id}P"),
            url: url.into(),
        };
        let mut streams = Streams {
            video: "high".into(),
            video_id: 80,
            audio: None,
            qualities: vec![quality(80, "high"), quality(64, "low")],
        };

        apply_preferred_quality(&mut streams, Some(64));
        assert_eq!(streams.video_id, 64);
        assert_eq!(streams.video, "low");

        apply_preferred_quality(&mut streams, Some(32));
        assert_eq!(streams.video_id, 64);
        assert_eq!(streams.video, "low");

        apply_preferred_quality(&mut streams, None);
        assert_eq!(streams.video_id, 64);
    }

    #[test]
    fn toggles_only_the_active_panel_tab() {
        assert!(panel_visibility(false, InfoTab::Info, InfoTab::Info));
        assert!(!panel_visibility(true, InfoTab::Info, InfoTab::Info));
        assert!(panel_visibility(true, InfoTab::Comments, InfoTab::Info));
        assert!(!panel_visibility(
            true,
            InfoTab::Comments,
            InfoTab::Comments
        ));
    }
}
