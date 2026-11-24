#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::player) enum BarOverlay {
    Volume,
    Quality,
    Audio,
    Subtitle,
    Pages,
    Speed,
    DanmakuSettings,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::player) struct BarOverlays {
    open: u64,
}

impl BarOverlay {
    fn mask(self) -> u64 {
        1 << self as u8
    }

    pub(in crate::player) fn id(self) -> &'static str {
        match self {
            Self::Volume => "player-volume-popover",
            Self::Quality => "quality-popover",
            Self::Audio => "audio-popover",
            Self::Subtitle => "subtitle-popover",
            Self::Pages => "player-page-popover",
            Self::Speed => "speed-popover",
            Self::DanmakuSettings => "danmaku-settings-popover",
        }
    }
}

impl BarOverlays {
    pub(in crate::player) fn is_open(self, overlay: BarOverlay) -> bool {
        self.open & overlay.mask() != 0
    }

    pub(in crate::player) fn set(&mut self, overlay: BarOverlay, open: bool) -> bool {
        let mask = overlay.mask();
        let was_open = self.open & mask != 0;
        if open {
            self.open |= mask;
        } else {
            self.open &= !mask;
        }
        was_open != open
    }

    pub(in crate::player) fn any_open(self) -> bool {
        self.open != 0
    }

    pub(in crate::player) fn close_all(&mut self) {
        self.open = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{BarOverlay, BarOverlays};

    #[test]
    fn tracks_each_overlay_independently() {
        let mut overlays = BarOverlays::default();
        assert!(!overlays.any_open());

        assert!(overlays.set(BarOverlay::Quality, true));
        assert!(!overlays.set(BarOverlay::Quality, true));
        assert!(overlays.is_open(BarOverlay::Quality));
        assert!(!overlays.is_open(BarOverlay::Audio));

        overlays.set(BarOverlay::Audio, true);
        assert!(overlays.any_open());

        overlays.set(BarOverlay::Quality, false);
        assert!(overlays.any_open());
        assert!(!overlays.is_open(BarOverlay::Quality));

        overlays.close_all();
        assert!(!overlays.any_open());
    }
}
