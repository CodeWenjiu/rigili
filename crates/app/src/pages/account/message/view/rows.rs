use std::collections::HashMap;

use rigili_bilibili::{
    MessageCursor, MessageEnvelope, MessageFeed, MessageSessions, MessageUserInfo,
};
use rigili_core::format_message_time;

use super::{MessageRow, MessageTab};
use crate::shared::browse::paging::Page;

pub(super) fn session_rows(
    sessions: MessageSessions,
    infos: Vec<MessageUserInfo>,
) -> Vec<MessageRow> {
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

pub(super) fn feed_page(
    tab: MessageTab,
    feed: MessageFeed,
) -> Page<MessageRow, Option<MessageCursor>> {
    let next_cursor = (!feed.cursor.is_end && feed.cursor.id > 0).then_some(feed.cursor);
    let rows: Vec<MessageRow> = feed
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

    Page::new(rows, next_cursor.is_some(), next_cursor)
}
