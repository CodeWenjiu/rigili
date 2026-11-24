use std::sync::Arc;
use std::time::{Duration, Instant};

use rigili_bilibili::{
    ApiError, Client, Dash, DashAudio, DashVideo, EpisodeId, PgcEpisode, PgcPlayUrl, PgcSeason,
    PlayUrl, PlayerInfo, RecommendVideo, VideoInfo, VideoPage,
};
use serde::{Deserialize, Serialize};
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
    Video {
        bvid: String,
        cid: Option<u64>,
        episode: Option<EpisodeId>,
    },
    Live {
        room_id: u64,
    },
}

impl PlaybackRequest {
    pub fn video(title: String, bvid: String, cid: Option<u64>) -> Self {
        Self {
            title,
            source: PlaybackSource::Video {
                bvid,
                cid,
                episode: None,
            },
        }
    }

    pub fn episode(title: String, bvid: String, cid: Option<u64>, episode: EpisodeId) -> Self {
        Self {
            title,
            source: PlaybackSource::Video {
                bvid,
                cid,
                episode: Some(episode),
            },
        }
    }

    pub fn live(title: String, room_id: u64) -> Self {
        Self {
            title,
            source: PlaybackSource::Live { room_id },
        }
    }

    pub fn from_source(title: String, source: PlaybackSource) -> Self {
        Self { title, source }
    }
}

