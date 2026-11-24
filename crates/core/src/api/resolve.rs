use rigili_bilibili::{ApiError, Client, EpisodeId, PgcEpisode, PgcProgress, PgcSeason};

use crate::ui::OpenVideo;

pub async fn resolve_season(
    client: &Client,
    season_id: u64,
) -> Result<Option<OpenVideo>, ApiError> {
    let requested = season_id;
    let PgcSeason {
        season_id,
        title,
        episodes,
        user_status,
    } = client.pgc_season(requested).await?;
    let season_id = if season_id != 0 { season_id } else { requested };
    let Some(episode) = target_episode(&episodes, user_status.progress.as_ref()) else {
        return Ok(None);
    };

    Ok(Some(episode_open_video(title, season_id, episode)))
}

fn target_episode(episodes: &[PgcEpisode], progress: Option<&PgcProgress>) -> Option<PgcEpisode> {
    let last = progress
        .map(|progress| progress.last_ep_id)
        .filter(|ep_id| *ep_id != 0);
    last.and_then(|ep_id| {
        episodes
            .iter()
            .find(|episode| episode.ep_id == ep_id && !episode.bvid.is_empty())
            .cloned()
    })
    .or_else(|| {
        episodes
            .iter()
            .find(|episode| !episode.bvid.is_empty())
            .cloned()
    })
}

fn episode_open_video(season_title: String, season_id: u64, episode: PgcEpisode) -> OpenVideo {
    let episode_id = (episode.ep_id != 0).then_some(EpisodeId {
        ep_id: episode.ep_id,
        season_id,
    });
    let title = if episode.long_title.is_empty() {
        season_title
    } else {
        format!("{} {}", season_title, episode.long_title)
    };

    OpenVideo {
        bvid: episode.bvid,
        cid: (episode.cid != 0).then_some(episode.cid),
        title,
        new_tab: false,
        episode: episode_id,
    }
}

#[cfg(test)]
mod tests {
    use rigili_bilibili::{EpisodeId, PgcEpisode, PgcProgress};

    use super::{episode_open_video, target_episode};

    fn episode(ep_id: u64, bvid: &str) -> PgcEpisode {
        PgcEpisode {
            bvid: bvid.into(),
            ep_id,
            cid: ep_id * 10,
            ..Default::default()
        }
    }

    #[test]
    fn reopens_the_last_watched_episode() {
        let episodes = [episode(1, "BV1"), episode(2, "BV2")];
        let progress = PgcProgress {
            last_ep_id: 2,
            last_time_seconds: 30.,
        };

        let picked = target_episode(&episodes, Some(&progress)).expect("episode");
        assert_eq!(picked.ep_id, 2);
    }

    #[test]
    fn falls_back_to_the_first_playable_episode() {
        let episodes = [episode(1, ""), episode(2, "BV2")];
        assert_eq!(target_episode(&episodes, None).map(|ep| ep.ep_id), Some(2));

        let stale = PgcProgress {
            last_ep_id: 99,
            ..Default::default()
        };
        assert_eq!(
            target_episode(&episodes, Some(&stale)).map(|ep| ep.ep_id),
            Some(2)
        );
    }

    #[test]
    fn keeps_the_season_and_episode_identity() {
        let video = episode_open_video(
            "鬼灭之刃 柱训练篇".into(),
            47_836,
            PgcEpisode {
                aid: 170_001,
                bvid: "BV15i421v7FQ".into(),
                cid: 1_602_741_036,
                ep_id: 826_497,
                long_title: "为了消灭鬼舞辻无惨".into(),
                ..Default::default()
            },
        );

        assert_eq!(video.title, "鬼灭之刃 柱训练篇 为了消灭鬼舞辻无惨");
        assert_eq!(video.cid, Some(1_602_741_036));
        assert_eq!(
            video.episode,
            Some(EpisodeId {
                ep_id: 826_497,
                season_id: 47_836,
            })
        );
    }

    #[test]
    fn falls_back_to_the_season_title_for_a_plain_episode() {
        let video = episode_open_video(
            "某电影".into(),
            7,
            PgcEpisode {
                bvid: "BV1".into(),
                cid: 9,
                ep_id: 0,
                ..Default::default()
            },
        );

        assert_eq!(video.title, "某电影");
        assert_eq!(video.episode, None);
    }
}
