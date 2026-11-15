rigili_macro::mod_prv!(render, rows);

use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::MessageCursor;
use rigili_bilibili::api::message::MessageFeedKind;
use rigili_core::client;
use rigili_core::login as login_state;
use rigili_core::{OpenSpace, Refresh, api_error_message};

use crate::shared::browse::paging::{Page, Paged, spawn_first, spawn_more};

use rows::{feed_page, session_rows};

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

type MessageState = Paged<MessageRow, Option<MessageCursor>>;

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
        let needs_load = self.state(tab).needs_first_load();
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
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.state_mut(tab)
                    .fail_first_reset(api_error_message(error));
                cx.notify();
                return;
            }
        };

        let future = async move {
            match tab.feed_kind() {
                Some(kind) => {
                    let feed = client.message_feed(kind, None).await?;
                    Ok(feed_page(tab, feed))
                }
                None => {
                    let sessions = client.message_sessions().await?;
                    let missing = sessions
                        .session_list
                        .iter()
                        .filter(|session| session.account_info.is_none() && session.talker_id > 0)
                        .map(|session| session.talker_id)
                        .collect::<Vec<_>>();
                    let infos = if missing.is_empty() {
                        Vec::new()
                    } else {
                        client
                            .message_user_infos(&missing)
                            .await
                            .unwrap_or_default()
                    };
                    Ok(Page::new(session_rows(sessions, infos), false, None))
                }
            }
        };

        spawn_first(
            self,
            cx,
            move |this| this.state_mut(tab),
            |row: &MessageRow| row.key.clone(),
            future,
            move |this| {
                this.state_mut(tab)
                    .scroll
                    .scroll_to_item(0, ScrollStrategy::Top);
            },
        );
    }

    fn load_more(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        let Some(kind) = tab.feed_kind() else {
            return;
        };
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.state_mut(tab).fail_more(api_error_message(error));
                cx.notify();
                return;
            }
        };

        spawn_more(
            self,
            cx,
            move |this| this.state_mut(tab),
            |row: &MessageRow| row.key.clone(),
            move |cursor| {
                let cursor = cursor.unwrap_or_default();
                async move {
                    let feed = client.message_feed(kind, Some(cursor)).await?;
                    let page = feed_page(tab, feed);
                    let next = page.meta.filter(|next| *next != cursor);
                    Ok(Page::new(page.items, next.is_some(), next))
                }
            },
            |_| {},
        );
    }

    fn retry_more(&mut self, tab: MessageTab, cx: &mut Context<Self>) {
        self.state_mut(tab).retry_more();
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
