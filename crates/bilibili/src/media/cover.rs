use std::fmt::Write;

/// Normalizes a protocol-relative Bilibili URL to an absolute https URL.
pub fn normalize_url(url: &str) -> String {
    match url.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

/// Builds a 16:9 thumbnail URL for a Bilibili cover image.
pub fn thumbnail(url: &str, width: u32) -> String {
    let (prefix, rest) = image_parts(url);
    let height = width * 9 / 16;
    let mut out = String::with_capacity(prefix.len() + rest.len() + 24);
    out.push_str(prefix);
    out.push_str(rest);
    let _ = write!(out, "@{width}w_{height}h_1c.webp");
    out
}

/// Builds a square avatar URL for a Bilibili profile image.
pub fn avatar(url: &str, size: u32) -> String {
    square(url, size)
}

/// Builds a square, center-cropped URL for an arbitrary Bilibili image.
pub fn square(url: &str, size: u32) -> String {
    let (prefix, rest) = image_parts(url);
    let mut out = String::with_capacity(prefix.len() + rest.len() + 24);
    out.push_str(prefix);
    out.push_str(rest);
    let _ = write!(out, "@{size}w_{size}h_1c.webp");
    out
}

/// Builds a width-only URL that keeps the image's aspect ratio.
pub fn scaled(url: &str, width: u32) -> String {
    resized(url, width, "webp")
}

/// Builds a width-only PNG URL that keeps the image's aspect ratio.
pub fn scaled_png(url: &str, width: u32) -> String {
    resized(url, width, "png")
}

fn resized(url: &str, width: u32, format: &str) -> String {
    let (prefix, rest) = image_parts(url);
    let mut out = String::with_capacity(prefix.len() + rest.len() + 24);
    out.push_str(prefix);
    out.push_str(rest);
    let _ = write!(out, "@{width}w_1c.{format}");
    out
}

/// Builds a width-only URL for wide images such as space banners.
pub fn banner(url: &str, width: u32) -> String {
    scaled(url, width)
}

fn image_parts(url: &str) -> (&'static str, &str) {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("//"))
        .unwrap_or(url);
    let rest = rest.split('@').next().unwrap_or(rest);
    let prefix = if rest.starts_with("bfs/") {
        "https://i0.hdslb.com/"
    } else {
        "https://"
    };
    (prefix, rest)
}

#[cfg(test)]
mod tests {
    use super::{avatar, banner, normalize_url, scaled, scaled_png, square, thumbnail};

    #[test]
    fn normalizes_protocol_relative_urls() {
        assert_eq!(
            normalize_url("//i0.hdslb.com/bfs/archive/a.jpg"),
            "https://i0.hdslb.com/bfs/archive/a.jpg"
        );
        assert_eq!(
            normalize_url("https://i0.hdslb.com/bfs/archive/a.jpg"),
            "https://i0.hdslb.com/bfs/archive/a.jpg"
        );
        assert_eq!(normalize_url(""), "");
    }

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

    #[test]
    fn builds_square_avatar_urls() {
        assert_eq!(
            avatar("http://i0.hdslb.com/bfs/face/a.jpg", 64),
            "https://i0.hdslb.com/bfs/face/a.jpg@64w_64h_1c.webp"
        );
        assert_eq!(
            avatar("//i1.hdslb.com/bfs/face/a.jpg", 96),
            "https://i1.hdslb.com/bfs/face/a.jpg@96w_96h_1c.webp"
        );
    }

    #[test]
    fn builds_square_and_scaled_urls_for_comment_pictures() {
        assert_eq!(
            square("//i0.hdslb.com/bfs/new_dyn/a.png", 192),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@192w_192h_1c.webp"
        );
        assert_eq!(
            scaled("http://i0.hdslb.com/bfs/new_dyn/a.png", 1280),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w_1c.webp"
        );
        assert_eq!(
            scaled("//i0.hdslb.com/bfs/new_dyn/a.png@320w_1c.webp", 1280),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w_1c.webp"
        );
        assert_eq!(
            scaled_png("//i0.hdslb.com/bfs/new_dyn/a.png@320w_1c.webp", 800),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@800w_1c.png"
        );
    }
}
