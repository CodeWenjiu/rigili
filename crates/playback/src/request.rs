use rigili_bilibili::{ApiError, Dash, RecommendVideo, VideoInfo};

use rigili_core::backend::Backend;

#[derive(Clone, Debug)]
pub struct PlaybackRequest {
    pub title: String,
    pub source: PlaybackSource,
}

#[derive(Clone, Debug)]
pub enum PlaybackSource {
    Video { bvid: String, cid: Option<u64> },
    Live { room_id: u64 },
}

impl PlaybackRequest {
    pub fn video(title: String, bvid: String, cid: Option<u64>) -> Self {
        Self {
            title,
            source: PlaybackSource::Video { bvid, cid },
        }
    }

    pub fn live(title: String, room_id: u64) -> Self {
        Self {
            title,
            source: PlaybackSource::Live { room_id },
        }
    }
}

pub(super) struct Resolved {
    pub(super) info: VideoInfo,
    pub(super) related: Vec<RecommendVideo>,
    pub(super) followers: u64,
    pub(super) owner_likes: u64,
    pub(super) cid: u64,
    pub(super) streams: Streams,
}

pub(crate) struct Streams {
    pub(super) video: String,
    pub(super) audio: Option<String>,
}

pub(super) async fn resolve(
    backend: Backend,
    bvid: String,
    cid: Option<u64>,
) -> Result<Resolved, ApiError> {
    let detail = backend.video_detail(&bvid).await?;
    let info = detail.view;
    let cid = cid.unwrap_or(info.cid);
    let streams = resolve_streams(&backend, &bvid, cid).await?;
    Ok(Resolved {
        info,
        related: detail.related,
        followers: detail.card.follower,
        owner_likes: detail.card.like_num,
        cid,
        streams,
    })
}

pub(super) async fn resolve_streams(
    backend: &Backend,
    bvid: &str,
    cid: u64,
) -> Result<Streams, ApiError> {
    let playurl = backend.playurl(bvid, cid).await?;
    let dash = playurl
        .dash
        .as_ref()
        .ok_or_else(|| ApiError::InvalidResponse("该视频没有可用的 DASH 流".into()))?;
    best_streams(dash).ok_or_else(|| ApiError::InvalidResponse("没有可用的视频流".into()))
}

pub async fn resolve_preview(
    backend: Backend,
    bvid: String,
    cid: Option<u64>,
) -> Result<String, ApiError> {
    let cid = match cid {
        Some(cid) => cid,
        None => backend.view(&bvid).await?.cid,
    };
    let playurl = backend.playurl(&bvid, cid).await?;
    playurl
        .dash
        .as_ref()
        .and_then(|dash| dash.preview_video())
        .map(|video| video.base_url.clone())
        .ok_or_else(|| ApiError::InvalidResponse("没有可用的预览流".into()))
}

fn best_streams(dash: &Dash) -> Option<Streams> {
    let video = dash.best_video()?;
    Some(Streams {
        video: video.base_url.clone(),
        audio: dash.best_audio().map(|audio| audio.base_url.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::best_streams;
    use rigili_bilibili::{Dash, DashAudio, DashVideo};

    #[test]
    fn pairs_video_with_audio_streams() {
        let dash = Dash {
            video: vec![DashVideo {
                id: 80,
                base_url: "https://example.invalid/avc.m4s".into(),
                bandwidth: 600_000,
                width: 1920,
                height: 1080,
                codecs: "avc1".into(),
                codecid: 7,
            }],
            audio: vec![DashAudio {
                id: 30280,
                base_url: "https://example.invalid/audio.m4s".into(),
                bandwidth: 196_608,
                codecs: "mp4a.40.2".into(),
            }],
        };

        let streams = best_streams(&dash).expect("streams");
        assert_eq!(streams.video, "https://example.invalid/avc.m4s");
        assert_eq!(
            streams.audio.as_deref(),
            Some("https://example.invalid/audio.m4s")
        );
    }
}