impl PlaybackSource {
    pub(crate) fn episode(&self) -> Option<EpisodeId> {
        match self {
            Self::Video { episode, .. } => *episode,
            Self::Live { .. } => None,
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
    pub(super) episode: Option<EpisodeId>,
    pub(super) episodes: Vec<PgcEpisode>,
    pub(super) preview: bool,
}

pub(super) struct ResolvedStreams {
    pub(super) streams: Streams,
    pub(super) preview: bool,
    pub(super) duration: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AudioKind {
    #[default]
    Regular,
    Dolby,
    Flac,
}

impl AudioKind {
    fn rank(self) -> u8 {
        match self {
            Self::Regular => 0,
            Self::Dolby => 1,
            Self::Flac => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AudioTrack {
    pub(crate) kind: AudioKind,
    pub(crate) id: u32,
    pub(crate) label: String,
    pub(crate) url: String,
    pub(crate) backup_urls: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct Streams {
    pub(crate) video: String,
    pub(crate) video_id: u32,
    pub(crate) audio_tracks: Vec<AudioTrack>,
    pub(crate) audio_kind: Option<AudioKind>,
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
            audio_tracks: Vec::new(),
            audio_kind: None,
            qualities: Vec::new(),
        }
    }

    pub(crate) fn select_audio(&mut self, preferred: AudioKind) {
        let preferred = preferred.rank();
        self.audio_kind = self
            .audio_tracks
            .iter()
            .filter(|track| track.kind.rank() <= preferred)
            .max_by_key(|track| track.kind.rank())
            .or_else(|| {
                self.audio_tracks
                    .iter()
                    .min_by_key(|track| track.kind.rank())
            })
            .map(|track| track.kind);
    }

    pub(crate) fn audio(&self) -> Option<&AudioTrack> {
        let kind = self.audio_kind?;
        self.audio_tracks.iter().find(|track| track.kind == kind)
    }

    pub(crate) fn audio_url(&self) -> Option<&str> {
        self.audio().map(|track| track.url.as_str())
    }

    pub(crate) fn set_audio_url(&mut self, url: String) {
        let kind = self.audio_kind;
        if let Some(track) = self
            .audio_tracks
            .iter_mut()
            .find(|track| Some(track.kind) == kind)
        {
            track.url = url;
        }
    }
}

pub(super) async fn resolve(
    client: Arc<Client>,
    title: String,
    bvid: String,
    cid: Option<u64>,
    episode: Option<EpisodeId>,
    preferred_quality: Option<u32>,
    preferred_audio: AudioKind,
) -> Result<Resolved, ApiError> {
    match episode {
        Some(episode) => {
            resolve_episode(
                client,
                title,
                bvid,
                cid,
                episode,
                preferred_quality,
                preferred_audio,
            )
            .await
        }
        None => resolve_video(client, title, bvid, cid, preferred_quality, preferred_audio).await,
    }
}

async fn resolve_video(
    client: Arc<Client>,
    title: String,
    bvid: String,
    cid: Option<u64>,
    preferred_quality: Option<u32>,
    preferred_audio: AudioKind,
) -> Result<Resolved, ApiError> {
    let started = Instant::now();
    rigili_monitor::monitor!("stream", "resolving {bvid} (cid {cid:?})");
    let detail = client.video_detail(&bvid).await?;
    if let Some(ep_id) = episode_from_redirect(&detail.view.redirect_url) {
        rigili_monitor::monitor!("stream", "bvid {bvid} redirects to episode {ep_id}");
        match client.pgc_season_by_ep(ep_id).await {
            Ok(season) if season.season_id != 0 => {
                let episode = EpisodeId {
                    ep_id,
                    season_id: season.season_id,
                };
                let cid = Some(cid.unwrap_or(detail.view.cid));
                return resolve_episode(
                    client,
                    title,
                    bvid,
                    cid,
                    episode,
                    preferred_quality,
                    preferred_audio,
                )
                .await;
            }
            Ok(_) => {}
            Err(error) => {
                rigili_monitor::monitor!("stream", "episode {ep_id} season lookup failed: {error}");
            }
        }
    }
    let info = detail.view;
    let cid = cid.unwrap_or(info.cid);
    let (streams, player_info) = tokio::join!(
        resolve_streams(
            &client,
            &bvid,
            cid,
            None,
            preferred_quality,
            preferred_audio
        ),
        client.player_info(&bvid, cid, None)
    );
    let streams = streams?.streams;
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
        episode: None,
        episodes: Vec::new(),
        preview: false,
    })
}

async fn resolve_episode(
    client: Arc<Client>,
    title: String,
    bvid: String,
    cid: Option<u64>,
    episode: EpisodeId,
    preferred_quality: Option<u32>,
    preferred_audio: AudioKind,
) -> Result<Resolved, ApiError> {
    let started = Instant::now();
    rigili_monitor::monitor!(
        "stream",
        "resolving episode {}/{} bvid {bvid} (cid {cid:?})",
        episode.season_id,
        episode.ep_id
    );
    // History entries may predate a stored cid; look it up before the parallel requests.
    let known_season = match cid {
        Some(_) => None,
        None => Some(client.pgc_season(episode.season_id).await?),
    };
    let cid = cid
        .or_else(|| {
            known_season
                .as_ref()
                .and_then(|season| episode_of(season, episode))
                .map(|episode| episode.cid)
                .filter(|cid| *cid != 0)
        })
        .ok_or_else(|| ApiError::InvalidResponse("该剧集没有可用的播放分片".into()))?;

    let (streams, player_info, detail, season) = tokio::join!(
        resolve_streams(
            &client,
            &bvid,
            cid,
            Some(episode),
            preferred_quality,
            preferred_audio
        ),
        client.player_info(&bvid, cid, Some(episode)),
        client.video_detail(&bvid),
        async {
            match known_season {
                Some(season) => season,
                None => match client.pgc_season(episode.season_id).await {
                    Ok(season) => season,
                    Err(error) => {
                        rigili_monitor::monitor!(
                            "stream",
                            "pgc season {} failed: {error}",
                            episode.season_id
                        );
                        PgcSeason::default()
                    }
                },
            }
        },
    );
    let ResolvedStreams {
        streams,
        preview,
        duration,
    } = streams?;
    let mut player_info = player_info.unwrap_or_else(|error| {
        rigili_monitor::monitor!("stream", "player info {bvid}/{cid} failed: {error}");
        PlayerInfo::default()
    });
    if player_info.last_play_time <= 0.
        && let Some(last) = season_resume_ms(&season, episode)
    {
        player_info.last_play_time = last;
    }
    let (mut info, related, followers, owner_likes) = match detail {
        Ok(detail) => (
            detail.view,
            detail.related,
            detail.card.follower,
            detail.card.like_num,
        ),
        Err(error) => {
            rigili_monitor::monitor!("stream", "video detail {bvid} failed: {error}");
            (fallback_episode_info(&title, &bvid, cid), Vec::new(), 0, 0)
        }
    };
    normalize_episode_info(&mut info, &title, &bvid, cid, episode, &season, duration);
    let episodes = season.episodes;
    rigili_monitor::monitor!(
        "stream",
        "resolved episode {}/{} cid {cid} in {:?}",
        episode.season_id,
        episode.ep_id,
        started.elapsed()
    );
    Ok(Resolved {
        info,
        related,
        followers,
        owner_likes,
        cid,
        streams,
        player_info,
        episode: Some(episode),
        episodes,
        preview,
    })
}

pub(super) async fn resolve_streams(
    client: &Arc<Client>,
    bvid: &str,
    cid: u64,
    episode: Option<EpisodeId>,
    preferred_quality: Option<u32>,
    preferred_audio: AudioKind,
) -> Result<ResolvedStreams, ApiError> {
    let (dash, accept_quality, accept_description, preview, duration) = match episode {
        Some(episode) => {
            let PgcPlayUrl {
                dash,
                accept_quality,
                accept_description,
                timelength,
                is_preview,
                is_drm,
            } = client.pgc_playurl(episode, cid).await?;
            if is_drm {
                return Err(ApiError::InvalidResponse(
                    "该内容受 DRM 保护，暂不支持播放".into(),
                ));
            }
            let Some(dash) = dash else {
                rigili_monitor::monitor!(
                    "stream",
                    "pgc playurl {}/{} cid {cid} returned no DASH manifest",
                    episode.season_id,
                    episode.ep_id
                );
                return Err(ApiError::InvalidResponse("该剧集没有可用的 DASH 流".into()));
            };
            (
                dash,
                accept_quality,
                accept_description,
                is_preview != 0,
                timelength / 1000,
            )
        }
        None => {
            let PlayUrl {
                dash,
                accept_quality,
                accept_description,
            } = client.playurl(bvid, cid).await?;
            let Some(dash) = dash else {
                rigili_monitor::monitor!(
                    "stream",
                    "playurl {bvid}/{cid} returned no DASH manifest"
                );
                return Err(ApiError::InvalidResponse("该视频没有可用的 DASH 流".into()));
            };
            (dash, accept_quality, accept_description, false, 0)
        }
    };
    rigili_monitor::monitor!(
        "stream",
        "playurl {bvid}/{cid}: {} video tracks, {} audio tracks",
        dash.video.len(),
        dash.audio.len()
    );
    let Some((mut streams, video_urls, audio_urls)) = select_streams(
        &dash,
        &accept_quality,
        &accept_description,
        preferred_quality,
        preferred_audio,
    ) else {
        return Err(ApiError::InvalidResponse("没有可用的视频流".into()));
    };

    let video_primary = streams.video.clone();
    let audio_primary = streams.audio_url().map(str::to_owned);
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
        streams.set_audio_url(url);
    }
    Ok(ResolvedStreams {
        streams,
        preview,
        duration,
    })
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
    dash: &Dash,
    accept_quality: &[u32],
    accept_description: &[String],
    preferred_quality: Option<u32>,
    preferred_audio: AudioKind,
) -> Option<(Streams, Vec<String>, Vec<String>)> {
    let video = preferred_video(dash, preferred_quality)?;
    let mut streams = Streams {
        video: video.base_url.clone(),
        video_id: video.id,
        audio_tracks: audio_tracks(dash),
        audio_kind: None,
        qualities: quality_tracks(dash, accept_quality, accept_description),
    };
    streams.select_audio(preferred_audio);
    let audio = streams.audio().cloned();
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
        "chose audio id={} kind={:?} label={} host={}",
        audio.as_ref().map(|audio| audio.id).unwrap_or_default(),
        audio.as_ref().map(|audio| audio.kind),
        audio
            .as_ref()
            .map(|audio| audio.label.as_str())
            .unwrap_or("none"),
        audio
            .as_ref()
            .map(|audio| rigili_monitor::host(&audio.url))
            .unwrap_or("none")
    );
    let video_urls = candidates(&video.base_url, &video.backup_urls);
    let audio_urls = audio
        .as_ref()
        .map(|audio| candidates(&audio.url, &audio.backup_urls))
        .unwrap_or_default();
    Some((streams, video_urls, audio_urls))
}

fn audio_tracks(dash: &Dash) -> Vec<AudioTrack> {
    let mut tracks = Vec::new();
    if let Some(audio) = dash.best_audio() {
        tracks.push(AudioTrack {
            kind: AudioKind::Regular,
            id: audio.id,
            label: format!("普通 {}", regular_audio_label(audio)),
            url: audio.base_url.clone(),
            backup_urls: audio.backup_urls.clone(),
        });
    }
    if let Some(dolby) = dash.dolby.as_ref().and_then(|dolby| {
        dolby
            .audio
            .iter()
            .filter(|audio| !audio.base_url.is_empty())
            .max_by_key(|audio| audio.bandwidth)
    }) {
        tracks.push(AudioTrack {
            kind: AudioKind::Dolby,
            id: dolby.id,
            label: "杜比全景声".into(),
            url: dolby.base_url.clone(),
            backup_urls: dolby.backup_urls.clone(),
        });
    }
    if let Some(flac) = dash
        .flac
        .as_ref()
        .filter(|flac| flac.display)
        .and_then(|flac| flac.audio.as_ref())
        .filter(|audio| !audio.base_url.is_empty())
    {
        tracks.push(AudioTrack {
            kind: AudioKind::Flac,
            id: flac.id,
            label: "Hi-Res 无损".into(),
            url: flac.base_url.clone(),
            backup_urls: flac.backup_urls.clone(),
        });
    }
    tracks
}

fn regular_audio_label(audio: &DashAudio) -> String {
    match audio.id {
        30216 => "64K".into(),
        30232 => "132K".into(),
        30280 => "192K".into(),
        _ => format!("{}K", audio.bandwidth / 1000),
    }
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

fn quality_tracks(
    dash: &Dash,
    accept_quality: &[u32],
    accept_description: &[String],
) -> Vec<VideoQuality> {
    let ids = if accept_quality.is_empty() {
        let mut ids: Vec<u32> = dash.video.iter().map(|video| video.id).collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids.dedup();
        ids
    } else {
        accept_quality.to_vec()
    };

    ids.into_iter()
        .filter_map(|id| {
            let video = video_track(dash, id)?;
            let description = accept_quality
                .iter()
                .position(|quality| *quality == id)
                .and_then(|index| accept_description.get(index))
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

fn episode_of(season: &PgcSeason, episode: EpisodeId) -> Option<&PgcEpisode> {
    season
        .episodes
        .iter()
        .find(|candidate| candidate.ep_id == episode.ep_id)
}

fn episode_from_redirect(url: &str) -> Option<u64> {
    let tail = url.rsplit('/').next()?.split(['?', '#']).next()?;
    let id = tail.strip_prefix("ep")?;
    id.parse().ok().filter(|id| *id != 0)
}

fn episode_title(season: &PgcSeason, episode: &PgcEpisode) -> Option<String> {
    let season_title = season.title.trim();
    let long_title = episode.long_title.trim();
    match (season_title.is_empty(), long_title.is_empty()) {
        (true, true) => None,
        (true, false) => Some(long_title.to_owned()),
        (false, true) => Some(season_title.to_owned()),
        (false, false) => Some(format!("{season_title} {long_title}")),
    }
}

fn season_resume_ms(season: &PgcSeason, episode: EpisodeId) -> Option<f64> {
    let progress = season.user_status.progress.as_ref()?;
    (progress.last_ep_id == episode.ep_id && progress.last_time_seconds > 0.)
        .then_some(progress.last_time_seconds * 1000.)
}

fn fallback_episode_info(title: &str, bvid: &str, cid: u64) -> VideoInfo {
    VideoInfo {
        bvid: bvid.to_owned(),
        cid,
        title: title.to_owned(),
        ..VideoInfo::default()
    }
}

fn normalize_episode_info(
    info: &mut VideoInfo,
    fallback_title: &str,
    bvid: &str,
    cid: u64,
    episode: EpisodeId,
    season: &PgcSeason,
    duration: u64,
) {
    let candidate = episode_of(season, episode);
    if let Some(candidate) = candidate {
        if info.aid == 0 {
            info.aid = candidate.aid;
        }
        if info.pic.is_empty() {
            info.pic = candidate.cover.clone();
        }
        if info.duration == 0 && candidate.duration > 0 {
            info.duration = candidate.duration / 1000;
        }
        if let Some(title) = episode_title(season, candidate) {
            info.title = title;
        }
    }
    if info.title.trim().is_empty() {
        info.title = fallback_title.to_owned();
    }
    if info.duration == 0 {
        info.duration = duration;
    }
    info.bvid = bvid.to_owned();
    info.cid = cid;
    let part = candidate
        .map(|candidate| candidate.long_title.trim())
        .filter(|part| !part.is_empty())
        .unwrap_or("正片")
        .to_owned();
    let mut pages = episode_pages(season);
    if !pages.iter().any(|page| page.cid == cid) {
        pages = vec![VideoPage {
            cid,
            page: 1,
            part,
            duration: info.duration,
        }];
    }
    info.pages = pages;
}

fn episode_pages(season: &PgcSeason) -> Vec<VideoPage> {
    season
        .episodes
        .iter()
        .filter(|episode| episode.cid != 0 && !episode.bvid.is_empty())
        .enumerate()
        .map(|(index, episode)| VideoPage {
            cid: episode.cid,
            page: (index + 1) as u32,
            part: episode.long_title.trim().to_owned(),
            duration: episode.duration / 1000,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        AudioKind, candidates, episode_from_redirect, episode_pages, normalize_episode_info,
        season_resume_ms, select_streams,
    };
    use rigili_bilibili::{
        Dash, DashAudio, DashDolby, DashFlac, DashVideo, EpisodeId, PgcEpisode, PgcProgress,
        PgcSeason, PgcUserStatus, PlayUrl, VideoInfo,
    };

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
            dolby: None,
            flac: None,
        };
        let playurl = PlayUrl {
            dash: Some(dash),
            accept_quality: vec![80],
            accept_description: vec!["1080P 高清".into()],
        };

        let streams = select_streams(
            playurl.dash.as_ref().expect("dash"),
            &playurl.accept_quality,
            &playurl.accept_description,
            None,
            AudioKind::Regular,
        )
        .expect("streams")
        .0;
        assert_eq!(streams.video, "https://example.invalid/avc.m4s");
        assert_eq!(
            streams.audio_url(),
            Some("https://example.invalid/audio.m4s")
        );
        assert_eq!(streams.video_id, 80);
        assert_eq!(streams.qualities.len(), 1);
        assert_eq!(streams.qualities[0].label, "1080P 高清");
        assert_eq!(streams.qualities[0].short_label, "1080P");
    }

    #[test]
    fn offers_regular_dolby_and_hires_audio() {
        let audio = |id, url: &str, bandwidth| DashAudio {
            id,
            base_url: url.into(),
            backup_urls: Vec::new(),
            bandwidth,
            codecs: String::new(),
        };
        let video = DashVideo {
            id: 80,
            base_url: "video".into(),
            backup_urls: Vec::new(),
            bandwidth: 600_000,
            width: 1920,
            height: 1080,
            codecs: String::new(),
            codecid: 7,
        };
        let dash = Dash {
            video: vec![video],
            audio: vec![audio(30280, "regular", 196_608)],
            dolby: Some(DashDolby {
                audio: vec![audio(30250, "dolby", 1_025_174)],
            }),
            flac: Some(DashFlac {
                display: true,
                audio: Some(audio(30251, "hires", 1_642_465)),
            }),
        };

        let streams = select_streams(&dash, &[80], &[], None, AudioKind::Regular)
            .expect("streams")
            .0;
        let tracks: Vec<_> = streams
            .audio_tracks
            .iter()
            .map(|track| (track.kind, track.id, track.url.as_str()))
            .collect();
        assert_eq!(
            tracks,
            [
                (AudioKind::Regular, 30280, "regular"),
                (AudioKind::Dolby, 30250, "dolby"),
                (AudioKind::Flac, 30251, "hires"),
            ]
        );
        assert_eq!(streams.audio_url(), Some("regular"));

        for (preferred, expected) in [
            (AudioKind::Dolby, Some("dolby")),
            (AudioKind::Flac, Some("hires")),
        ] {
            let streams = select_streams(&dash, &[80], &[], None, preferred)
                .expect("streams")
                .0;
            assert_eq!(streams.audio_url(), expected);
        }

        let hidden = Dash {
            video: vec![DashVideo {
                id: 80,
                base_url: "video".into(),
                backup_urls: Vec::new(),
                bandwidth: 600_000,
                width: 1920,
                height: 1080,
                codecs: String::new(),
                codecid: 7,
            }],
            audio: vec![audio(30280, "regular", 196_608)],
            dolby: None,
            flac: Some(DashFlac {
                display: false,
                audio: Some(audio(30251, "hires", 1_642_465)),
            }),
        };
        let streams = select_streams(&hidden, &[], &[], None, AudioKind::Flac)
            .expect("streams")
            .0;
        assert_eq!(streams.audio_tracks.len(), 1);
        assert_eq!(streams.audio_url(), Some("regular"));
    }

    #[test]
    fn falls_back_between_audio_tiers_like_video_quality() {
        let audio = |id, url: &str, bandwidth| DashAudio {
            id,
            base_url: url.into(),
            backup_urls: Vec::new(),
            bandwidth,
            codecs: String::new(),
        };
        let video = || DashVideo {
            id: 80,
            base_url: "video".into(),
            backup_urls: Vec::new(),
            bandwidth: 600_000,
            width: 1920,
            height: 1080,
            codecs: String::new(),
            codecid: 7,
        };
        let dash = |regular: bool, dolby: bool, flac: bool| Dash {
            video: vec![video()],
            audio: if regular {
                vec![audio(30280, "regular", 196_608)]
            } else {
                Vec::new()
            },
            dolby: dolby.then(|| DashDolby {
                audio: vec![audio(30250, "dolby", 1_025_174)],
            }),
            flac: flac.then(|| DashFlac {
                display: true,
                audio: Some(audio(30251, "hires", 1_642_465)),
            }),
        };
        let chosen = |regular, dolby, flac, preferred| {
            select_streams(&dash(regular, dolby, flac), &[80], &[], None, preferred)
                .expect("streams")
                .0
                .audio_kind
        };

        assert_eq!(
            chosen(true, true, false, AudioKind::Flac),
            Some(AudioKind::Dolby)
        );
        assert_eq!(
            chosen(true, false, true, AudioKind::Dolby),
            Some(AudioKind::Regular)
        );
        assert_eq!(
            chosen(false, true, false, AudioKind::Regular),
            Some(AudioKind::Dolby)
        );
        assert_eq!(
            chosen(false, false, true, AudioKind::Regular),
            Some(AudioKind::Flac)
        );
        assert_eq!(
            chosen(false, true, true, AudioKind::Regular),
            Some(AudioKind::Dolby)
        );
        assert_eq!(chosen(false, false, false, AudioKind::Flac), None);
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
                dolby: None,
                flac: None,
            }),
            accept_quality: vec![80, 64],
            accept_description: Vec::new(),
        };

        let streams = select_streams(
            playurl.dash.as_ref().expect("dash"),
            &playurl.accept_quality,
            &playurl.accept_description,
            None,
            AudioKind::Regular,
        )
        .expect("streams")
        .0;
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
        let dash = Dash {
            video: vec![video(80, "1080p", 600_000), video(64, "720p", 300_000)],
            audio: Vec::new(),
            dolby: None,
            flac: None,
        };
        let accept = [80, 64];

        let exact = select_streams(&dash, &accept, &[], Some(80), AudioKind::Regular)
            .expect("streams")
            .0;
        assert_eq!(exact.video, "1080p");
        assert_eq!(exact.video_id, 80);

        let below = select_streams(&dash, &accept, &[], Some(64), AudioKind::Regular)
            .expect("streams")
            .0;
        assert_eq!(below.video, "720p");
        assert_eq!(below.video_id, 64);

        let unavailable = select_streams(&dash, &accept, &[], Some(32), AudioKind::Regular)
            .expect("streams")
            .0;
        assert_eq!(unavailable.video, "720p");
        assert_eq!(unavailable.video_id, 64);

        let above = select_streams(&dash, &accept, &[], Some(127), AudioKind::Regular)
            .expect("streams")
            .0;
        assert_eq!(above.video, "1080p");
        assert_eq!(above.video_id, 80);

        let only_high = Dash {
            video: vec![video(80, "1080p", 600_000)],
            audio: Vec::new(),
            dolby: None,
            flac: None,
        };
        let lowest = select_streams(&only_high, &[80], &[], Some(32), AudioKind::Regular)
            .expect("streams")
            .0;
        assert_eq!(lowest.video_id, 80);
    }

    #[test]
    fn prefers_the_episode_title_and_fills_a_single_page() {
        let season = PgcSeason {
            season_id: 1,
            title: "鬼灭之刃".into(),
            episodes: vec![PgcEpisode {
                aid: 42,
                bvid: "BV15i421v7FQ".into(),
                cid: 7,
                ep_id: 3,
                long_title: "第一话".into(),
                cover: "cover".into(),
                duration: 1_500_000,
            }],
            ..PgcSeason::default()
        };
        let episode = EpisodeId {
            ep_id: 3,
            season_id: 1,
        };

        let mut info = VideoInfo::default();
        normalize_episode_info(&mut info, "", "BV15i421v7FQ", 7, episode, &season, 1_500);

        assert_eq!(info.title, "鬼灭之刃 第一话");
        assert_eq!(info.aid, 42);
        assert_eq!(info.pic, "cover");
        assert_eq!((info.bvid.as_str(), info.cid), ("BV15i421v7FQ", 7));
        assert_eq!(info.duration, 1_500);
        assert_eq!(info.pages.len(), 1);
        assert_eq!(info.pages[0].part, "第一话");
        assert_eq!(info.pages[0].duration, 1_500);
    }

    #[test]
    fn keeps_the_request_title_without_season_metadata() {
        let season = PgcSeason::default();
        let episode = EpisodeId {
            ep_id: 3,
            season_id: 1,
        };

        let mut info = VideoInfo::default();
        normalize_episode_info(&mut info, "某电影", "BV1", 9, episode, &season, 120);

        assert_eq!(info.title, "某电影");
        assert_eq!(info.duration, 120);
        assert_eq!(info.pages.len(), 1);
        assert_eq!(info.pages[0].part, "正片");
    }

    #[test]
    fn reads_the_episode_from_a_bangumi_redirect() {
        assert_eq!(
            episode_from_redirect("https://www.bilibili.com/bangumi/play/ep331423"),
            Some(331_423)
        );
        assert_eq!(
            episode_from_redirect("https://www.bilibili.com/bangumi/play/ep331423?spm_id_from=333"),
            Some(331_423)
        );
        assert_eq!(
            episode_from_redirect("https://www.bilibili.com/bangumi/play/ss33323"),
            None
        );
        assert_eq!(
            episode_from_redirect("https://www.bilibili.com/video/BV1xx411c7mD"),
            None
        );
        assert_eq!(episode_from_redirect(""), None);
    }

    #[test]
    fn lists_every_playable_episode_as_a_page() {
        let season = PgcSeason {
            episodes: vec![
                PgcEpisode {
                    ep_id: 1,
                    bvid: "BV1".into(),
                    cid: 11,
                    long_title: "第一话".into(),
                    duration: 1_500_000,
                    ..PgcEpisode::default()
                },
                PgcEpisode {
                    ep_id: 2,
                    bvid: "BV2".into(),
                    cid: 0,
                    ..PgcEpisode::default()
                },
                PgcEpisode {
                    ep_id: 3,
                    bvid: String::new(),
                    cid: 33,
                    ..PgcEpisode::default()
                },
                PgcEpisode {
                    ep_id: 4,
                    bvid: "BV4".into(),
                    cid: 44,
                    duration: 2_000_000,
                    ..PgcEpisode::default()
                },
            ],
            ..PgcSeason::default()
        };

        let pages = episode_pages(&season);
        assert_eq!(pages.len(), 2);
        assert_eq!((pages[0].page, pages[0].cid), (1, 11));
        assert_eq!(pages[0].part, "第一话");
        assert_eq!(pages[0].duration, 1_500);
        assert_eq!((pages[1].page, pages[1].cid), (2, 44));
        assert_eq!(pages[1].duration, 2_000);
    }

    #[test]
    fn resumes_only_the_matching_episode_from_season_progress() {
        let season = PgcSeason {
            user_status: PgcUserStatus {
                progress: Some(PgcProgress {
                    last_ep_id: 3,
                    last_time_seconds: 12.5,
                }),
            },
            ..PgcSeason::default()
        };
        let matching = EpisodeId {
            ep_id: 3,
            season_id: 1,
        };
        let other = EpisodeId {
            ep_id: 4,
            season_id: 1,
        };

        assert_eq!(season_resume_ms(&season, matching), Some(12_500.));
        assert_eq!(season_resume_ms(&season, other), None);
        assert_eq!(season_resume_ms(&PgcSeason::default(), matching), None);
    }
}
