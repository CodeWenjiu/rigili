use gpui_kit::*;
use rigili_core::{OpenLive, OpenSpace, OpenVideo};

use super::season::SeasonOpener;

#[derive(Clone)]
pub(crate) enum OpenTarget {
    Video(OpenVideo),
    Live { room_id: u64, title: String },
    Season(u64),
    Article(u64),
    User { mid: u64, name: String },
}

pub(crate) trait OpenTargets:
    EventEmitter<OpenVideo>
    + EventEmitter<OpenLive>
    + EventEmitter<OpenSpace>
    + SeasonOpener
    + Sized
    + 'static
{
    fn open_target(&mut self, target: OpenTarget, new_tab: bool, cx: &mut Context<Self>) {
        match target {
            OpenTarget::Video(mut video) => {
                video.new_tab = new_tab;
                cx.emit(video);
            }
            OpenTarget::Live { room_id, title } => cx.emit(OpenLive {
                room_id,
                title,
                new_tab,
            }),
            OpenTarget::Season(season_id) => self.open_season(season_id, cx),
            OpenTarget::Article(id) => {
                cx.open_url(&format!("https://www.bilibili.com/read/cv{id}"));
            }
            OpenTarget::User { mid, name } => cx.emit(OpenSpace { mid, name }),
        }
    }
}
