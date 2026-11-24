use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use gpui_kit::*;
use rigili_bilibili::data_dir;
use serde::{Deserialize, Serialize};

use crate::pages::account::mine::MineTab;

const FILE_NAME: &str = "preferences.json";
const TMP_FILE_NAME: &str = "preferences.json.tmp";
const LEGACY_HISTORY_FILE: &str = "search_history.txt";
const HISTORY_LIMIT: usize = 20;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    mine_tab: MineTab,
    search_history: Vec<String>,
    #[serde(skip)]
    dirty: bool,
}

impl Global for Preferences {}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(load());
    cx.on_app_quit(|cx| {
        if let Some(preferences) = cx.try_global::<Preferences>()
            && preferences.dirty
            && let Err(error) = preferences.save()
        {
            eprintln!("failed to save preferences: {error}");
        }
        async {}
    })
    .detach();
}

pub(crate) fn mine_tab(cx: &App) -> MineTab {
    cx.try_global::<Preferences>()
        .map_or_else(MineTab::default, |preferences| preferences.mine_tab)
}

pub(crate) fn set_mine_tab(cx: &mut App, tab: MineTab) {
    update(cx, |preferences| {
        if preferences.mine_tab != tab {
            preferences.mine_tab = tab;
            preferences.dirty = true;
        }
    });
}

pub(crate) fn search_history(cx: &App) -> &[String] {
    cx.try_global::<Preferences>()
        .map(|preferences| preferences.search_history.as_slice())
        .unwrap_or_default()
}

pub(crate) fn record_search(cx: &mut App, keyword: &str) {
    update(cx, |preferences| {
        if preferences.insert_search(keyword) {
            preferences.dirty = true;
        }
    });
}

pub(crate) fn clear_search_history(cx: &mut App) {
    update(cx, |preferences| {
        if !preferences.search_history.is_empty() {
            preferences.search_history.clear();
            preferences.dirty = true;
        }
    });
}

pub(crate) fn remove_search(cx: &mut App, keyword: &str) {
    update(cx, |preferences| {
        if preferences.remove_search(keyword) {
            preferences.dirty = true;
        }
    });
}

fn update(cx: &mut App, apply: impl FnOnce(&mut Preferences)) {
    if cx.has_global::<Preferences>() {
        cx.update_global::<Preferences, _>(|preferences, _| apply(preferences));
    }
}

fn load() -> Preferences {
    data_dir().map_or_else(Preferences::default, |dir| Preferences::load_from(&dir))
}

impl Preferences {
    fn insert_search(&mut self, keyword: &str) -> bool {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return false;
        }
        if self
            .search_history
            .first()
            .is_some_and(|first| first == keyword)
        {
            return false;
        }

        self.search_history.retain(|existing| existing != keyword);
        self.search_history.insert(0, keyword.to_owned());
        self.search_history.truncate(HISTORY_LIMIT);
        true
    }

    fn remove_search(&mut self, keyword: &str) -> bool {
        let Some(index) = self
            .search_history
            .iter()
            .position(|existing| existing == keyword)
        else {
            return false;
        };
        self.search_history.remove(index);
        true
    }

    fn normalize(&mut self) {
        let keywords = std::mem::take(&mut self.search_history);
        let mut normalized = Vec::with_capacity(keywords.len().min(HISTORY_LIMIT));
        for keyword in keywords {
            let keyword = keyword.trim();
            if keyword.is_empty()
                || normalized
                    .iter()
                    .any(|existing: &String| existing == keyword)
            {
                continue;
            }
            normalized.push(keyword.to_owned());
            if normalized.len() == HISTORY_LIMIT {
                break;
            }
        }
        self.search_history = normalized;
    }

    fn load_from(dir: &Path) -> Self {
        if let Ok(text) = fs::read_to_string(dir.join(FILE_NAME))
            && let Ok(mut preferences) = serde_json::from_str::<Self>(&text)
        {
            preferences.normalize();
            return preferences;
        }

        let mut preferences = Self::default();
        if let Ok(text) = fs::read_to_string(dir.join(LEGACY_HISTORY_FILE)) {
            for keyword in text.lines() {
                preferences.insert_search(keyword);
            }
            preferences.dirty = !preferences.search_history.is_empty();
        }
        preferences
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
            file.sync_all()?;
        }
        fs::rename(tmp, dir.join(FILE_NAME))?;
        let _ = fs::remove_file(dir.join(LEGACY_HISTORY_FILE));
        Ok(())
    }
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

    use super::{HISTORY_LIMIT, LEGACY_HISTORY_FILE, MineTab, Preferences};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rigili-preferences-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn records_keywords_most_recent_first_and_dedupes() {
        let mut preferences = Preferences::default();
        preferences.insert_search("first");
        preferences.insert_search("second");
        preferences.insert_search("first");

        assert_eq!(preferences.search_history, ["first", "second"]);
        assert!(!preferences.insert_search("   "));
        assert_eq!(preferences.search_history, ["first", "second"]);
        assert!(!preferences.insert_search("first"));
    }

    #[test]
    fn removes_a_single_keyword() {
        let mut preferences = Preferences::default();
        for keyword in ["first", "second", "third"] {
            preferences.insert_search(keyword);
        }

        assert!(preferences.remove_search("second"));
        assert_eq!(preferences.search_history, ["third", "first"]);
        assert!(!preferences.remove_search("missing"));
        assert_eq!(preferences.search_history, ["third", "first"]);
    }

    #[test]
    fn round_trips_tabs_and_history() {
        let dir = temp_dir("roundtrip");
        let mut preferences = Preferences {
            mine_tab: MineTab::History,
            ..Default::default()
        };
        for index in 0..HISTORY_LIMIT + 5 {
            preferences.insert_search(&format!("keyword {index}"));
        }
        preferences.save_to(&dir).expect("save");

        let loaded = Preferences::load_from(&dir);
        assert_eq!(loaded.mine_tab, MineTab::History);
        assert_eq!(loaded.search_history.len(), HISTORY_LIMIT);
        assert_eq!(
            loaded.search_history[0],
            format!("keyword {}", HISTORY_LIMIT + 4)
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn imports_the_legacy_history_file_once() {
        let dir = temp_dir("legacy");
        fs::write(dir.join(LEGACY_HISTORY_FILE), "first\nsecond\nfirst\n").expect("write");

        let preferences = Preferences::load_from(&dir);
        assert_eq!(preferences.search_history, ["first", "second"]);
        assert!(preferences.dirty);

        preferences.save_to(&dir).expect("save");
        assert!(!dir.join(LEGACY_HISTORY_FILE).exists());
        assert_eq!(
            Preferences::load_from(&dir).search_history,
            ["first", "second"]
        );

        fs::remove_dir_all(&dir).ok();
    }
}
