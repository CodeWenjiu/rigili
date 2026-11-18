use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(
    ExtraIcons,
    [
        Clock,
        Coins,
        Download,
        Flame,
        House,
        ListFilter,
        ListVideo,
        Lock,
        Mail,
        MessageCircle,
        MessageSquareOff,
        MessageSquareText,
        MonitorPlay,
        Radio,
        Share,
        Share2,
        Sticker,
        Tv,
        Volume,
        Volume1,
        Volume2,
        VolumeOff
    ]
);

pub(crate) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Ok(Assets.load(path).ok().flatten()),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = ExtraIcons.list(path)?;
        names.extend(Assets.list(path)?);
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::AppAssets;
    use gpui_kit::AssetSource;

    #[test]
    fn serves_default_and_extra_icons() {
        assert!(AppAssets.load("icons/play.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/clock.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/mail.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/flame.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/house.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/share.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/sticker.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/list-video.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/zoom-in.svg").unwrap().is_none());
    }

    #[test]
    fn serves_every_icon_the_app_draws() {
        use gpui_kit::assets::IconName;

        for icon in [
            IconName::Ban,
            IconName::Calendar,
            IconName::Check,
            IconName::ChevronDown,
            IconName::ChevronUp,
            IconName::CircleAlert,
            IconName::Clock,
            IconName::Coins,
            IconName::Download,
            IconName::ListFilter,
            IconName::Lock,
            IconName::MessageSquareOff,
            IconName::MessageSquareText,
            IconName::MonitorPlay,
            IconName::Radio,
            IconName::RefreshCw,
            IconName::Share2,
            IconName::Sticker,
            IconName::Tv,
            IconName::Volume,
            IconName::Volume1,
            IconName::Volume2,
            IconName::VolumeOff,
        ] {
            assert!(
                AppAssets.load(&icon.path()).unwrap().is_some(),
                "missing {}",
                icon.path()
            );
        }
    }
}
