use std::cmp::Ordering;

use serde::Deserialize;

use super::null_or_default;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Popular {
    #[serde(default)]
    pub list: Vec<PopularVideo>,
    #[serde(default)]
    pub no_more: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PopularVideo {
    pub aid: u64,
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    pub pic: String,
    pub title: String,
    pub duration: u64,
    pub pubdate: i64,
    pub owner: Owner,
    #[serde(default)]
    pub stat: Stat,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub bvid: String,
    pub aid: u64,
    pub cid: u64,
    pub pic: String,
    pub title: String,
    #[serde(default)]
    pub desc: String,
    pub duration: u64,
    pub pubdate: i64,
    pub owner: Owner,
    #[serde(default)]
    pub stat: Stat,
    #[serde(default)]
    pub rights: VideoRights,
    #[serde(default)]
    pub pages: Vec<VideoPage>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VideoRights {
    #[serde(default)]
    pub no_reprint: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VideoDetail {
    #[serde(default, rename = "View")]
    pub view: VideoInfo,
    #[serde(default, rename = "Card")]
    pub card: VideoCard,
    #[serde(default, rename = "Related")]
    pub related: Vec<RecommendVideo>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VideoCard {
    #[serde(default)]
    pub follower: u64,
    #[serde(default)]
    pub like_num: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct VideoPage {
    pub cid: u64,
    pub page: u32,
    #[serde(default)]
    pub part: String,
    pub duration: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PlayerInfo {
    #[serde(default)]
    pub view_points: Vec<ViewPoint>,
    /// Milliseconds into the requested part where the viewer stopped last time.
    #[serde(default)]
    pub last_play_time: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ViewPoint {
    #[serde(default, rename = "type")]
    pub kind: i32,
    #[serde(default)]
    pub from: Option<f64>,
    #[serde(default)]
    pub to: Option<f64>,
    #[serde(default)]
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

impl PlayerInfo {
    pub fn chapters(&self) -> Vec<Chapter> {
        let mut points = self
            .view_points
            .iter()
            .filter(|point| point.kind == 2)
            .collect::<Vec<_>>();
        points.sort_by(|a, b| a.from.unwrap_or(0.0).total_cmp(&b.from.unwrap_or(0.0)));
        points
            .into_iter()
            .filter_map(|point| {
                let start = point.from.unwrap_or(0.0);
                let end = point.to.unwrap_or(f64::INFINITY);
                (end > start).then(|| Chapter {
                    start,
                    end,
                    title: point.content.clone(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct VideoShot {
    #[serde(default, rename = "img_x_len")]
    pub columns: u32,
    #[serde(default, rename = "img_y_len")]
    pub rows: u32,
    #[serde(default, rename = "img_x_size")]
    pub frame_width: u32,
    #[serde(default, rename = "img_y_size")]
    pub frame_height: u32,
    #[serde(default)]
    pub image: Vec<String>,
    #[serde(default)]
    pub index: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoShotFrame<'a> {
    pub image: &'a str,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub sprite_width: u32,
    pub sprite_height: u32,
}

impl VideoShot {
    pub fn frame_at(&self, time: f64) -> Option<VideoShotFrame<'_>> {
        let columns = self.columns as usize;
        let per_sheet = columns.checked_mul(self.rows as usize)?;
        let capacity = self.image.len().checked_mul(per_sheet)?;
        if per_sheet == 0 || capacity == 0 || self.index.is_empty() {
            return None;
        }
        let frame = match self
            .index
            .binary_search_by(|value| value.partial_cmp(&time).unwrap_or(Ordering::Equal))
        {
            Ok(index) => index,
            Err(0) => 0,
            Err(index) => index - 1,
        }
        .min(capacity - 1);
        let image = self.image.get(frame / per_sheet)?;
        let slot = frame % per_sheet;

        Some(VideoShotFrame {
            image,
            x: (slot % columns) as u32 * self.frame_width,
            y: (slot / columns) as u32 * self.frame_height,
            width: self.frame_width,
            height: self.frame_height,
            sprite_width: self.columns * self.frame_width,
            sprite_height: self.rows * self.frame_height,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PlayUrl {
    #[serde(default)]
    pub dash: Option<Dash>,
    #[serde(default)]
    pub accept_quality: Vec<u32>,
    #[serde(default)]
    pub accept_description: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dash {
    #[serde(default)]
    pub video: Vec<DashVideo>,
    #[serde(default)]
    pub audio: Vec<DashAudio>,
}

impl Dash {
    pub fn best_video(&self) -> Option<&DashVideo> {
        self.usable_videos()
            .filter(|video| video.codecid == 7)
            .max_by_key(|video| video.bandwidth)
            .or_else(|| self.usable_videos().max_by_key(|video| video.bandwidth))
    }

    pub fn best_audio(&self) -> Option<&DashAudio> {
        self.audio
            .iter()
            .filter(|audio| !audio.base_url.is_empty())
            .max_by_key(|audio| audio.bandwidth)
    }

    pub fn preview_video(&self) -> Option<&DashVideo> {
        self.usable_videos()
            .min_by_key(|video| (video.height, video.codecid != 7, video.bandwidth))
    }

    fn usable_videos(&self) -> impl Iterator<Item = &DashVideo> {
        self.video
            .iter()
            .filter(|video| !video.base_url.is_empty() && video.width > 0 && video.height > 0)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawDashVideo")]
pub struct DashVideo {
    pub id: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub bandwidth: u64,
    pub width: u32,
    pub height: u32,
    pub codecs: String,
    pub codecid: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawDashAudio")]
pub struct DashAudio {
    pub id: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub bandwidth: u64,
    pub codecs: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDashVideo {
    id: u32,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default, rename = "base_url")]
    base_url_snake: Option<String>,
    #[serde(default)]
    backup_url: Option<Vec<String>>,
    #[serde(default, rename = "backup_url")]
    backup_url_snake: Option<Vec<String>>,
    bandwidth: u64,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    codecs: String,
    #[serde(default)]
    codecid: u32,
}

impl From<RawDashVideo> for DashVideo {
    fn from(raw: RawDashVideo) -> Self {
        Self {
            id: raw.id,
            base_url: raw.base_url.or(raw.base_url_snake).unwrap_or_default(),
            backup_urls: raw.backup_url.or(raw.backup_url_snake).unwrap_or_default(),
            bandwidth: raw.bandwidth,
            width: raw.width,
            height: raw.height,
            codecs: raw.codecs,
            codecid: raw.codecid,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDashAudio {
    id: u32,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default, rename = "base_url")]
    base_url_snake: Option<String>,
    #[serde(default)]
    backup_url: Option<Vec<String>>,
    #[serde(default, rename = "backup_url")]
    backup_url_snake: Option<Vec<String>>,
    bandwidth: u64,
    #[serde(default)]
    codecs: String,
}

impl From<RawDashAudio> for DashAudio {
    fn from(raw: RawDashAudio) -> Self {
        Self {
            id: raw.id,
            base_url: raw.base_url.or(raw.base_url_snake).unwrap_or_default(),
            backup_urls: raw.backup_url.or(raw.backup_url_snake).unwrap_or_default(),
            bandwidth: raw.bandwidth,
            codecs: raw.codecs,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Owner {
    pub mid: u64,
    pub name: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Stat {
    #[serde(default)]
    pub view: u64,
    #[serde(default)]
    pub danmaku: u64,
    #[serde(default)]
    pub like: u64,
    #[serde(default)]
    pub coin: u64,
    #[serde(default)]
    pub favorite: u64,
    #[serde(default)]
    pub share: u64,
    #[serde(default)]
    pub reply: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ArchiveRelation {
    #[serde(default)]
    pub attention: bool,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub like: bool,
    #[serde(default)]
    pub coin: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Recommend {
    #[serde(default)]
    pub item: Vec<RecommendVideo>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RecommendVideo {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub pubdate: i64,
    #[serde(default, deserialize_with = "null_or_default")]
    pub owner: Owner,
    #[serde(default, deserialize_with = "null_or_default")]
    pub stat: Stat,
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveRelation, Dash, DashAudio, DashVideo, PlayerInfo, RecommendVideo, VideoDetail,
        VideoShot,
    };

    fn video(codecid: u32, bandwidth: u64) -> DashVideo {
        DashVideo {
            id: 32,
            base_url: format!("https://example.invalid/{codecid}.m4s"),
            backup_urls: Vec::new(),
            bandwidth,
            width: 852,
            height: 480,
            codecs: String::new(),
            codecid,
        }
    }

    fn audio(bandwidth: u64) -> DashAudio {
        DashAudio {
            id: 30280,
            base_url: "https://example.invalid/audio.m4s".into(),
            backup_urls: Vec::new(),
            bandwidth,
            codecs: String::new(),
        }
    }

    #[test]
    fn selects_best_video_and_audio_tracks() {
        let dash = Dash {
            video: vec![video(13, 900_000), video(7, 600_000)],
            audio: vec![audio(65_536), audio(196_608)],
        };
        assert_eq!(dash.best_video().map(|video| video.codecid), Some(7));
        assert_eq!(
            dash.best_audio().map(|audio| audio.bandwidth),
            Some(196_608)
        );

        let no_avc = Dash {
            video: vec![video(12, 600_000), video(13, 900_000)],
            audio: Vec::new(),
        };
        assert_eq!(no_avc.best_video().map(|video| video.codecid), Some(13));

        let unavailable = Dash {
            video: vec![DashVideo {
                bandwidth: 9_000_000,
                base_url: String::new(),
                ..video(7, 1)
            }],
            audio: vec![DashAudio {
                bandwidth: 9_000_000,
                base_url: String::new(),
                ..audio(1)
            }],
        };
        assert!(unavailable.best_video().is_none());
        assert!(unavailable.best_audio().is_none());
    }

    #[test]
    fn selects_the_smallest_preview_track() {
        let dash = Dash {
            video: vec![
                track(80, 1080, 7, 2_000_000),
                track(32, 480, 7, 600_000),
                track(16, 360, 12, 400_000),
                track(16, 360, 7, 500_000),
            ],
            audio: Vec::new(),
        };
        assert_eq!(
            dash.preview_video().map(|video| video.codecid),
            Some(7),
            "prefer the AVC track at the smallest size"
        );
        assert_eq!(dash.preview_video().map(|video| video.height), Some(360));

        let no_avc = Dash {
            video: vec![track(80, 1080, 13, 2_000_000), track(32, 480, 13, 600_000)],
            audio: Vec::new(),
        };
        assert_eq!(no_avc.preview_video().map(|video| video.id), Some(32));

        let unavailable = Dash {
            video: vec![DashVideo {
                base_url: String::new(),
                ..track(16, 360, 7, 500_000)
            }],
            audio: Vec::new(),
        };
        assert!(unavailable.preview_video().is_none());
    }

    fn track(id: u32, height: u32, codecid: u32, bandwidth: u64) -> DashVideo {
        DashVideo {
            id,
            base_url: format!("https://example.invalid/{id}-{codecid}.m4s"),
            backup_urls: Vec::new(),
            bandwidth,
            width: height * 16 / 9,
            height,
            codecs: String::new(),
            codecid,
        }
    }

    #[test]
    fn parses_backup_stream_urls() {
        let dash: Dash = serde_json::from_str(
            r#"{
                "video": [{
                    "id": 80,
                    "baseUrl": "https://a.invalid/v.m4s",
                    "backupUrl": ["https://b.invalid/v.m4s"],
                    "bandwidth": 1,
                    "width": 1920,
                    "height": 1080,
                    "codecs": "avc1",
                    "codecid": 7
                }],
                "audio": [{
                    "id": 30280,
                    "base_url": "https://a.invalid/a.m4s",
                    "backup_url": ["https://b.invalid/a.m4s"],
                    "bandwidth": 1,
                    "codecs": "mp4a.40.2"
                }]
            }"#,
        )
        .expect("dash with backup urls");

        assert_eq!(dash.video[0].backup_urls, ["https://b.invalid/v.m4s"]);
        assert_eq!(dash.audio[0].backup_urls, ["https://b.invalid/a.m4s"]);
    }

    #[test]
    fn parses_video_detail_with_owner_card_and_related() {
        let detail: VideoDetail = serde_json::from_str(
            r#"{
                "View":{
                    "bvid":"BV1xx","aid":1,"cid":2,"pic":"//i0.hdslb.com/a.jpg",
                    "title":"t","duration":10,"pubdate":0,
                    "owner":{"mid":3,"name":"up","face":""},
                    "stat":{"view":10,"danmaku":1,"like":2,"coin":3,"favorite":4,"share":5,"reply":1931},
                    "rights":{"no_reprint":1}
                },
                "Card":{"follower":100,"like_num":200},
                "Related":[{
                    "aid":4,"bvid":"BV1yy","cid":5,"pic":"//i0.hdslb.com/b.jpg",
                    "title":"r","duration":6,"pubdate":0,
                    "owner":{"mid":6,"name":"up2","face":""},
                    "stat":{"view":7,"danmaku":8}
                }]
            }"#,
        )
        .unwrap();

        assert_eq!((detail.view.aid, detail.view.cid), (1, 2));
        assert_eq!(detail.view.stat.favorite, 4);
        assert_eq!(detail.view.stat.reply, 1931);
        assert_eq!(detail.view.rights.no_reprint, 1);
        assert_eq!((detail.card.follower, detail.card.like_num), (100, 200));
        assert_eq!(detail.related.len(), 1);
        assert_eq!(detail.related[0].cid, 5);
    }

    #[test]
    fn extracts_chapters_from_view_points() {
        let info: PlayerInfo = serde_json::from_str(
            r#"{"view_points":[
                {"type":1,"from":10,"to":20,"content":"看点"},
                {"type":2,"from":null,"to":30,"content":"开场"},
                {"type":2,"from":30,"to":60,"content":"正片"},
                {"type":2,"from":60,"to":null,"content":"尾声"},
                {"type":2,"from":60,"to":60,"content":"空"}
            ]}"#,
        )
        .unwrap();

        let chapters = info.chapters();
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].title, "开场");
        assert_eq!((chapters[0].start, chapters[0].end), (0., 30.));
        assert_eq!((chapters[1].start, chapters[1].end), (30., 60.));
        assert_eq!((chapters[2].start, chapters[2].end), (60., f64::INFINITY));
    }

    #[test]
    fn maps_time_to_storyboard_frame() {
        let shot = VideoShot {
            columns: 2,
            rows: 2,
            frame_width: 160,
            frame_height: 90,
            image: vec!["//i0.hdslb.com/a.jpg".into(), "//i0.hdslb.com/b.jpg".into()],
            index: vec![0., 10., 20., 30., 40.],
        };

        let frame = shot.frame_at(-5.).unwrap();
        assert_eq!(frame.image, "//i0.hdslb.com/a.jpg");
        assert_eq!((frame.x, frame.y), (0, 0));

        let frame = shot.frame_at(25.).unwrap();
        assert_eq!((frame.x, frame.y), (0, 90));

        let frame = shot.frame_at(45.).unwrap();
        assert_eq!(frame.image, "//i0.hdslb.com/b.jpg");
        assert_eq!((frame.x, frame.y), (0, 0));
        assert_eq!((frame.sprite_width, frame.sprite_height), (320, 180));

        assert!(VideoShot::default().frame_at(0.).is_none());
    }

    #[test]
    fn parses_video_payloads() {
        let related: Vec<RecommendVideo> = serde_json::from_str(
            r#"[{
                "aid":1,"bvid":"BV1xx","cid":2,"pic":"//i0.hdslb.com/a.jpg",
                "title":"related","duration":32,"pubdate":0,
                "owner":{"mid":3,"name":"up","face":""},
                "stat":{"view":100,"danmaku":2,"like":1}
            }]"#,
        )
        .unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].cid, 2);
        assert_eq!(related[0].owner.name, "up");
        assert_eq!(related[0].stat.view, 100);

        let relation: ArchiveRelation =
            serde_json::from_str(r#"{"attention":false,"favorite":true,"like":true,"coin":2}"#)
                .unwrap();
        assert!(relation.like);
        assert!(relation.favorite);
        assert!(!relation.attention);
        assert_eq!(relation.coin, 2);

        let shot: VideoShot = serde_json::from_str(
            r#"{
                "pvdata": "//i0.hdslb.com/bfs/videoshot/1.bin",
                "img_x_len": 10,
                "img_y_len": 10,
                "img_x_size": 160,
                "img_y_size": 90,
                "image": ["//i0.hdslb.com/bfs/videoshot/1.jpg"],
                "index": [0, 0, 5, 11]
            }"#,
        )
        .unwrap();
        assert_eq!((shot.columns, shot.rows), (10, 10));
        assert_eq!((shot.frame_width, shot.frame_height), (160, 90));
        assert_eq!(shot.index, [0., 0., 5., 11.]);
        assert_eq!(shot.frame_at(6.).unwrap().x, 160 * 2);
    }
}
