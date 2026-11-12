use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::message::MessageFeedKind;
use rigili_bilibili::{
    MessageCursor, MessageEnvelope, MessageFeed, MessageSessions, MessageUserInfo,
};
use rigili_core::backend;
use rigili_core::login as login_state;
use rigili_core::runtime::{ApiTaskError, spawn_api};
use rigili_core::{
    OpenSpace, Refresh, api_error_message, api_task_error_message, format_message_time,
};

use crate::grid::{LIST_FOOTER_HEIGHT, list_content_size, page_gutter, virtual_list_scrollbar};
use crate::login;

const ROW_HEIGHT: Pixels = px(88.);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MessageTab {
    #[default]
    Chat,
    Reply,
    At,
    Like,
}

impl MessageTab {
    const ALL: [Self; 4] = [Self::Chat, Self::Reply, Self::At, Self::Like];

    fn label(self) -> &'static str {
        match self {
            Self::Chat => "聊天列表",
            Self::Reply => "回复我的",
            Self::At => "@我的",
            Self::Like => "收到的赞",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Chat => 0,
            Self::Reply => 1,
            Self::At => 2,
            Self::Like => 3,
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Self::Chat => "message-chat-list",
            Self::Reply => "message-reply-list",
            Self::At => "message-at-list",
            Self::Like => "message-like-list",
        }
    }

    fn feed_kind(self) -> Option<MessageFeedKind> {
        match self {
            Self::Chat => None,
            Self::Reply => Some(MessageFeedKind::Reply),
            Self::At => Some(MessageFeedKind::At),
            Self::Like => Some(MessageFeedKind::Like),
        }
    }
}

struct MessageRow {
    key: String,
    name: String,
    avatar: String,
    preview: String,
    time: String,
    unread: u32,
    mid: Option<u64>,
}

struct MessagePage {
    rows: Vec<MessageRow>,
    next_cursor: Option<MessageCursor>,
}

struct MessageState {
    rows: Vec<MessageRow>,
    next_cursor: Option<MessageCursor>,
    loading: bool,
    loading_more: bool,
    loaded: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    scroll: VirtualListScrollHandle,
}

impl MessageState {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            next_cursor: None,
            loading: false,
            loading_more: false,
            loaded: false,
            error: None,
            more_error: None,
            generation: 0,
            scroll: VirtualListScrollHandle::new(),
        }
    }

    fn reset(&mut self) -> u64 {
        self.generation += 1;
        self.rows.clear();
        self.next_cursor = None;
        self.loading = true;
        self.loading_more = false;
        self.loaded = false;
        self.error = None;
        self.more_error = None;
        self.generation
    }
}

pub(crate) struct MessagesView {
    tab: MessageTab,
    tabs: [MessageState; 4],
    user_mid: Option<u64>,
}

impl EventEmitter<OpenSpace> for MessagesView {}

impl MessagesView {
    pub(crate) fn new() -> Self {
        Self {
            tab: MessageTab::default(),
            tabs: std::array::from_fn(|_| MessageState::new()),
            user_mid: None,
        }
    }

    fn state(&self, tab: MessageTab) -> &MessageState {
        &self.tabs[tab.index()]
    }

    fn state_mut(&mut self, tab: MessageTab) -> &mut MessageState {
        &mut self.tabs[tab.index()]
    }

    fn sync_user(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mid = login_state::user(cx).map(|user| user.mid);
        if mid == self.user_mid {
            return;
        }

        self.user_mid = mid;
        self.tab = MessageTab::default();
        self.tabs = std::array::from_fn(|_| MessageState::new());

        if mid.is_some() {
            let weak = cx.entity().downgrade();
            window.defer(cx, move |_, cx| {
                let _ = weak.update(cx, |this, cx| this.load(MessageTab::default(), cx));
            });
        }
    }

    fn show_tab(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }

        self.tab = tab;
        let state = self.state(tab);
        let needs_load = !state.loaded && !state.loading && state.error.is_none();
        if needs_load && self.user_mid.is_some() {
            self.load(tab, cx);
        } else {
            cx.notify();
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        self.load(tab, cx);
    }

    fn load(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                let state = self.state_mut(tab);
                state.loading = false;
                state.error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };

