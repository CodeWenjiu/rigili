use crate::{ApiError, Client, EpisodeId, PgcIndex, PgcPlayUrl, PgcSeason};

const INDEX_URL: &str = "https://api.bilibili.com/pgc/season/index/result";
const SEASON_URL: &str = "https://api.bilibili.com/pgc/view/web/season";
// The v2 path answers with the app-style video_info/view_info payload; the web
// player still reads dash from this legacy path.
const PLAYURL_URL: &str = "https://api.bilibili.com/pgc/player/web/playurl";
const DASH_FEATURES: &str = "4048";

impl Client {
    pub async fn pgc_index(
        &self,
        season_type: u32,
        page: u32,
        page_size: u32,
    ) -> Result<PgcIndex, ApiError> {
        self.get_json(
            INDEX_URL,
            &[
                ("season_type", season_type.to_string()),
                ("st", "1".into()),
                ("sort", "0".into()),
                ("page", page.max(1).to_string()),
                ("pagesize", page_size.clamp(1, 50).to_string()),
                ("type", "1".into()),
            ],
        )
        .await
    }

    pub async fn pgc_season(&self, season_id: u64) -> Result<PgcSeason, ApiError> {
        self.get_json(SEASON_URL, &[("season_id", season_id.to_string())])
            .await
    }

    pub async fn pgc_season_by_ep(&self, ep_id: u64) -> Result<PgcSeason, ApiError> {
        self.get_json(SEASON_URL, &[("ep_id", ep_id.to_string())])
            .await
    }

    pub async fn pgc_playurl(&self, episode: EpisodeId, cid: u64) -> Result<PgcPlayUrl, ApiError> {
        self.get_wbi_json(PLAYURL_URL, &pgc_playurl_params(episode, cid))
            .await
    }
}

fn pgc_playurl_params(episode: EpisodeId, cid: u64) -> Vec<(&'static str, String)> {
    vec![
        ("ep_id", episode.ep_id.to_string()),
        ("season_id", episode.season_id.to_string()),
        ("cid", cid.to_string()),
        ("qn", "127".to_owned()),
        ("fnval", DASH_FEATURES.to_owned()),
        ("fnver", "0".to_owned()),
        ("fourk", "1".to_owned()),
        ("try_look", "1".to_owned()),
    ]
}

#[cfg(test)]
mod tests {
    use super::pgc_playurl_params;
    use crate::{EpisodeId, PgcIndex, PgcPlayUrl, PgcSeason};

    const INDEX_FIXTURE: &str = include_str!("../../fixtures/pgc_index.json");
    const SEASON_FIXTURE: &str = include_str!("../../fixtures/pgc_season.json");
    const PLAYURL_FIXTURE: &str = include_str!("../../fixtures/pgc_playurl.json");

    #[derive(serde::Deserialize)]
    struct IndexEnvelope {
        data: PgcIndex,
    }

    #[derive(serde::Deserialize)]
    struct SeasonEnvelope {
        result: PgcSeason,
    }

    #[derive(serde::Deserialize)]
    struct PlayUrlEnvelope {
        result: PgcPlayUrl,
    }

    #[test]
    fn parses_pgc_fixtures() {
        let index: IndexEnvelope = serde_json::from_str(INDEX_FIXTURE).expect("pgc index");
        let index = index.data;

        assert_eq!(index.has_next, 1);
        assert_eq!(index.list.len(), 1);
        assert_eq!(index.list[0].season_id, 47_836);
        assert_eq!(index.list[0].subtitle, "羁绊连结，传承信念");
        assert_eq!(index.list[0].score, "9.7");

        let season: SeasonEnvelope = serde_json::from_str(SEASON_FIXTURE).expect("pgc season");
        let season = season.result;

        assert_eq!(season.season_id, 47_836);
        assert_eq!(season.episodes.len(), 1);
        assert_eq!(season.episodes[0].bvid, "BV15i421v7FQ");
        assert_eq!(season.episodes[0].cid, 1_602_741_036);
    }

    #[test]
    fn parses_a_pgc_playurl_and_its_entitlement_flags() {
        let playurl: PlayUrlEnvelope = serde_json::from_str(PLAYURL_FIXTURE).expect("pgc playurl");
        let playurl = playurl.result;

        assert_eq!(playurl.is_preview, 0);
        assert!(!playurl.is_drm);
        assert_eq!(playurl.timelength, 2_938_060);
        assert_eq!(playurl.accept_quality, [80, 64]);

        let dash = playurl.dash.expect("dash");
        assert!(dash.audio.is_empty());
        assert_eq!(dash.video.len(), 1);
        assert_eq!(dash.video[0].id, 80);
        assert_eq!(dash.video[0].codecid, 7);
        assert_eq!(dash.video[0].backup_urls.len(), 1);
    }

    #[test]
    fn signs_the_episode_identity_and_dash_features() {
        let episode = EpisodeId {
            ep_id: 826_497,
            season_id: 47_836,
        };
        let params = pgc_playurl_params(episode, 1_602_741_036);
        let value = |key: &str| {
            params
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or_else(|| panic!("missing {key}"))
        };

        assert_eq!(value("ep_id"), "826497");
        assert_eq!(value("season_id"), "47836");
        assert_eq!(value("cid"), "1602741036");
        assert_eq!(value("fnval"), "4048");
        assert_eq!(value("fourk"), "1");
        assert_eq!(value("qn"), "127");
        assert_eq!(value("try_look"), "1");
    }
}
