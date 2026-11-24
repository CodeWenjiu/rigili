use serde::Deserialize;

use super::null_or_default;
use super::video::Dash;

/// Identity of a PGC episode: the season owns the rights, the episode is the
/// playable unit. The episode's bvid/cid are only its underlying archive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EpisodeId {
    pub ep_id: u64,
    pub season_id: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcIndex {
    #[serde(default)]
    pub has_next: u32,
    #[serde(default)]
    pub list: Vec<PgcItem>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcItem {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub index_show: String,
    #[serde(default, rename = "subTitle")]
    pub subtitle: String,
    #[serde(default)]
    pub score: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub season_type: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcSeason {
    #[serde(default, deserialize_with = "null_or_default")]
    pub season_id: u64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub title: String,
    #[serde(default, deserialize_with = "null_or_default")]
    pub episodes: Vec<PgcEpisode>,
    #[serde(default, deserialize_with = "null_or_default")]
    pub user_status: PgcUserStatus,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcUserStatus {
    #[serde(default)]
    pub progress: Option<PgcProgress>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcProgress {
    #[serde(default, deserialize_with = "null_or_default")]
    pub last_ep_id: u64,
    #[serde(default, rename = "last_time", deserialize_with = "null_or_default")]
    pub last_time_seconds: f64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcEpisode {
    #[serde(default, deserialize_with = "null_or_default")]
    pub aid: u64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub bvid: String,
    #[serde(default, deserialize_with = "null_or_default")]
    pub cid: u64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub ep_id: u64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub long_title: String,
    #[serde(default, deserialize_with = "null_or_default")]
    pub cover: String,
    #[serde(default, deserialize_with = "null_or_default")]
    pub duration: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PgcPlayUrl {
    #[serde(default)]
    pub dash: Option<Dash>,
    #[serde(default, deserialize_with = "null_or_default")]
    pub accept_quality: Vec<u32>,
    #[serde(default, deserialize_with = "null_or_default")]
    pub accept_description: Vec<String>,
    #[serde(default, deserialize_with = "null_or_default")]
    pub timelength: u64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub is_preview: u32,
    #[serde(default, deserialize_with = "null_or_default")]
    pub is_drm: bool,
}

#[cfg(test)]
mod tests {
    use super::PgcSeason;

    #[test]
    fn parses_episode_identity_and_watch_progress() {
        let season: PgcSeason = serde_json::from_str(
            r#"{
                "season_id": 47836,
                "title": "鬼灭之刃 柱训练篇",
                "episodes": [{
                    "aid": 170001,
                    "bvid": "BV15i421v7FQ",
                    "cid": 1602741036,
                    "ep_id": 826497,
                    "long_title": "为了消灭鬼舞辻无惨",
                    "duration": 2938060
                }],
                "user_status": {
                    "progress": {"last_ep_id": 826497, "last_time": 123}
                }
            }"#,
        )
        .expect("season");

        assert_eq!(season.episodes[0].aid, 170001);
        let progress = season.user_status.progress.expect("progress");
        assert_eq!(progress.last_ep_id, 826497);
        assert_eq!(progress.last_time_seconds, 123.0);
    }

    #[test]
    fn tolerates_missing_or_null_progress() {
        let missing: PgcSeason = serde_json::from_str(r#"{"season_id":1}"#).unwrap();
        assert!(missing.user_status.progress.is_none());

        let null: PgcSeason =
            serde_json::from_str(r#"{"season_id":1,"user_status":null}"#).unwrap();
        assert!(null.user_status.progress.is_none());
    }
}
