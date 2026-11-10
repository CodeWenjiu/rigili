use std::{
    fs,
    io::{self, Write},
    path::Path,
};

const FILE_NAME: &str = "search_history.txt";
const TMP_FILE_NAME: &str = "search_history.txt.tmp";
const LIMIT: usize = 20;

#[derive(Clone, Debug, Default)]
pub(super) struct SearchHistory {
    keywords: Vec<String>,
}

impl SearchHistory {
    pub(super) fn load() -> Self {
        match rigili_bilibili::data_dir() {
            Some(dir) => Self::load_from(&dir),
            None => Self::default(),
        }
    }

    pub(super) fn keywords(&self) -> &[String] {
        &self.keywords
    }

    pub(super) fn is_empty(&self) -> bool {
        self.keywords.is_empty()
    }

    pub(super) fn record(&mut self, keyword: &str) {
        if self.insert(keyword) {
            self.save();
        }
    }

    pub(super) fn clear(&mut self) {
        self.keywords.clear();
        if let Some(dir) = rigili_bilibili::data_dir() {
            let _ = fs::remove_file(dir.join(FILE_NAME));
        }
    }

    fn insert(&mut self, keyword: &str) -> bool {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return false;
        }

        let already_first = self.keywords.first().is_some_and(|first| first == keyword);
        if already_first {
            return false;
        }

        self.keywords.retain(|existing| existing != keyword);
        self.keywords.insert(0, keyword.to_owned());
        self.keywords.truncate(LIMIT);
        true
    }

    fn load_from(dir: &Path) -> Self {
        let text = match fs::read_to_string(dir.join(FILE_NAME)) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };
        let mut history = Self::default();
        for keyword in text.lines() {
            if history.keywords.len() == LIMIT {
                break;
            }
            let keyword = keyword.trim();
            if !keyword.is_empty() && !history.keywords.iter().any(|existing| existing == keyword) {
                history.keywords.push(keyword.to_owned());
            }
        }
        history
    }

    fn save(&self) {
        let Some(dir) = rigili_bilibili::data_dir() else {
            return;
        };
        if let Err(error) = self.save_to(&dir) {
            eprintln!("failed to save search history: {error}");
        }
    }

    fn save_to(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join(TMP_FILE_NAME);
        {
            let mut file = fs::File::create(&tmp)?;
            restrict_permissions(&file)?;
            for keyword in &self.keywords {
                writeln!(file, "{keyword}")?;
            }
            file.sync_all()?;
        }
        fs::rename(tmp, dir.join(FILE_NAME))
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

    use super::{LIMIT, SearchHistory};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rigili-search-history-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn records_keywords_most_recent_first_and_dedupes() {
        let mut history = SearchHistory::default();
        history.insert("first");
        history.insert("second");
        history.insert("first");

        assert_eq!(history.keywords(), ["first", "second"]);
        assert!(!history.insert("   "));
        assert_eq!(history.keywords(), ["first", "second"]);
        assert!(!history.insert("first"));
    }

    #[test]
    fn round_trips_through_the_history_file() {
        let dir = temp_dir("roundtrip");
        let mut history = SearchHistory::default();
        for index in 0..LIMIT + 5 {
            history.insert(&format!("keyword {index}"));
        }
        history.save_to(&dir).expect("save");

        let loaded = SearchHistory::load_from(&dir);
        assert_eq!(loaded.keywords().len(), LIMIT);
        assert_eq!(loaded.keywords()[0], format!("keyword {}", LIMIT + 4));

        fs::remove_dir_all(&dir).ok();
    }
}
