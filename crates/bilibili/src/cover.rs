/// Builds a 16:9 thumbnail URL for a Bilibili cover image.
pub fn thumbnail(url: &str, width: u32) -> String {
    let base = image_base(url);
    let height = width * 9 / 16;
    format!("{base}@{width}w_{height}h_1c.webp")
}

/// Builds a width-only URL for wide images such as space banners.
pub fn banner(url: &str, width: u32) -> String {
    let base = image_base(url);
    format!("{base}@{width}w_1c.webp")
}

fn image_base(url: &str) -> String {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("//"))
        .unwrap_or(url);
    let rest = rest.split('@').next().unwrap_or(rest);
    if rest.starts_with("bfs/") {
        format!("https://i0.hdslb.com/{rest}")
    } else {
        format!("https://{rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::{banner, thumbnail};

    #[test]
    fn builds_thumbnail_and_banner_urls() {
        assert_eq!(
            thumbnail("http://i0.hdslb.com/cover.jpg", 480),
            "https://i0.hdslb.com/cover.jpg@480w_270h_1c.webp"
        );
        assert_eq!(
            thumbnail("https://i0.hdslb.com/cover.jpg@320w_200h_1c.webp", 720),
            "https://i0.hdslb.com/cover.jpg@720w_405h_1c.webp"
        );

        assert_eq!(
            banner("http://i0.hdslb.com/banner.jpg", 1280),
            "https://i0.hdslb.com/banner.jpg@1280w_1c.webp"
        );
        assert_eq!(
            banner("bfs/space/banner.png", 1280),
            "https://i0.hdslb.com/bfs/space/banner.png@1280w_1c.webp"
        );
        assert_eq!(
            banner("//i1.hdslb.com/bfs/space/banner.png", 1280),
            "https://i1.hdslb.com/bfs/space/banner.png@1280w_1c.webp"
        );
    }
}