        let generation = self.state_mut(tab).reset();
        let future = async move {
            match tab.feed_kind() {
                Some(kind) => backend
                    .message_feed(kind, None)
                    .await
                    .map(|feed| feed_page(tab, feed)),
                None => {
                    let sessions = backend.message_sessions().await?;
                    let missing = sessions
                        .session_list
                        .iter()
                        .filter(|session| session.account_info.is_none() && session.talker_id > 0)
                        .map(|session| session.talker_id)
                        .collect::<Vec<_>>();
                    let infos = if missing.is_empty() {
                        Vec::new()
                    } else {
                        backend
                            .message_user_infos(missing)
                            .await
                            .unwrap_or_default()
                    };
                    Ok(MessagePage {
                        rows: session_rows(sessions, infos),
                        next_cursor: None,
                    })
                }
            }
        };
        spawn_api(cx, future, move |this, result, cx| {
            this.finish(tab, generation, result, cx);
        });
        cx.notify();
    }

    fn load_more(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        let (cursor, generation) = {
            let state = self.state(tab);
            if state.loading
                || state.loading_more
                || state.more_error.is_some()
                || state.rows.is_empty()
            {
                return;
            }
            let Some(cursor) = state.next_cursor else {
                return;
            };
            (cursor, state.generation)
        };
        let Some(kind) = tab.feed_kind() else {
            return;
        };
        let backend = match backend::get(cx) {
            Ok(backend) => backend,
            Err(error) => {
                self.state_mut(tab).more_error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };

        self.state_mut(tab).loading_more = true;
        cx.notify();

        let future = async move {
            backend
                .message_feed(kind, Some(cursor))
                .await
                .map(|feed| feed_page(tab, feed))
        };
        spawn_api(cx, future, move |this, result, cx| {
            this.finish_more(tab, generation, cursor, result, cx);
        });
    }

    fn finish_more(
        &mut self,
        tab: MessageTab,
        generation: u64,
        cursor: MessageCursor,
        result: Result<MessagePage, ApiTaskError>,
        cx: &mut Context<Self>,
    ) {
        let state = self.state_mut(tab);
        if state.generation != generation {
            return;
        }

        state.loading_more = false;
        match result {
            Ok(page) => {
                let known = state
                    .rows
                    .iter()
                    .map(|row| row.key.clone())
                    .collect::<HashSet<_>>();
                state.rows.extend(
                    page.rows
                        .into_iter()
                        .filter(|row| !known.contains(&row.key)),
                );
                state.next_cursor = page.next_cursor.filter(|next| *next != cursor);
            }
            Err(error) => state.more_error = Some(api_task_error_message(error, cx)),
        }
        cx.notify();
    }

    fn retry_more(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        self.state_mut(tab).more_error = None;
        self.load_more(tab, cx);
    }

    fn maybe_load_more(
        &mut self,
        tab: MessageTab,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && last_visible + 1 >= rows {
            self.load_more(tab, cx);
        }
    }

    fn finish(
        &mut self,
        tab: MessageTab,
        generation: u64,
        result: Result<MessagePage, ApiTaskError>,
        cx: &mut Context<Self>,
    ) {
        let state = self.state_mut(tab);
        if state.generation != generation {
            return;
        }

        state.loading = false;
        match result {
            Ok(page) => {
                state.rows = page.rows;
                state.next_cursor = page.next_cursor;
                state.loaded = true;
                state.error = None;
                state.scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            Err(error) => {
                state.error = Some(api_task_error_message(error, cx));
            }
        }
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::MEDIUM)
                    .child("消息"),
            )
            .child(
                h_flex()
                    .items_center()
                    .when(login_state::user(cx).is_some(), |this| {
                        this.child(
                            Button::new("messages-refresh")
                                .ghost()
                                .icon(IconName::RefreshCw)
                                .accessibility_label("刷新消息")
                                .tooltip("刷新消息")
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .items_end()
            .gap_5()
            .border_b_1()
            .border_color(border)
            .children(MessageTab::ALL.map(|tab| {
                let active = tab == self.tab;
                let group = SharedString::from(format!("message-tab-{}", tab.index()));

                v_flex()
                    .group(group.clone())
                    .items_center()
                    .gap_1()
                    .rounded(radius)
                    .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                    .child(
                        Button::new(("message-tab", tab.index()))
                            .text()
                            .px_2()
                            .py_1()
                            .accessibility_label(tab.label())
                            .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { primary } else { muted })
                                    .group_hover(group, move |this| this.text_color(primary))
                                    .child(tab.label()),
                            ),
                    )
                    .child(div().h(px(2.)).w(px(32.)).rounded(px(1.)).bg(if active {
                        primary
                    } else {
                        hsla(0., 0., 0., 0.)
                    }))
            }))
            .into_any_element()
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if login_state::user(cx).is_none() {
            return self.render_login_prompt(cx);
        }

        let tab = self.tab;
        let state = self.state(tab);
        let error = state.error.clone();
        let loaded = state.loaded;
        let empty = state.rows.is_empty();
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if let Some(error) = error {
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("messages-retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                )
                .into_any_element();
        }

        if !loaded && empty {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element();
        }

        if empty {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("暂无消息")
                .into_any_element();
        }

        self.render_list(window, cx)
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.tab;
        let state = self.state(tab);
        let rows = state.rows.len();
        let footer = state.loading_more || state.more_error.is_some();
        let footer_index = rows;
        let mut item_sizes = vec![size(px(0.), ROW_HEIGHT); rows];
        if footer {
            item_sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let sizes = Rc::new(item_sizes);
        let content_size = list_content_size(&sizes, px(0.));
        let gutter = page_gutter(window);
        let view = cx.entity();
        let list = v_virtual_list(view, tab.list_id(), sizes, move |this, range, _, cx| {
            this.maybe_load_more(tab, range.end.saturating_sub(1), rows, cx);
            range
                .map(|index| {
                    if footer && index == footer_index {
                        this.render_footer(tab, cx)
                    } else {
                        this.render_row(tab, index, cx)
                    }
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.state(tab).scroll);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .mr(px(-gutter.as_f32()))
            .pr(gutter)
            .overflow_hidden()
            .child(list)
            .child(virtual_list_scrollbar(
                &self.state(tab).scroll,
                content_size,
            ))
            .into_any_element()
    }

    fn render_footer(&self, tab: MessageTab, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state(tab);
        let more_error = state.more_error.clone();
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        h_flex()
            .w_full()
            .h(LIST_FOOTER_HEIGHT)
            .items_center()
            .justify_center()
            .gap_2()
            .when_some(more_error, |this, error| {
                this.child(
                    Button::new("messages-more-retry")
                        .text()
                        .on_click(cx.listener(move |this, _, _, cx| this.retry_more(tab, cx)))
                        .child(div().text_xs().text_color(danger).child(error)),
                )
            })
            .when(state.more_error.is_none(), |this| {
                this.child(Spinner::new().small())
                    .child(div().text_xs().text_color(muted).child("加载中"))
            })
            .into_any_element()
    }

    fn render_row(&self, tab: MessageTab, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.state(tab).rows[index];
        let key = SharedString::from(row.key.clone());
        let name = row.name.clone();
        let avatar = row.avatar.clone();
        let preview = row.preview.clone();
        let tooltip_preview = preview.clone();
        let single_line = preview.replace(['\n', '\r', '\t'], " ");
        let time = row.time.clone();
        let unread = row.unread;
        let mid = row.mid;
        let click_name = name.clone();
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;
        let danger = cx.theme().danger;
        let danger_foreground = cx.theme().danger_foreground;

        h_flex()
            .id(ElementId::Name(key))
            .w_full()
            .h(ROW_HEIGHT)
            .items_center()
            .gap_3()
            .px_3()
            .rounded(radius)
            .tooltip(move |window, cx| Tooltip::new(tooltip_preview.clone()).build(window, cx))
            .when(mid.is_some(), move |this| {
                this.cursor_pointer().hover(move |this| this.bg(hover_bg))
            })
            .when_some(mid, move |this, mid| {
                this.on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(OpenSpace {
                        mid,
                        name: click_name.clone(),
                    });
                }))
            })
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child({
                        let mut avatar_element = Avatar::new().name(name.clone()).size(px(48.));
                        if !avatar.is_empty() {
                            avatar_element = avatar_element.src(avatar);
                        }
                        avatar_element
                    })
                    .when(unread > 0, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-2.))
                                .right(px(-2.))
                                .min_w(px(18.))
                                .h(px(18.))
                                .px(px(4.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(danger)
                                .text_color(danger_foreground)
                                .text_xs()
                                .child(if unread > 99 {
                                    "99+".to_owned()
                                } else {
                                    unread.to_string()
                                }),
                        )
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(foreground)
                            .truncate()
                            .child(name),
                    )
                    .child({
                        div()
                            .w_full()
                            .min_w_0()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(single_line)
                    }),
            )
            .child(div().text_xs().text_color(muted).child(time))
            .when(mid.is_some(), |this| {
                this.child(
                    Icon::new(IconName::ChevronRight)
                        .with_size(px(16.))
                        .text_color(muted),
                )
            })
            .into_any_element()
    }

    fn render_login_prompt(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Icon::new(IconName::Mail)
                    .with_size(px(48.))
                    .text_color(muted),
            )
            .child(div().text_sm().text_color(muted).child("登录后查看消息"))
            .child(
                Button::new("messages-sign-in")
                    .primary()
                    .label("登录")
                    .on_click(cx.listener(|_, _, window, cx| {
                        login::open(window, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Refresh for MessagesView {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        self.load(tab, cx);
    }

    fn can_refresh(&self) -> bool {
        self.user_mid.is_some()
    }
}

impl Render for MessagesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_user(window, cx);

        let background = cx.theme().popover;
        let foreground = cx.theme().foreground;
        let radius = cx.theme().radius_lg;
        let content = self.render_content(window, cx);

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .rounded(radius)
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(cx))
            .child(self.render_tabs(cx))
            .child(content)
    }
}

fn session_rows(sessions: MessageSessions, infos: Vec<MessageUserInfo>) -> Vec<MessageRow> {
    let infos = infos
        .into_iter()
        .map(|info| (info.uid, info))
        .collect::<HashMap<_, _>>();
    sessions
        .session_list
        .into_iter()
        .map(|session| {
            let timestamp = session.timestamp();
            let unread = session.unread_count;
            let preview = session
                .last_msg
                .as_ref()
                .map(MessageEnvelope::preview)
                .filter(|preview| !preview.is_empty())
                .unwrap_or_else(|| "暂无消息".into());
            let talker_id = session.talker_id;
            let account = session.account_info.unwrap_or_default();
            let fallback = infos.get(&talker_id);
            let name = if !account.name.is_empty() {
                account.name
            } else if let Some(info) = fallback.filter(|info| !info.uname.is_empty()) {
                info.uname.clone()
            } else {
                "系统消息".to_owned()
            };
            let avatar = if account.pic_url.is_empty() {
                fallback.map(|info| info.face.clone()).unwrap_or_default()
            } else {
                account.pic_url
            };

            MessageRow {
                key: format!("session-{}-{}", talker_id, session.system_msg_type),
                name,
                avatar,
                preview,
                time: format_message_time(timestamp),
                unread,
                mid: (talker_id > 0 && session.system_msg_type == 0).then_some(talker_id),
            }
        })
        .collect()
}

fn feed_page(tab: MessageTab, feed: MessageFeed) -> MessagePage {
    let next_cursor = (!feed.cursor.is_end && feed.cursor.id > 0).then_some(feed.cursor);
    let rows = feed
        .items
        .into_iter()
        .map(|item| {
            let timestamp = item.timestamp();
            let liker_name = item
                .users
                .first()
                .map(|user| user.nickname.clone())
                .filter(|name| !name.is_empty());
            let liker_avatar = item.users.first().map(|user| user.avatar.clone());
            let liker_mid = item
                .users
                .first()
                .map(|user| user.mid)
                .filter(|mid| *mid > 0);
            let preview = match tab {
                MessageTab::Reply | MessageTab::At => {
                    if !item.reply.content.is_empty() {
                        item.reply.content.clone()
                    } else if !item.item.title.is_empty() {
                        item.item.title.clone()
                    } else {
                        "回复了你的内容".into()
                    }
                }
                MessageTab::Like => {
                    let business = item.item.business.trim();
                    let title = item.item.title.trim();
                    match (business.is_empty(), title.is_empty()) {
                        (false, false) => format!("赞了你的{business}：{title}"),
                        (false, true) => format!("赞了你的{business}"),
                        (true, false) => title.to_owned(),
                        (true, true) => "赞了你的内容".into(),
                    }
                }
                MessageTab::Chat => "新消息".into(),
            };
            let name = liker_name
                .or_else(|| (!item.user.nickname.is_empty()).then(|| item.user.nickname.clone()))
                .unwrap_or_else(|| "用户".into());
            let avatar = liker_avatar.unwrap_or_else(|| item.user.avatar.clone());
            let mid = liker_mid.or_else(|| (item.user.mid > 0).then_some(item.user.mid));
            let key_mid = mid.unwrap_or(0);

            MessageRow {
                key: format!("{}-{}-{}", tab.index(), item.id, key_mid),
                name,
                avatar,
                preview,
                time: format_message_time(timestamp),
                unread: 0,
                mid,
            }
        })
        .collect();

    MessagePage { rows, next_cursor }
}
