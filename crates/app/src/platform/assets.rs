use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(ExtraIcons, [Flame, MessageCircle]);

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
        assert!(AppAssets.load("icons/flame.svg").unwrap().is_some());
        assert!(AppAssets.load("icons/zoom-in.svg").unwrap().is_none());
    }
}
