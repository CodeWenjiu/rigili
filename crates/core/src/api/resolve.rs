use rigili_bilibili::{ApiError, Client};

use crate::ui::OpenVideo;

pub async fn resolve_season(
    client: &Client,
    season_id: u64,
) -> Result<Option<OpenVideo>, ApiError> {
    let season = client.pgc_season(season_id).await?;
    let Some(episode) = season
        .episodes
        .into_iter()
        .find(|episode| !episode.bvid.is_empty())
    else {
        return Ok(None);
    };
    let title = if episode.long_title.is_empty() {
        season.title
    } else {
        format!("{} {}", season.title, episode.long_title)
    };

    Ok(Some(OpenVideo {
        bvid: episode.bvid,
        cid: (episode.cid != 0).then_some(episode.cid),
        title,
        new_tab: false,
    }))
}
