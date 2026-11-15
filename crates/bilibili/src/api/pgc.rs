use crate::{ApiError, Client, PgcIndex, PgcSeason};

const INDEX_URL: &str = "https://api.bilibili.com/pgc/season/index/result";
const SEASON_URL: &str = "https://api.bilibili.com/pgc/view/web/season";

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
}

#[cfg(test)]
mod tests {
    use crate::{PgcIndex, PgcSeason};

    const INDEX_FIXTURE: &str = include_str!("../../fixtures/pgc_index.json");
    const SEASON_FIXTURE: &str = include_str!("../../fixtures/pgc_season.json");

    #[derive(serde::Deserialize)]
    struct IndexEnvelope {
        data: PgcIndex,
    }

    #[derive(serde::Deserialize)]
    struct SeasonEnvelope {
        result: PgcSeason,
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
}
