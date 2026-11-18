use std::sync::Arc;
use std::time::{Duration, Instant};

use rigili_bilibili::{
    ApiError, Client, Dash, DashVideo, PlayUrl, PlayerInfo, RecommendVideo, VideoInfo,
};
use tokio::task::JoinSet;

const PROBE_FAST_ENOUGH: Duration = Duration::from_millis(500);
const PROBE_DEADLINE: Duration = Duration::from_millis(2500);

#[derive(Clone, Debug)]
pub struct PlaybackRequest {
    pub title: String,
    pub source: PlaybackSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub(super) player_info: PlayerInfo,
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
    client: Arc<Client>,
    bvid: String,
    cid: Option<u64>,
    preferred_quality: Option<u32>,
) -> Result<Resolved, ApiError> {
    let started = Instant::now();
    rigili_monitor::monitor!("stream", "resolving {bvid} (cid {cid:?})");
    let detail = client.video_detail(&bvid).await?;
    let info = detail.view;
    let cid = cid.unwrap_or(info.cid);
    let (streams, player_info) = tokio::join!(
        resolve_streams(&client, &bvid, cid, preferred_quality),
        client.player_info(&bvid, cid)
    );
    let streams = streams?;
    let player_info = player_info.unwrap_or_else(|error| {
        // Chapters and the resume point are a bonus; a failed call must not
        // keep the video from playing.
        rigili_monitor::monitor!("stream", "player info {bvid}/{cid} failed: {error}");
        PlayerInfo::default()
    });
    rigili_monitor::monitor!(
        "stream",
        "resolved {bvid} cid {cid} in {:?}",
        started.elapsed()
    );
    Ok(Resolved {
        info,
        related: detail.related,
        followers: detail.card.follower,
        owner_likes: detail.card.like_num,
        cid,
        streams,
        player_info,
    })
}

pub(super) async fn resolve_streams(
    client: &Arc<Client>,
    bvid: &str,
    cid: u64,
    preferred_quality: Option<u32>,
) -> Result<Streams, ApiError> {
    let playurl = client.playurl(bvid, cid).await?;
    let Some(dash) = playurl.dash.as_ref() else {
        rigili_monitor::monitor!("stream", "playurl {bvid}/{cid} returned no DASH manifest");
        return Err(ApiError::InvalidResponse("该视频没有可用的 DASH 流".into()));
    };
    rigili_monitor::monitor!(
        "stream",
        "playurl {bvid}/{cid}: {} video tracks, {} audio tracks",
        dash.video.len(),
        dash.audio.len()
    );
    let Some((mut streams, video_urls, audio_urls)) = select_streams(&playurl, preferred_quality)
    else {
        return Err(ApiError::InvalidResponse("没有可用的视频流".into()));
    };

    let video_primary = streams.video.clone();
    let audio_primary = streams.audio.clone();
    let video_probe = fastest_stream(client, "video", &video_primary, video_urls);
    let audio_probe = async {
        match audio_primary.as_deref() {
            Some(primary) => fastest_stream(client, "audio", primary, audio_urls).await,
            None => None,
        }
    };
    let (video, audio) = tokio::join!(video_probe, audio_probe);
    if let Some(url) = video {
        let video_id = streams.video_id;
        if let Some(quality) = streams
            .qualities
            .iter_mut()
            .find(|quality| quality.id == video_id)
        {
            quality.url = url.clone();
        }
        streams.video = url;
    }
    if let Some(url) = audio {
        streams.audio = Some(url);
    }
    Ok(streams)
}

pub async fn resolve_preview(
    client: Arc<Client>,
    bvid: String,
    cid: Option<u64>,
) -> Result<String, ApiError> {
    let cid = match cid {
        Some(cid) => cid,
        None => client.view(&bvid).await?.cid,
    };
    let playurl = client.playurl(&bvid, cid).await?;
    playurl
        .dash
        .as_ref()
        .and_then(|dash| dash.preview_video())
        .map(|video| video.base_url.clone())
        .ok_or_else(|| ApiError::InvalidResponse("没有可用的预览流".into()))
}

