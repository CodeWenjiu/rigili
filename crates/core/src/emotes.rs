use std::{
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use gpui_kit::*;
use rigili_bilibili::{EmotePackage, EmotePanel, data_dir};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "emotes.json";
const TMP_FILE_NAME: &str = "emotes.json.tmp";
const MAX_AGE: u64 = 24 * 60 * 60;
const CACHE_VERSION: u32 = 2;

/// The signed-in user's reply emote packages.
///
/// The list does not depend on the video, so it is shared by every comment
/// panel and persisted, which keeps a restart from fetching it again. The
/// images are not kept here: they decode into the bounded image tier and evict
/// from there like any other picture.
#[derive(Default)]
struct Emotes {
    packages: Rc<Vec<EmotePackage>>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    cache_dir: Option<PathBuf>,
}

impl Global for Emotes {}

#[derive(Deserialize, Serialize)]
struct Cache {
    #[serde(default)]
    version: u32,
    fetched_at: u64,
    packages: Vec<EmotePackage>,
}

/// Restores the packages cached by a previous run.
pub fn init(cx: &mut App) {
    let Some(dir) = data_dir() else {
        return;
    };
    let (packages, loaded) = restore(&dir);
    cx.set_global(Emotes {
        packages: Rc::new(packages),
        loaded,
        cache_dir: Some(dir),
        ..Default::default()
    });
}

pub fn packages(cx: &App) -> Rc<Vec<EmotePackage>> {
    cx.try_global::<Emotes>()
        .map(|emotes| emotes.packages.clone())
        .unwrap_or_default()
}

pub fn loading(cx: &App) -> bool {
    cx.try_global::<Emotes>()
        .is_some_and(|emotes| emotes.loading)
}

pub fn error(cx: &App) -> Option<String> {
    cx.try_global::<Emotes>()
        .and_then(|emotes| emotes.error.clone())
}

/// Claims the single fetch, so opening the picker again in another panel does
/// not hit the network. Returns false once it ran or is running.
pub fn start(cx: &mut App) -> bool {
    let emotes = state(cx);
    if emotes.loading || emotes.loaded {
        return false;
    }
    emotes.loading = true;
    emotes.error = None;
    true
}

pub fn succeed(cx: &mut App, panel: EmotePanel) {
    let packages = panel
        .packages
        .unwrap_or_default()
        .into_iter()
        .filter(|package| {
            package
                .emote
                .as_ref()
                .is_some_and(|emotes| emotes.iter().any(|emote| !emote.text.is_empty()))
        })
        .collect();
    let emotes = state(cx);
    emotes.packages = Rc::new(packages);
    emotes.loading = false;
    emotes.loaded = true;
    emotes.error = None;
    if let Some(dir) = emotes.cache_dir.as_deref()
        && let Err(error) = write(dir, &emotes.packages)
    {
        eprintln!("failed to cache emotes: {error}");
    }
}

pub fn fail(cx: &mut App, error: String) {
    let emotes = state(cx);
    emotes.loading = false;
    emotes.error = Some(error);
}

fn state(cx: &mut App) -> &mut Emotes {
    if !cx.has_global::<Emotes>() {
        cx.set_global(Emotes::default());
    }
    cx.global_mut::<Emotes>()
}

fn fresh(fetched_at: u64) -> bool {
    now().saturating_sub(fetched_at) < MAX_AGE
}

fn restore(dir: &Path) -> (Vec<EmotePackage>, bool) {
    let cache = read(&dir.join(FILE_NAME)).filter(|cache| cache.version == CACHE_VERSION);
    let loaded = cache.as_ref().is_some_and(|cache| fresh(cache.fetched_at));
    let packages = cache.map(|cache| cache.packages).unwrap_or_default();
    (packages, loaded)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn read(path: &Path) -> Option<Cache> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write(dir: &Path, packages: &[EmotePackage]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec(&Cache {
        version: CACHE_VERSION,
        fetched_at: now(),
        packages: packages.to_vec(),
    })
    .map_err(io::Error::other)?;
    let tmp = dir.join(TMP_FILE_NAME);
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, dir.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        CACHE_VERSION, Cache, FILE_NAME, fail, fresh, loading, packages, read, restore, start,
        succeed, write,
    };
    use gpui_kit::TestAppContext;
    use rigili_bilibili::{EmoteItem, EmotePackage, EmotePanel};

    fn package(text: &str, url: &str) -> EmotePackage {
        EmotePackage {
            text: text.to_owned(),
            url: String::new(),
            emote: Some(vec![EmoteItem {
                text: text.to_owned(),
                url: url.to_owned(),
            }]),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rigili-emotes-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[gpui_kit::test]
    fn fetches_once_and_drops_empty_packages(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(start(cx));
            assert!(loading(cx));
            assert!(!start(cx), "a running fetch must not be claimed twice");

            succeed(
                cx,
                EmotePanel {
                    packages: Some(vec![
                        package("[doge]", "//i0.hdslb.com/bfs/emote/doge.png"),
                        package("[gone]", ""),
                        package("", "//i0.hdslb.com/bfs/emote/blank.png"),
                        EmotePackage {
                            text: "empty".into(),
                            url: String::new(),
                            emote: None,
                        },
                    ]),
                },
            );

            assert!(!loading(cx));
            assert_eq!(packages(cx).len(), 2);
            assert_eq!(packages(cx)[0].text, "[doge]");
            assert_eq!(packages(cx)[1].text, "[gone]");
            assert!(!start(cx), "a loaded list must not be fetched again");
        });
    }

    #[gpui_kit::test]
    fn a_failed_fetch_can_be_retried(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(start(cx));
            fail(cx, "boom".into());
            assert!(!loading(cx));
            assert_eq!(super::error(cx).as_deref(), Some("boom"));
            assert!(start(cx), "a failed fetch must be retryable");
        });
    }

    #[test]
    fn round_trips_packages_and_expires_the_cache() {
        let dir = temp_dir("roundtrip");
        let packages = vec![package("[doge]", "//i0.hdslb.com/bfs/emote/doge.png")];

        write(&dir, &packages).expect("write cache");
        let cache = read(&dir.join(FILE_NAME)).expect("read cache");
        assert_eq!(cache.packages, packages);
        assert!(fresh(cache.fetched_at));
        assert!(!fresh(0));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restores_a_fresh_cache_and_refetches_an_old_one() {
        let dir = temp_dir("restore");
        assert_eq!(restore(&dir), (Vec::new(), false));

        let packages = vec![package("[doge]", "//i0.hdslb.com/bfs/emote/doge.png")];
        write(&dir, &packages).expect("write cache");
        assert_eq!(restore(&dir), (packages.clone(), true));

        let stale = Cache {
            version: CACHE_VERSION,
            fetched_at: 0,
            packages,
        };
        fs::write(
            dir.join(FILE_NAME),
            serde_json::to_vec(&stale).expect("encode"),
        )
        .expect("write stale cache");
        assert_eq!(restore(&dir), (stale.packages, false));

        let old = Cache {
            version: CACHE_VERSION - 1,
            fetched_at: u64::MAX,
            packages: vec![package("[doge]", "//i0.hdslb.com/bfs/emote/doge.png")],
        };
        fs::write(
            dir.join(FILE_NAME),
            serde_json::to_vec(&old).expect("encode"),
        )
        .expect("write old cache");
        assert_eq!(restore(&dir), (Vec::new(), false));

        fs::remove_dir_all(&dir).ok();
    }
}
