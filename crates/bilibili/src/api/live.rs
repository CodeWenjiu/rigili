use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use crate::http::USER_AGENT;
use crate::{ApiError, Client, DanmakuMode, LiveDanmaku, LivePlayUrl, LiveRoom};

const LIST_URL: &str = "https://api.live.bilibili.com/room/v1/Area/getRoomList";
const PLAY_URL: &str = "https://api.live.bilibili.com/room/v1/Room/playUrl";
const ROOM_INFO_URL: &str = "https://api.live.bilibili.com/room/v1/Room/get_info";
const DANMAKU_INFO_URL: &str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

const AUTH: u32 = 7;
const AUTH_REPLY: u32 = 8;
const HEARTBEAT: u32 = 2;
const MESSAGE: u32 = 5;

impl Client {
    pub async fn live_list(&self, page: u32, page_size: u32) -> Result<Vec<LiveRoom>, ApiError> {
        self.get_json(
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

    pub async fn live_play_url(&self, room_id: u64) -> Result<String, ApiError> {
        let playurl: LivePlayUrl = self
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

    /// Keeps a live room's real-time comments flowing into the sender until
    /// the receiver is dropped or the client is torn down.
    pub async fn watch_live_danmaku(
        self: Arc<Self>,
        room_id: u64,
        sender: UnboundedSender<Vec<LiveDanmaku>>,
    ) -> Result<(), ApiError> {
        loop {
            if sender.is_closed() {
                return Ok(());
            }
            match stream_live_danmaku(&self, room_id, &sender).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    rigili_monitor::monitor!("live", "danmaku {room_id}: {error}, reconnecting")
                }
            }
            if sender.is_closed() {
                return Ok(());
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    }
}

#[derive(Deserialize)]
struct LiveRoomData {
    #[serde(default)]
    room_id: u64,
}

#[derive(Deserialize)]
struct LiveDanmakuData {
    #[serde(default)]
    token: String,
    #[serde(default)]
    host_list: Vec<LiveDanmakuHost>,
}

#[derive(Deserialize)]
struct LiveDanmakuHost {
    #[serde(default)]
    host: String,
    #[serde(default)]
    wss_port: u16,
    #[serde(default)]
    ws_port: u16,
}

async fn stream_live_danmaku(
    client: &Arc<Client>,
    room_id: u64,
    sender: &UnboundedSender<Vec<LiveDanmaku>>,
) -> Result<(), ApiError> {
    let real_id = match client
        .get_json::<LiveRoomData>(ROOM_INFO_URL, &[("room_id", room_id.to_string())])
        .await
    {
        Ok(room) if room.room_id != 0 => room.room_id,
        Ok(_) => room_id,
        Err(error) => {
            rigili_monitor::monitor!("live", "room {room_id} info failed: {error}");
            room_id
        }
    };
    let info: LiveDanmakuData = client
        .get_wbi_json(
            DANMAKU_INFO_URL,
            &[("type", "0".into()), ("id", real_id.to_string())],
        )
        .await?;
    let cookies = client.cookie_values();
    let uid = cookies
        .get("DedeUserID")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let buvid = cookies
        .get("buvid3")
        .or_else(|| cookies.get("buvid4"))
        .cloned()
        .unwrap_or_default();

    let mut last_error = None;
    for host in &info.host_list {
        for url in host_urls(host) {
            match connect(&url, &cookies, real_id).await {
                Ok(mut socket) => {
                    return run_socket(&mut socket, &info.token, uid, &buvid, real_id, sender)
                        .await;
                }
                Err(error) => {
                    rigili_monitor::monitor!("live", "danmaku host {url} failed: {error}");
                    last_error = Some(error);
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| ApiError::Live("没有可用的弹幕服务器".into())))
}

fn host_urls(host: &LiveDanmakuHost) -> Vec<String> {
    let mut urls = Vec::new();
    if !host.host.is_empty() {
        if host.wss_port > 0 {
            urls.push(format!("wss://{}:{}/sub", host.host, host.wss_port));
        }
        if host.ws_port > 0 {
            urls.push(format!("ws://{}:{}/sub", host.host, host.ws_port));
        }
    }
    urls
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(
    url: &str,
    cookies: &BTreeMap<String, String>,
    room_id: u64,
) -> Result<Socket, ApiError> {
    let mut request = url
        .into_client_request()
        .map_err(|error| ApiError::Live(error.to_string()))?;
    let headers = request.headers_mut();
    headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    headers.insert(
        "Origin",
        HeaderValue::from_static("https://live.bilibili.com"),
    );
    if let Ok(referer) = HeaderValue::from_str(&format!("https://live.bilibili.com/{room_id}")) {
        headers.insert("Referer", referer);
    }
    let cookie = cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    if !cookie.is_empty()
        && let Ok(value) = HeaderValue::from_str(&cookie)
    {
        headers.insert("Cookie", value);
    }

    let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| ApiError::Live("连接超时".into()))?
        .map_err(|error| ApiError::Live(error.to_string()))?;
    Ok(socket)
}

async fn run_socket(
    socket: &mut Socket,
    token: &str,
    uid: u64,
    buvid: &str,
    room_id: u64,
    sender: &UnboundedSender<Vec<LiveDanmaku>>,
) -> Result<(), ApiError> {
    let auth = serde_json::json!({
        "uid": uid,
        "roomid": room_id,
        "protover": 2,
        "buvid": buvid,
        "platform": "web",
        "type": 2,
        "key": token,
    });
    let packet = encode_packet(AUTH, auth.to_string().as_bytes());
    socket
        .send(Message::Binary(packet.into()))
        .await
        .map_err(|error| ApiError::Live(error.to_string()))?;

    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sender.is_closed() {
                    return Ok(());
                }
                let packet = encode_packet(HEARTBEAT, b"");
                socket
                    .send(Message::Binary(packet.into()))
                    .await
                    .map_err(|error| ApiError::Live(error.to_string()))?;
            }
            message = socket.next() => {
                let message = message
                    .ok_or_else(|| ApiError::Live("弹幕连接已关闭".into()))?
                    .map_err(|error| ApiError::Live(error.to_string()))?;
                match message {
                    Message::Binary(bytes) => {
                        let mut items = Vec::new();
                        decode_packets(&bytes, &mut items);
                        if !items.is_empty() && sender.send(items).is_err() {
                            return Ok(());
                        }
                    }
                    Message::Text(text) => {
                        let mut items = Vec::new();
                        parse_danmaku_json(text.as_str().as_bytes(), &mut items);
                        if !items.is_empty() && sender.send(items).is_err() {
                            return Ok(());
                        }
                    }
                    Message::Ping(payload) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Message::Close(_) => return Err(ApiError::Live("弹幕连接已关闭".into())),
                    _ => {}
                }
            }
        }
    }
}

fn encode_packet(operation: u32, body: &[u8]) -> Vec<u8> {
    let length = 16 + body.len();
    let mut packet = Vec::with_capacity(length);
    packet.extend_from_slice(&(length as u32).to_be_bytes());
    packet.extend_from_slice(&16u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&operation.to_be_bytes());
    packet.extend_from_slice(&1u32.to_be_bytes());
    packet.extend_from_slice(body);
    packet
}

fn decode_packets(bytes: &[u8], out: &mut Vec<LiveDanmaku>) {
    let mut offset = 0;
    while offset + 16 <= bytes.len() {
        let Some(total) = bytes
            .get(offset..offset + 4)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_be_bytes)
        else {
            break;
        };
        let total = total as usize;
        let Some(header) = bytes
            .get(offset + 4..offset + 6)
            .and_then(|slice| slice.try_into().ok())
            .map(u16::from_be_bytes)
        else {
            break;
        };
        let header = header as usize;
        let Some(version) = bytes
            .get(offset + 6..offset + 8)
            .and_then(|slice| slice.try_into().ok())
            .map(u16::from_be_bytes)
        else {
            break;
        };
        let Some(operation) = bytes
            .get(offset + 8..offset + 12)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_be_bytes)
        else {
            break;
        };
        if header < 16 || total < header || offset + total > bytes.len() {
            break;
        }
        let body = &bytes[offset + header..offset + total];
        match operation {
            MESSAGE => match version {
                2 => {
                    if let Ok(data) = inflate(body) {
                        decode_packets(&data, out);
                    }
                }
                3 => {
                    if let Ok(data) = brotli_decode(body) {
                        decode_packets(&data, out);
                    }
                }
                _ => parse_danmaku_json(body, out),
            },
            AUTH_REPLY => {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
                    let code = value.get("code").and_then(|code| code.as_i64());
                    if code.unwrap_or(0) != 0 {
                        rigili_monitor::monitor!("live", "danmaku auth rejected: {code:?}");
                    }
                }
            }
            _ => {}
        }
        offset += total;
    }
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

