use gpui_kit::*;

use rigili_playback::{PlaybackRequest, PlayerWindow};

pub(super) struct Playback {
    handle: AnyWindowHandle,
    view: Entity<PlayerWindow>,
}

impl Playback {
    pub(super) fn open(request: PlaybackRequest, cx: &mut App) -> Result<Self> {
        let (handle, view) = rigili_playback::open(request, cx)?;
        Ok(Self { handle, view })
    }

    pub(super) fn load(&self, request: PlaybackRequest, cx: &mut App) -> Result<()> {
        let view = self.view.clone();
        self.handle.update(cx, |_, window, cx| {
            view.update(cx, |player_window, cx| player_window.play(request, cx));
            window.activate_window();
        })
    }

    pub(super) fn is_for(&self, window_id: WindowId) -> bool {
        self.handle.window_id() == window_id
    }
}
