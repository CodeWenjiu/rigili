use std::sync::Arc;

use gpui_kit::*;
use rigili_bilibili::Client;
use rigili_core::OpenVideo;
use rigili_core::runtime::spawn_api;
use rigili_core::{api_task_error_message, resolve_season};

pub(crate) trait SeasonOpener: EventEmitter<OpenVideo> + Sized + 'static {
    fn opening(&mut self) -> &mut Option<u64>;
    fn notice(&mut self) -> &mut Option<String>;
    fn season_client(&self) -> Option<Arc<Client>>;

    fn open_season(&mut self, season_id: u64, cx: &mut Context<Self>) {
        if self.opening().is_some() {
            return;
        }
        let Some(client) = self.season_client() else {
            *self.notice() = Some("客户端尚未初始化".into());
            cx.notify();
            return;
        };

        *self.opening() = Some(season_id);
        *self.notice() = None;
        cx.notify();

        spawn_api(
            cx,
            async move { resolve_season(&client, season_id).await },
            move |this, result, cx| {
                *this.opening() = None;
                match result {
                    Ok(Some(video)) => cx.emit(video),
                    Ok(None) => *this.notice() = Some("该作品暂无可播放的剧集".into()),
                    Err(error) => *this.notice() = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }
}
