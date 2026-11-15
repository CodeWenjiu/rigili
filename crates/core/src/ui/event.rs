#[derive(Clone)]
pub struct OpenVideo {
    pub bvid: String,
    pub cid: Option<u64>,
    pub title: String,
}

#[derive(Clone)]
pub struct OpenLive {
    pub room_id: u64,
    pub title: String,
}

#[derive(Clone)]
pub struct OpenSpace {
    pub mid: u64,
    pub name: String,
}
