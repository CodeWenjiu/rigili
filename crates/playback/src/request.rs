use rigili_bilibili::{ApiError, Dash, PlayUrl, RecommendVideo, VideoInfo};

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
    pub(crate) video: String,
    pub(crate) video_id: u32,
    pub(crate) audio: Option<String>,
    pub(crate) qualities: Vec<VideoQuality>,
}

#[derive(Clone)]
pub(crate) struct VideoQuality {
    pub(crate) id: u32,
    pub(crate) label: String,
    pub(crate) short_label: String,
    pub(crate) url: String,
}

impl Streams {
    pub(crate) fn single(video: String) -> Self {
        Self {
            video,
            video_id: 0,
            audio: None,
            qualities: Vec::new(),
        }
    }
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
    if playurl.dash.is_none() {
        return Err(ApiError::InvalidResponse("该视频没有可用的 DASH 流".into()));
    }
    best_streams(&playurl).ok_or_else(|| ApiError::InvalidResponse("没有可用的视频流".into()))
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

fn best_streams(playurl: &PlayUrl) -> Option<Streams> {
    let dash = playurl.dash.as_ref()?;
    let video = dash.best_video()?;
    Some(Streams {
        video: video.base_url.clone(),
        video_id: video.id,
        audio: dash.best_audio().map(|audio| audio.base_url.clone()),
        qualities: quality_tracks(playurl, dash),
    })
}

fn quality_tracks(playurl: &PlayUrl, dash: &Dash) -> Vec<VideoQuality> {
    let ids = if playurl.accept_quality.is_empty() {
        let mut ids: Vec<u32> = dash.video.iter().map(|video| video.id).collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids.dedup();
        ids
    } else {
        playurl.accept_quality.clone()
    };

    ids.into_iter()
        .filter_map(|id| {
            let video = dash
                .video
                .iter()
                .filter(|video| {
                    video.id == id
                        && !video.base_url.is_empty()
                        && video.width > 0
                        && video.height > 0
                })
                .max_by_key(|video| (video.codecid == 7, video.bandwidth))?;
            let description = playurl
                .accept_quality
                .iter()
                .position(|quality| *quality == id)
                .and_then(|index| playurl.accept_description.get(index))
                .map(String::as_str)
                .filter(|text| !text.trim().is_empty());
            let short_label = quality_label(id, video.height);
            Some(VideoQuality {
                id,
                label: description
                    .map(str::to_owned)
                    .unwrap_or_else(|| short_label.clone()),
                short_label,
                url: video.base_url.clone(),
            })
        })
        .collect()
}

fn quality_label(id: u32, height: u32) -> String {
    match id {
        127 => "8K".into(),
        126 => "杜比视界".into(),
        125 => "HDR".into(),
        120 => "4K".into(),
        116 => "1080P60".into(),
        112 => "1080P+".into(),
        80 => "1080P".into(),
        74 => "720P60".into(),
        64 => "720P".into(),
        32 => "480P".into(),
        16 => "360P".into(),
        6 => "240P".into(),
        _ if height >= 2160 => "4K".into(),
        _ if height >= 1440 => "2K".into(),
        _ if height >= 1080 => "1080P".into(),
        _ if height >= 720 => "720P".into(),
        _ if height >= 480 => "480P".into(),
        _ if height > 0 => format!("{height}P"),
        _ => "未知".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::best_streams;
    use rigili_bilibili::{Dash, DashAudio, DashVideo, PlayUrl};

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
        let playurl = PlayUrl {
            dash: Some(dash),
            accept_quality: vec![80],
            accept_description: vec!["1080P 高清".into()],
        };

        let streams = best_streams(&playurl).expect("streams");
        assert_eq!(streams.video, "https://example.invalid/avc.m4s");
        assert_eq!(
            streams.audio.as_deref(),
            Some("https://example.invalid/audio.m4s")
        );
        assert_eq!(streams.video_id, 80);
        assert_eq!(streams.qualities.len(), 1);
        assert_eq!(streams.qualities[0].label, "1080P 高清");
        assert_eq!(streams.qualities[0].short_label, "1080P");
    }

    #[test]
    fn lists_qualities_in_api_order_and_prefers_avc() {
        let video = |id, url: &str, codecid, bandwidth| DashVideo {
            id,
            base_url: url.into(),
            bandwidth,
            width: 1920,
            height: 1080,
            codecs: String::new(),
            codecid,
        };
        let playurl = PlayUrl {
            dash: Some(Dash {
                video: vec![
                    video(80, "hevc", 12, 900_000),
                    video(80, "avc", 7, 600_000),
                    video(64, "720p", 7, 300_000),
                ],
                audio: Vec::new(),
            }),
            accept_quality: vec![80, 64],
            accept_description: Vec::new(),
        };

        let streams = best_streams(&playurl).expect("streams");
        assert_eq!(streams.video, "avc");
        assert_eq!(streams.video_id, 80);
        let qualities: Vec<_> = streams
            .qualities
            .iter()
            .map(|quality| {
                (
                    quality.id,
                    quality.url.as_str(),
                    quality.short_label.as_str(),
                )
            })
            .collect();
        assert_eq!(qualities, [(80, "avc", "1080P"), (64, "720p", "720P")]);
    }
}
