use crate::{
    ApiError, BangumiFollows, Client, CreatedFolder, FavFolders, FavItems, HistoryPage, NavStat,
    WatchLater, WatchReport,
};

const NAV_STAT_URL: &str = "https://api.bilibili.com/x/web-interface/nav/stat";
const FAV_FOLDERS_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/created/list";
const FAV_FOLDERS_WITH_STATE_URL: &str =
    "https://api.bilibili.com/x/v3/fav/folder/created/list-all";
const COLLECTED_FOLDERS_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/collected/list";
const FAV_ITEMS_URL: &str = "https://api.bilibili.com/x/v3/fav/resource/list";
const FOLDER_ADD_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/add";
const FOLDER_SORT_URL: &str = "https://api.bilibili.com/x/v3/fav/folder/sort";
const BANGUMI_FOLLOW_URL: &str = "https://api.bilibili.com/x/space/bangumi/follow/list";
const HISTORY_URL: &str = "https://api.bilibili.com/x/web-interface/history/cursor";
const HISTORY_REPORT_URL: &str = "https://api.bilibili.com/x/click-interface/web/heartbeat";
const WATCH_LATER_URL: &str = "https://api.bilibili.com/x/v2/history/toview";
const WATCH_LATER_ADD_URL: &str = "https://api.bilibili.com/x/v2/history/toview/add";
const WATCH_LATER_DEL_URL: &str = "https://api.bilibili.com/x/v2/history/toview/del";
const PAGE_SIZE: u32 = 20;
const FOLDER_LIMIT: u32 = 100;

impl Client {
    pub async fn nav_stat(&self) -> Result<NavStat, ApiError> {
        self.get_json(NAV_STAT_URL, &[]).await
    }

    pub async fn fav_folders(&self, mid: u64) -> Result<FavFolders, ApiError> {
        self.get_json(
            FAV_FOLDERS_URL,
            &[
                ("up_mid", mid.to_string()),
                ("pn", 1.to_string()),
                ("ps", FOLDER_LIMIT.to_string()),
            ],
        )
        .await
    }

    pub async fn fav_folders_with_state(&self, mid: u64, rid: u64) -> Result<FavFolders, ApiError> {
        self.get_json(
            FAV_FOLDERS_WITH_STATE_URL,
            &[("up_mid", mid.to_string()), ("rid", rid.to_string())],
        )
        .await
    }

    pub async fn collected_folders(&self, mid: u64) -> Result<FavFolders, ApiError> {
        self.get_json(
            COLLECTED_FOLDERS_URL,
            &[
                ("up_mid", mid.to_string()),
                ("pn", 1.to_string()),
                ("ps", FOLDER_LIMIT.to_string()),
                ("platform", "web".into()),
            ],
        )
        .await
    }