fn brotli_decode(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::new();
    brotli::Decompressor::new(bytes, 4096).read_to_end(&mut out)?;
    Ok(out)
}

fn parse_danmaku_json(body: &[u8], out: &mut Vec<LiveDanmaku>) {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    let Some(cmd) = value.get("cmd").and_then(|cmd| cmd.as_str()) else {
        return;
    };
    if !cmd.starts_with("DANMU_MSG") {
        return;
    }
    let Some(info) = value.get("info").and_then(|info| info.as_array()) else {
        return;
    };
    let Some(attribute) = info.first().and_then(|item| item.as_array()) else {
        return;
    };
    let Some(text) = info.get(1).and_then(|text| text.as_str()) else {
        return;
    };
    let text = text.replace(['\n', '\r'], " ");
    if text.trim().is_empty() {
        return;
    }
    let mode = attribute.get(1).and_then(|mode| mode.as_u64()).unwrap_or(1) as u8;
    let font_size = attribute
        .get(2)
        .and_then(|size| size.as_u64())
        .unwrap_or(25)
        .clamp(12, 64) as u8;
    let color = attribute
        .get(3)
        .and_then(|color| color.as_u64())
        .unwrap_or(0x00ff_ffff) as u32
        & 0x00ff_ffff;
    out.push(LiveDanmaku {
        text,
        mode: DanmakuMode::from(mode),
        font_size,
        color,
    });
}