fn select_streams(
    playurl: &PlayUrl,
    preferred_quality: Option<u32>,
) -> Option<(Streams, Vec<String>, Vec<String>)> {
    let dash = playurl.dash.as_ref()?;
    let video = preferred_video(dash, preferred_quality)?;
    let audio = dash.best_audio();
    rigili_monitor::monitor!(
        "stream",
        "chose video qn={} {}x{} codec={} codecid={} bandwidth={} host={}",
        video.id,
        video.width,
        video.height,
        video.codecs,
        video.codecid,
        video.bandwidth,
        rigili_monitor::host(&video.base_url)
    );
    rigili_monitor::monitor!(
        "stream",
        "chose audio id={} codec={} bandwidth={} host={}",
        audio.map(|audio| audio.id).unwrap_or_default(),
        audio.map(|audio| audio.codecs.as_str()).unwrap_or("none"),
        audio.map(|audio| audio.bandwidth).unwrap_or_default(),
        audio
            .map(|audio| rigili_monitor::host(&audio.base_url))
            .unwrap_or("none")
    );
    let video_urls = candidates(&video.base_url, &video.backup_urls);
    let audio_urls = audio
        .map(|audio| candidates(&audio.base_url, &audio.backup_urls))
        .unwrap_or_default();
    Some((
        Streams {
            video: video.base_url.clone(),
            video_id: video.id,
            audio: audio.map(|audio| audio.base_url.clone()),
            qualities: quality_tracks(playurl, dash),
        },
        video_urls,
        audio_urls,
    ))
}

fn preferred_video(dash: &Dash, preferred: Option<u32>) -> Option<&DashVideo> {
    let Some(preferred) = preferred else {
        return dash.best_video();
    };
    let mut ids: Vec<u32> = dash
        .video
        .iter()
        .filter(|video| playable(video))
        .map(|video| video.id)
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.dedup();
    let id = ids
        .iter()
        .copied()
        .find(|id| *id <= preferred)
        .or_else(|| ids.last().copied())?;
    video_track(dash, id)
}

fn video_track(dash: &Dash, id: u32) -> Option<&DashVideo> {
    dash.video
        .iter()
        .filter(|video| video.id == id && playable(video))
        .max_by_key(|video| (video.codecid == 7, video.bandwidth))
}

fn playable(video: &DashVideo) -> bool {
    !video.base_url.is_empty() && video.width > 0 && video.height > 0
}

fn candidates(primary: &str, backups: &[String]) -> Vec<String> {
    let mut urls = Vec::with_capacity(backups.len() + 1);
    for url in std::iter::once(primary).chain(backups.iter().map(String::as_str)) {
        if !url.is_empty() && !urls.iter().any(|seen: &String| seen.as_str() == url) {
            urls.push(url.to_owned());
        }
    }
    urls
}

