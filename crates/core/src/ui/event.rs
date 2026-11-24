use gpui_kit::*;
use rigili_bilibili::EpisodeId;

#[derive(Clone)]
pub struct OpenVideo {
    pub bvid: String,
    pub cid: Option<u64>,
    pub title: String,
    pub new_tab: bool,
    pub episode: Option<EpisodeId>,
}

/// Whether a card click asked for a new player tab instead of replacing the current one.
pub fn opens_new_tab(event: &ClickEvent) -> bool {
    match event {
        ClickEvent::Mouse(event) => event.down.modifiers.control,
        _ => false,
    }
}

#[derive(Clone)]
pub struct OpenLive {
    pub room_id: u64,
    pub title: String,
    pub new_tab: bool,
}

#[derive(Clone)]
pub struct OpenSpace {
    pub mid: u64,
    pub name: String,
}

#[derive(Clone)]
pub struct FocusMain;
