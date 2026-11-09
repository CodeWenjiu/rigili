use gpuili_bilibili::{ApiError, Dash};

use crate::backend::Backend;

#[derive(Clone, Debug)]
pub struct PlaybackRequest {
    pub title: String,
    pub source: PlaybackSource,
}

#[derive(Clone, Debug)]
pub enum PlaybackSource {
    Video {
        bvid: String,
        cid: Option<u64>,
    },
    Stream {
        video: String,
        audio: Option<String>,
    },
}

impl PlaybackRequest {
    pub fn video(title: impl Into<String>, bvid: impl Into<String>, cid: Option<u64>) -> Self {
        Self {
            title: title.into(),
            source: PlaybackSource::Video {
                bvid: bvid.into(),
                cid,
            },
        }
    }

    pub fn stream(title: impl Into<String>, video: String, audio: Option<String>) -> Self {
        Self {
            title: title.into(),
            source: PlaybackSource::Stream { video, audio },
        }
    }
}

pub(super) struct Streams {
    pub(super) video: String,
    pub(super) audio: Option<String>,
}

pub(super) async fn resolve(
    backend: Backend,
    bvid: String,
    cid: Option<u64>,
) -> Result<(Option<String>, Streams), ApiError> {
    let (cid, title) = match cid {
        Some(cid) => (cid, None),
        None => {
            let info = backend.view(&bvid).await?;
            (info.cid, Some(info.title))
        }
    };
    let playurl = backend.playurl(&bvid, cid).await?;
    let dash = playurl
        .dash
        .as_ref()
        .ok_or_else(|| ApiError::InvalidResponse("该视频没有可用的 DASH 流".into()))?;
    let streams =
        best_streams(dash).ok_or_else(|| ApiError::InvalidResponse("没有可用的视频流".into()))?;
    Ok((title, streams))
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
    use super::{PlaybackRequest, PlaybackSource};

    #[test]
    fn builds_requests() {
        let video = PlaybackRequest::video("标题", "BV1xx", Some(42));
        assert_eq!(video.title, "标题");
        assert!(matches!(
            video.source,
            PlaybackSource::Video { cid: Some(42), .. }
        ));

        let stream = PlaybackRequest::stream("本地", "/tmp/a.mp4".into(), None);
        assert!(matches!(stream.source, PlaybackSource::Stream { .. }));
    }
}