async fn fastest_stream(
    client: &Arc<Client>,
    kind: &'static str,
    primary: &str,
    candidates: Vec<String>,
) -> Option<String> {
    let primary_host = rigili_monitor::host(primary);
    if candidates.len() <= 1
        || candidates
            .iter()
            .all(|url| rigili_monitor::host(url) == primary_host)
    {
        return None;
    }

    let mut set = JoinSet::new();
    for url in candidates {
        let client = Arc::clone(client);
        set.spawn(async move {
            let elapsed = client.probe_stream(&url).await;
            rigili_monitor::monitor!(
                "stream",
                "probe {kind} host={} -> {elapsed:?}",
                rigili_monitor::host(&url)
            );
            (url, elapsed)
        });
    }

    let best = {
        let mut best: Option<(String, Duration)> = None;
        let _ = tokio::time::timeout(PROBE_DEADLINE, async {
            while let Some(joined) = set.join_next().await {
                if let Ok((url, Some(elapsed))) = joined {
                    if best.as_ref().is_none_or(|(_, best)| elapsed < *best) {
                        best = Some((url, elapsed));
                    }
                    if elapsed <= PROBE_FAST_ENOUGH {
                        break;
                    }
                }
            }
        })
        .await;
        best
    };

    match best {
        Some((url, elapsed)) => {
            rigili_monitor::monitor!(
                "stream",
                "using {kind} host={} (probed {elapsed:?})",
                rigili_monitor::host(&url)
            );
            Some(url)
        }
        None => {
            rigili_monitor::monitor!(
                "stream",
                "no {kind} probe succeeded within {PROBE_DEADLINE:?}, keeping host={primary_host}"
            );
            None
        }
    }
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
            let video = video_track(dash, id)?;
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
    use super::{candidates, select_streams};
    use rigili_bilibili::{Dash, DashAudio, DashVideo, PlayUrl};

    #[test]
    fn deduplicates_stream_candidates() {
        let urls = candidates(
            "https://a.invalid/v.m4s",
            &[
                "https://b.invalid/v.m4s".into(),
                "https://a.invalid/v.m4s".into(),
                String::new(),
                "https://b.invalid/v.m4s".into(),
            ],
        );
        assert_eq!(
            urls.iter().map(String::as_str).collect::<Vec<_>>(),
            ["https://a.invalid/v.m4s", "https://b.invalid/v.m4s"]
        );
    }

    #[tokio::test]
    async fn prefers_the_fastest_stream_candidate() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::Arc,
            thread,
            time::Duration,
        };

        use rigili_bilibili::Client;

        fn serve(delay: Duration) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind probe server");
            let port = listener.local_addr().expect("probe server address").port();
            thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                thread::sleep(delay);
                let body = vec![0u8; 64 * 1024];
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body);
            });
            format!("http://127.0.0.1:{port}/v.m4s")
        }

        let slow = serve(Duration::from_millis(700));
        let fast = serve(Duration::ZERO);
        let client = Arc::new(Client::with_data_dir(None).expect("probe client"));
        let chosen =
            super::fastest_stream(&client, "video", &slow, vec![slow.clone(), fast.clone()]).await;
        assert_eq!(chosen.as_deref(), Some(fast.as_str()));
    }

    #[test]
    fn pairs_video_with_audio_streams() {
        let dash = Dash {
            video: vec![DashVideo {
                id: 80,
                base_url: "https://example.invalid/avc.m4s".into(),
                backup_urls: Vec::new(),
                bandwidth: 600_000,
                width: 1920,
                height: 1080,
                codecs: "avc1".into(),
                codecid: 7,
            }],
            audio: vec![DashAudio {
                id: 30280,
                base_url: "https://example.invalid/audio.m4s".into(),
                backup_urls: Vec::new(),
                bandwidth: 196_608,
                codecs: "mp4a.40.2".into(),
            }],
        };
        let playurl = PlayUrl {
            dash: Some(dash),
            accept_quality: vec![80],
            accept_description: vec!["1080P 高清".into()],
        };

        let streams = select_streams(&playurl, None).expect("streams").0;
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
            backup_urls: Vec::new(),
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

        let streams = select_streams(&playurl, None).expect("streams").0;
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

    #[test]
    fn falls_back_to_the_best_quality_below_the_preference() {
        let video = |id, url: &str, bandwidth| DashVideo {
            id,
            base_url: url.into(),
            backup_urls: Vec::new(),
            bandwidth,
            width: 1920,
            height: 1080,
            codecs: String::new(),
            codecid: 7,
        };
        let playurl = PlayUrl {
            dash: Some(Dash {
                video: vec![video(80, "1080p", 600_000), video(64, "720p", 300_000)],
                audio: Vec::new(),
            }),
            accept_quality: vec![80, 64],
            accept_description: Vec::new(),
        };

        let exact = select_streams(&playurl, Some(80)).expect("streams").0;
        assert_eq!(exact.video, "1080p");
        assert_eq!(exact.video_id, 80);

        let below = select_streams(&playurl, Some(64)).expect("streams").0;
        assert_eq!(below.video, "720p");
        assert_eq!(below.video_id, 64);

        let unavailable = select_streams(&playurl, Some(32)).expect("streams").0;
        assert_eq!(unavailable.video, "720p");
        assert_eq!(unavailable.video_id, 64);

        let above = select_streams(&playurl, Some(127)).expect("streams").0;
        assert_eq!(above.video, "1080p");
        assert_eq!(above.video_id, 80);

        let only_high = PlayUrl {
            dash: Some(Dash {
                video: vec![video(80, "1080p", 600_000)],
                audio: Vec::new(),
            }),
            accept_quality: vec![80],
            accept_description: Vec::new(),
        };
        let lowest = select_streams(&only_high, Some(32)).expect("streams").0;
        assert_eq!(lowest.video_id, 80);
    }
}
