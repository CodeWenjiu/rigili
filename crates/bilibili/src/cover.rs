/// Builds a 16:9 thumbnail URL for a Bilibili cover image.
pub fn thumbnail(url: &str, width: u32) -> String {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let base = rest.split('@').next().unwrap_or(rest);
    let height = width * 9 / 16;
    format!("https://{base}@{width}w_{height}h_1c.webp")
}

#[cfg(test)]
mod tests {
    use super::thumbnail;

    #[test]
    fn upgrades_and_resizes_cover_urls() {
        assert_eq!(
            thumbnail("http://i0.hdslb.com/cover.jpg", 480),
            "https://i0.hdslb.com/cover.jpg@480w_270h_1c.webp"
        );
        assert_eq!(
            thumbnail("https://i0.hdslb.com/cover.jpg@320w_200h_1c.webp", 720),
            "https://i0.hdslb.com/cover.jpg@720w_405h_1c.webp"
        );
    }
}