#[cfg(test)]
mod tests {
    use super::{LiveDanmakuHost, decode_packets, encode_packet, host_urls, parse_danmaku_json};
    use crate::DanmakuMode;

    fn danmaku_json(mode: u64, size: u64, color: u64, text: &str) -> String {
        format!(
            r#"{{"cmd":"DANMU_MSG","info":[[0,{mode},{size},{color},0],"{}",[1,"name"],[]]}}"#,
            text
        )
    }

    #[test]
    fn parses_live_danmaku_and_ignores_other_messages() {
        let mut items = Vec::new();
        parse_danmaku_json(
            danmaku_json(1, 25, 0x123456, "hello").as_bytes(),
            &mut items,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "hello");
        assert_eq!(items[0].mode, DanmakuMode::Scroll);
        assert_eq!(items[0].font_size, 25);
        assert_eq!(items[0].color, 0x123456);

        let mut items = Vec::new();
        parse_danmaku_json(r#"{"cmd":"SEND_GIFT","info":[]}"#.as_bytes(), &mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn decodes_plain_and_compressed_packets() {
        let json = danmaku_json(5, 25, 0xffffff, "top");
        let mut plain = encode_packet(5, json.as_bytes());
        let mut items = Vec::new();
        decode_packets(&plain, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].mode, DanmakuMode::Top);

        let compressed = {
            use std::io::Write;
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&plain).expect("compress");
            encoder.finish().expect("finish")
        };
        plain = encode_packet(5, &compressed);
        plain[6..8].copy_from_slice(&2u16.to_be_bytes());
        let mut items = Vec::new();
        decode_packets(&plain, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "top");
    }

    #[test]
    fn chooses_wss_before_plain_ws() {
        let host = LiveDanmakuHost {
            host: "example.invalid".into(),
            wss_port: 443,
            ws_port: 2244,
        };
        assert_eq!(
            host_urls(&host),
            [
                "wss://example.invalid:443/sub",
                "ws://example.invalid:2244/sub"
            ]
        );
    }
}
