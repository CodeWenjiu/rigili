use crate::{ApiError, Client, LivePlayUrl, LiveRoom};

const LIST_URL: &str = "https://api.live.bilibili.com/room/v1/Area/getRoomList";
const PLAY_URL: &str = "https://api.live.bilibili.com/room/v1/Room/playUrl";

pub async fn list(client: &Client, page: u32, page_size: u32) -> Result<Vec<LiveRoom>, ApiError> {
    client
        .get_json(
            LIST_URL,
            &[
                ("parent_area_id", "0".into()),
                ("area_id", "0".into()),
                ("sort_type", "online".into()),
                ("page", page.max(1).to_string()),
                ("page_size", page_size.clamp(1, 30).to_string()),
            ],
        )
        .await
}

pub async fn play_url(client: &Client, room_id: u64) -> Result<String, ApiError> {
    let playurl: LivePlayUrl = client
        .get_json(
            PLAY_URL,
            &[
                ("cid", room_id.to_string()),
                ("platform", "web".into()),
                ("qn", "10000".into()),
            ],
        )
        .await?;

    playurl
        .durl
        .into_iter()
        .map(|stream| stream.url)
        .find(|url| !url.is_empty())
        .ok_or_else(|| ApiError::InvalidResponse("该直播间没有可用的播放地址".into()))
}

#[cfg(test)]
mod tests {
    use crate::{LivePlayUrl, LiveRoom};

    const LIVE_FIXTURE: &str = include_str!("../../fixtures/live.json");
    const LIVE_PLAY_FIXTURE: &str = include_str!("../../fixtures/live_play.json");

    #[derive(serde::Deserialize)]
    struct ListEnvelope {
        data: Vec<LiveRoom>,
    }

    #[derive(serde::Deserialize)]
    struct PlayEnvelope {
        data: LivePlayUrl,
    }

    #[test]
    fn parses_live_list_fixture() {
        let envelope: ListEnvelope = serde_json::from_str(LIVE_FIXTURE).expect("live fixture");
        let rooms = envelope.data;

        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].roomid, 7_734_200);
        assert_eq!(rooms[0].uname, "测试主播");
        assert_eq!(rooms[0].online, 895_359);
        assert_eq!(rooms[0].area_name, "游戏赛事");
    }

    #[test]
    fn parses_live_play_fixture() {
        let envelope: PlayEnvelope =
            serde_json::from_str(LIVE_PLAY_FIXTURE).expect("live play fixture");
        assert_eq!(
            envelope.data.durl[0].url,
            "https://example.invalid/live.flv"
        );
    }
}