    pub async fn fav_items(
        &self,
        media_id: u64,
        page: u32,
        keyword: &str,
    ) -> Result<FavItems, ApiError> {
        let page = page.max(1);

        self.get_json(
            FAV_ITEMS_URL,
            &[
                ("media_id", media_id.to_string()),
                ("pn", page.to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("keyword", keyword.to_owned()),
                ("platform", "web".into()),
            ],
        )
        .await
    }

    pub async fn create_folder(
        &self,
        title: &str,
        private: bool,
    ) -> Result<CreatedFolder, ApiError> {
        self.post_form(
            FOLDER_ADD_URL,
            &[
                ("title", title.to_owned()),
                ("intro", String::new()),
                ("privacy", if private { "1" } else { "0" }.to_owned()),
                ("csrf", self.csrf()?),
            ],
        )
        .await
    }

    pub async fn sort_folders(&self, ids: &[u64]) -> Result<(), ApiError> {
        if ids.is_empty() {
            return Ok(());
        }

        let sort = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        self.post_form_ack(
            FOLDER_SORT_URL,
            &[
                ("sort", sort),
                ("csrf", self.csrf()?),
                ("platform", "web".into()),
            ],
        )
        .await
    }

    pub async fn bangumi_follows(
        &self,
        mid: u64,
        season_type: u32,
        page: u32,
    ) -> Result<BangumiFollows, ApiError> {
        self.get_json(
            BANGUMI_FOLLOW_URL,
            &[
                ("type", season_type.to_string()),
                ("follow_status", 0.to_string()),
                ("pn", page.max(1).to_string()),
                ("ps", PAGE_SIZE.to_string()),
                ("vmid", mid.to_string()),
            ],
        )
        .await
    }

    pub async fn history(&self, max: u64, view_at: i64) -> Result<HistoryPage, ApiError> {
        self.get_json(
            HISTORY_URL,
            &[
                ("max", max.to_string()),
                ("view_at", view_at.to_string()),
                ("business", String::new()),
            ],
        )
        .await
    }

    pub async fn report_history(&self, report: &WatchReport) -> Result<(), ApiError> {
        let form = history_form(report, self.account_uid()?, self.csrf()?);
        self.post_form_ack(HISTORY_REPORT_URL, &form).await
    }

    pub async fn watch_later(&self) -> Result<WatchLater, ApiError> {
        self.get_json(WATCH_LATER_URL, &[]).await
    }

    pub async fn watch_later_add(&self, bvid: &str) -> Result<(), ApiError> {
        self.post_form_ack(
            WATCH_LATER_ADD_URL,
            &[("bvid", bvid.to_owned()), ("csrf", self.csrf()?)],
        )
        .await
    }

    pub async fn watch_later_remove(&self, aid: u64) -> Result<(), ApiError> {
        self.post_form_ack(
            WATCH_LATER_DEL_URL,
            &[("aid", aid.to_string()), ("csrf", self.csrf()?)],
        )
        .await
    }
}

fn history_form(report: &WatchReport, mid: u64, csrf: String) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("bvid", report.bvid.clone()),
        ("cid", report.cid.to_string()),
        ("mid", mid.to_string()),
        ("csrf", csrf),
        ("played_time", report.played.to_string()),
        ("realtime", report.realtime.to_string()),
        ("start_ts", report.started_at.to_string()),
        ("dt", "2".to_owned()),
        ("play_type", "0".to_owned()),
        ("video_duration", report.duration.to_string()),
        ("last_play_progress_time", report.last.to_string()),
        ("max_play_progress_time", report.max.to_string()),
    ];
    if report.episode.is_none() && report.aid != 0 {
        form.push(("aid", report.aid.to_string()));
    }
    match report.episode {
        Some(episode) => {
            form.push(("epid", episode.ep_id.to_string()));
            form.push(("sid", episode.season_id.to_string()));
            form.push(("type", "4".to_owned()));
            form.push(("sub_type", "1".to_owned()));
        }
        None => {
            form.push(("type", "3".to_owned()));
            form.push(("sub_type", "0".to_owned()));
        }
    }
    form
}

#[cfg(test)]
mod tests {
    use super::history_form;
    use crate::{EpisodeId, WatchReport};

    fn report() -> WatchReport {
        WatchReport {
            aid: 170_001,
            bvid: "BV15i421v7FQ".into(),
            cid: 1_602_741_036,
            episode: None,
            played: 90,
            duration: 2938,
            started_at: 1_700_000_000,
            realtime: 10,
            last: 80,
            max: 90,
        }
    }

    fn value<'a>(form: &'a [(&'static str, String)], key: &str) -> &'a str {
        form.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("missing {key}"))
    }

    #[test]
    fn reports_a_video_through_the_ugc_heartbeat() {
        let form = history_form(&report(), 42, "token".into());
        assert_eq!(value(&form, "aid"), "170001");
        assert_eq!(value(&form, "type"), "3");
        assert_eq!(value(&form, "sub_type"), "0");
        assert!(form.iter().all(|(key, _)| *key != "epid"));
    }

    #[test]
    fn reports_an_episode_through_the_pgc_heartbeat() {
        let mut report = report();
        report.episode = Some(EpisodeId {
            ep_id: 826_497,
            season_id: 47_836,
        });
        let form = history_form(&report, 42, "token".into());
        assert_eq!(value(&form, "epid"), "826497");
        assert_eq!(value(&form, "sid"), "47836");
        assert_eq!(value(&form, "type"), "4");
        assert_eq!(value(&form, "sub_type"), "1");
        assert_eq!(value(&form, "played_time"), "90");
        assert!(form.iter().all(|(key, _)| *key != "aid"));
    }

    #[test]
    fn omits_an_unknown_aid() {
        let mut report = report();
        report.aid = 0;
        let form = history_form(&report, 42, "token".into());
        assert!(form.iter().all(|(key, _)| *key != "aid"));
    }
}
