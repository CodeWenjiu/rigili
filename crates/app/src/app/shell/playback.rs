use gpui_kit::*;

use rigili_core::OpenSpace;
use rigili_playback::{PlaybackRequest, PlayerWindow};

use super::MainView;

pub(super) struct Playback {
    handle: AnyWindowHandle,
    view: Entity<PlayerWindow>,
    _space_sub: Subscription,
}

impl Playback {
    pub(super) fn open(
        request: PlaybackRequest,
        main_window: AnyWindowHandle,
        main: WeakEntity<MainView>,
        cx: &mut App,
    ) -> Result<Self> {
        let (handle, view) = rigili_playback::open(request, cx)?;
        let space_sub = cx.subscribe(&view, move |_, event: &OpenSpace, cx| {
            let request = event.clone();
            let main = main.clone();
            let _ = main_window.update(cx, |_, window, cx| {
                if let Some(main) = main.upgrade() {
                    main.update(cx, |main, cx| main.open_space(request, window, cx));
                }
                window.activate_window();
            });
        });
        Ok(Self {
            handle,
            view,
            _space_sub: space_sub,
        })
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
