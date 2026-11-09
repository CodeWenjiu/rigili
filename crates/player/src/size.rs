#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoSize {
    pub width: u32,
    pub height: u32,
}

impl VideoSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}
