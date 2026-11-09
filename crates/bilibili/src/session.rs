use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const FILE_NAME: &str = "session.json";
const TMP_FILE_NAME: &str = "session.json.tmp";
const VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("读取会话失败：{0}")]
    Read(#[source] io::Error),
    #[error("写入会话失败：{0}")]
    Write(#[source] io::Error),
    #[error("会话格式异常：{0}")]
    Format(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    version: u32,
    cookies: BTreeMap<String, String>,
    saved_at: u64,
}

impl Session {
    pub fn new(cookies: BTreeMap<String, String>) -> Self {
        Self {
            version: VERSION,
            cookies,
            saved_at: unix_timestamp(),
        }
    }

    pub fn cookies(&self) -> &BTreeMap<String, String> {
        &self.cookies
    }

    pub fn load(dir: &Path) -> Result<Option<Self>, SessionError> {
        let path = dir.join(FILE_NAME);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(SessionError::Read(error)),
        };
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn save(&self, dir: &Path) -> Result<(), SessionError> {
        fs::create_dir_all(dir).map_err(SessionError::Write)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        let tmp = dir.join(TMP_FILE_NAME);
        {
            let mut file = fs::File::create(&tmp).map_err(SessionError::Write)?;
            restrict_permissions(&file)?;
            file.write_all(&bytes).map_err(SessionError::Write)?;
            file.sync_all().map_err(SessionError::Write)?;
        }
        fs::rename(&tmp, dir.join(FILE_NAME)).map_err(SessionError::Write)
    }

    pub fn clear(dir: &Path) -> Result<(), SessionError> {
        match fs::remove_file(dir.join(FILE_NAME)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SessionError::Write(error)),
        }
    }
}

pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GPUILI_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("gpuili"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/gpuili"))
}

#[cfg(unix)]
fn restrict_permissions(file: &fs::File) -> Result<(), SessionError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(SessionError::Write)
}

#[cfg(not(unix))]
fn restrict_permissions(_: &fs::File) -> Result<(), SessionError> {
    Ok(())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::PathBuf};

    use super::{Session, SessionError};

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gpuili-session-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn cookies() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("SESSDATA".to_string(), "session".to_string()),
            ("bili_jct".to_string(), "csrf".to_string()),
        ])
    }

    #[test]
    fn saves_and_loads_a_session() {
        let dir = temp_dir("roundtrip");
        let session = Session::new(cookies());
        session.save(&dir).expect("save");

        let loaded = Session::load(&dir).expect("load").expect("session");
        assert_eq!(loaded.cookies(), &cookies());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = temp_dir("missing");
        assert!(Session::load(&dir).expect("load").is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_broken_session_files() {
        let dir = temp_dir("broken");
        fs::write(dir.join("session.json"), b"not json").expect("write");
        assert!(matches!(Session::load(&dir), Err(SessionError::Format(_))));
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn writes_a_private_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("permissions");
        Session::new(cookies()).save(&dir).expect("save");

        let mode = fs::metadata(dir.join("session.json"))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_is_idempotent() {
        let dir = temp_dir("clear");
        Session::new(cookies()).save(&dir).expect("save");
        Session::clear(&dir).expect("clear");
        assert!(Session::load(&dir).expect("load").is_none());
        Session::clear(&dir).expect("clear again");
        fs::remove_dir_all(&dir).ok();
    }
}
