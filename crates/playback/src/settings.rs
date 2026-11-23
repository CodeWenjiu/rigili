use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use gpui_kit::*;
use rigili_bilibili::data_dir;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "playback.json";
const TMP_FILE_NAME: &str = "playback.json.tmp";
const DEFAULT_SCROLL_SECONDS: f64 = 8.0;
const DEFAULT_AREA_RATIO: f32 = 0.72;
const DEFAULT_SUBTITLE_X: f32 = 0.5;
const DEFAULT_SUBTITLE_Y: f32 = 0.8;
pub(crate) const DEFAULT_VOLUME: f64 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DanmakuSettings {
    pub(crate) enabled: bool,
    pub(crate) opacity: f32,
    pub(crate) font_scale: f32,
    pub(crate) scroll_seconds: f64,
    pub(crate) area_ratio: f32,
}

impl Default for DanmakuSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            opacity: 1.0,
            font_scale: 1.0,
            scroll_seconds: DEFAULT_SCROLL_SECONDS,
            area_ratio: DEFAULT_AREA_RATIO,
        }
    }
}

impl DanmakuSettings {
    fn sanitize(&mut self) {
        self.opacity = finite_or(self.opacity, 1.0).clamp(0.05, 1.0);
        self.font_scale = finite_or(self.font_scale, 1.0).clamp(0.5, 2.0);
        if !self.scroll_seconds.is_finite() {
            self.scroll_seconds = DEFAULT_SCROLL_SECONDS;
        }
        self.scroll_seconds = self.scroll_seconds.clamp(2.0, 30.0);
        self.area_ratio = finite_or(self.area_ratio, DEFAULT_AREA_RATIO).clamp(0.1, 1.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SubtitleSettings {
    pub(crate) enabled: bool,
    /// Horizontal center of the subtitle box, as a fraction of the video width.
    pub(crate) x: f32,
    /// Top edge of the subtitle box, as a fraction of the video height.
    pub(crate) y: f32,
}

impl Default for SubtitleSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            x: DEFAULT_SUBTITLE_X,
            y: DEFAULT_SUBTITLE_Y,
        }
    }
}

impl SubtitleSettings {
    fn sanitize(&mut self) {
        self.x = finite_or(self.x, DEFAULT_SUBTITLE_X).clamp(0.0, 1.0);
        self.y = finite_or(self.y, DEFAULT_SUBTITLE_Y).clamp(0.0, 1.0);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PlaybackSettings {
    pub(crate) danmaku: DanmakuSettings,
    pub(crate) subtitle: SubtitleSettings,
    pub(crate) speed: f64,
    pub(crate) preferred_quality: Option<u32>,
    #[serde(skip)]
    dirty: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            danmaku: DanmakuSettings::default(),
            subtitle: SubtitleSettings::default(),
            speed: 1.0,
            preferred_quality: None,
            dirty: false,
        }
    }
}

impl Global for PlaybackSettings {}

pub fn init(cx: &mut App) {
    cx.set_global(load());
    cx.on_app_quit(|cx| {
        if let Some(settings) = cx.try_global::<PlaybackSettings>()
            && settings.dirty
            && let Err(error) = settings.save()
        {
            eprintln!("failed to save playback settings: {error}");
        }
        async {}
    })
    .detach();
}

pub(crate) fn current(cx: &App) -> PlaybackSettings {
    cx.try_global::<PlaybackSettings>()
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn update(cx: &mut App, update: impl FnOnce(&mut PlaybackSettings)) {
    if !cx.has_global::<PlaybackSettings>() {
        return;
    }
    cx.update_global::<PlaybackSettings, _>(|settings, _| {
        update(settings);
        settings.normalize();
        settings.dirty = true;
    });
}

fn load() -> PlaybackSettings {
    match data_dir() {
        Some(dir) => PlaybackSettings::load_from(&dir),
        None => PlaybackSettings::default(),
    }
}

impl PlaybackSettings {
    fn normalize(&mut self) {
        self.danmaku.sanitize();
        self.subtitle.sanitize();
        if self.speed.is_finite() {
            self.speed = self.speed.clamp(0.25, 4.0);
        } else {
            self.speed = 1.0;
        }
    }

    fn load_from(dir: &Path) -> Self {
        let Ok(text) = fs::read_to_string(dir.join(FILE_NAME)) else {
            return Self::default();
        };
        let Ok(mut settings) = serde_json::from_str::<Self>(&text) else {
            return Self::default();
        };
        settings.normalize();
        settings
    }

    fn save(&self) -> io::Result<()> {
        let Some(dir) = data_dir() else {
            return Ok(());
        };
        self.save_to(&dir)
    }

    fn save_to(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let tmp = dir.join(TMP_FILE_NAME);
        {
            let mut file = fs::File::create(&tmp)?;
            restrict_permissions(&file)?;
            file.write_all(&bytes)?;
        }
        fs::rename(tmp, dir.join(FILE_NAME))
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(unix)]
fn restrict_permissions(file: &fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_: &fs::File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{DanmakuSettings, PlaybackSettings, SubtitleSettings};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rigili-playback-settings-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn round_trips_through_the_settings_file() {
        let dir = temp_dir("roundtrip");
        let settings = PlaybackSettings {
            danmaku: DanmakuSettings {
                enabled: false,
                opacity: 0.5,
                font_scale: 1.25,
                scroll_seconds: 5.0,
                area_ratio: 1.0,
            },
            subtitle: SubtitleSettings {
                enabled: true,
                x: 0.3,
                y: 0.7,
            },
            speed: 2.0,
            preferred_quality: Some(80),
            dirty: false,
        };
        settings.save_to(&dir).expect("save");

        assert_eq!(PlaybackSettings::load_from(&dir), settings);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clamps_invalid_values() {
        let dir = temp_dir("invalid");
        fs::write(
            dir.join(super::FILE_NAME),
            r#"{"danmaku":{"opacity":4.0,"font_scale":0.1},"speed":12.0,"volume":140.0}"#,
        )
        .expect("write");

        let loaded = PlaybackSettings::load_from(&dir);
        assert_eq!(loaded.danmaku.opacity, 1.0);
        assert_eq!(loaded.danmaku.font_scale, 0.5);
        assert_eq!(loaded.speed, 4.0);
        assert_eq!(loaded.subtitle.x, 0.5);
        assert_eq!(loaded.subtitle.y, 0.8);

        let mut subtitles = SubtitleSettings {
            x: f32::NAN,
            y: 2.0,
            ..SubtitleSettings::default()
        };
        subtitles.sanitize();
        assert_eq!(subtitles.x, 0.5);
        assert_eq!(subtitles.y, 1.0);

        let mut danmaku = DanmakuSettings {
            scroll_seconds: f64::NAN,
            ..DanmakuSettings::default()
        };
        danmaku.sanitize();
        assert_eq!(danmaku.scroll_seconds, 8.0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignores_the_volume_and_mute_of_older_files() {
        let dir = temp_dir("legacy-volume");
        fs::write(dir.join(super::FILE_NAME), r#"{"volume":42.0,"mute":true}"#).expect("write");

        let loaded = PlaybackSettings::load_from(&dir);
        assert_eq!(loaded, PlaybackSettings::default());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignores_corrupt_files() {
        let dir = temp_dir("corrupt");
        fs::write(dir.join(super::FILE_NAME), "{not json").expect("write");
        assert_eq!(
            PlaybackSettings::load_from(&dir),
            PlaybackSettings::default()
        );
        fs::remove_dir_all(&dir).ok();
    }
}
