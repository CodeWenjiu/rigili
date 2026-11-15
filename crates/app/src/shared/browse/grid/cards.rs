use gpui_kit::*;
use rigili_bilibili::{LiveRoom, PgcItem, SearchLiveRoom, SearchMedia, normalize_url};
use rigili_core::format_count;

#[derive(Clone)]
pub(crate) struct LiveCard {
    pub(crate) id: SharedString,
    pub(crate) room_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) online: String,
}

impl LiveCard {
    pub(crate) fn key(&self) -> String {
        format!("live:{}", self.room_id)
    }
}

impl From<LiveRoom> for LiveCard {
    fn from(room: LiveRoom) -> Self {
        let cover = if room.cover.is_empty() {
            room.system_cover
        } else {
            room.cover
        };
        let meta = if room.area_name.is_empty() {
            room.uname
        } else {
            format!("{} · {}", room.uname, room.area_name)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.roomid)),
            room_id: room.roomid,
            cover: normalize_url(&cover),
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

impl From<SearchLiveRoom> for LiveCard {
    fn from(room: SearchLiveRoom) -> Self {
        let meta = if room.area.is_empty() {
            room.name
        } else {
            format!("{} · {}", room.name, room.area)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.room_id)),
            room_id: room.room_id,
            cover: room.cover,
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PgcCard {
    pub(crate) id: SharedString,
    pub(crate) season_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) score: String,
    pub(crate) index_show: String,
    pub(crate) badge: Option<String>,
}

impl PgcCard {
    pub(crate) fn key(&self) -> String {
        format!("pgc:{}", self.season_id)
    }
}

impl From<PgcItem> for PgcCard {
    fn from(item: PgcItem) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: (!item.badge.is_empty()).then_some(item.badge),
        }
    }
}

impl From<SearchMedia> for PgcCard {
    fn from(item: SearchMedia) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: item.badge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LiveCard;
    use rigili_bilibili::LiveRoom;

    #[test]
    fn normalizes_live_room_cards() {
        let card = LiveCard::from(LiveRoom {
            roomid: 7,
            title: "直播".into(),
            uname: "UP主".into(),
            online: 12_345,
            cover: "//i0.hdslb.com/cover.jpg".into(),
            system_cover: String::new(),
            area_name: "游戏".into(),
            face: String::new(),
        });

        assert_eq!(card.id.as_ref(), "live:7");
        assert_eq!(card.cover, "https://i0.hdslb.com/cover.jpg");
        assert_eq!(card.meta, "UP主 · 游戏");
        assert_eq!(card.online, "1.2万");
    }
}
