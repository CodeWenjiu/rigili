use rigili_bilibili::{ApiError, Dash, VideoInfo};

use crate::backend::Backend;

#[derive(Clone, Debug)]
pub struct PlaybackRequest {
    pub title: String,
    pub bvid: String,
    pub cid: Option<u64>,
}

pub(super) struct Resolved {
    pub(super) info: VideoInfo,
    pub(super) cid: u64,
    pub(super) streams: Streams,
}

pub(super) struct Streams {
    pub(super) video: String,
    pub(super) audio: Option<String>,
}

pub(super) async fn resolve(
    backend: Backend,
    bvid: String,
    cid: Option<u64>,
) -> Result<Resolved, ApiError> {
    let info = backend.view(&bvid).await?;
    let cid = cid.unwrap_or(info.cid);
    let streams = resolve_streams(&backend, &bvid, cid).await?;
    Ok(Resolved { info, cid, streams })
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
    fn pairs_avc_video_with_best_audio() {
        let dash = Dash {
            video: vec![
                DashVideo {
                    id: 80,
                    base_url: "https://example.invalid/hevc.m4s".into(),
                    bandwidth: 900_000,
                    width: 1920,
                    height: 1080,
                    codecs: "hvc1".into(),
                    codecid: 12,
                },
                DashVideo {
                    id: 80,
                    base_url: "https://example.invalid/avc.m4s".into(),
                    bandwidth: 600_000,
                    width: 1920,
                    height: 1080,
                    codecs: "avc1".into(),
                    codecid: 7,
                },
            ],
            audio: vec![
                DashAudio {
                    id: 30216,
                    base_url: "https://example.invalid/audio-64.m4s".into(),
                    bandwidth: 65_536,
                    codecs: "mp4a.40.2".into(),
                },
                DashAudio {
                    id: 30280,
                    base_url: "https://example.invalid/audio-192.m4s".into(),
                    bandwidth: 196_608,
                    codecs: "mp4a.40.2".into(),
                },
            ],
        };

        let streams = best_streams(&dash).expect("streams");
        assert_eq!(streams.video, "https://example.invalid/avc.m4s");
        assert_eq!(
            streams.audio.as_deref(),
            Some("https://example.invalid/audio-192.m4s")
        );
    }
}
