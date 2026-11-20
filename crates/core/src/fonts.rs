use std::path::PathBuf;

use rigili_bilibili::data_dir;

const FILE: &str = "LXGWNeoXiHei.ttf";

/// The UI font shipped with the binary, so every platform renders the same face.
pub fn bytes() -> &'static [u8] {
    include_bytes!("../assets/fonts/LXGWNeoXiHei.ttf")
}

/// Directory mpv scans for OSD fonts.
///
/// libass resolves `\fn` through fontconfig, which only sees installed files,
/// so the bundled face has to exist on disk for danmaku to use it instead of a
/// system fallback.
pub fn osd_dir() -> Option<PathBuf> {
    let dir = data_dir()?.join("fonts");
    let path = dir.join(FILE);
    let installed = std::fs::metadata(&path).is_ok_and(|meta| meta.len() == bytes().len() as u64);
    if !installed {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes()).ok()?;
    }
    Some(dir)
}

/// The factor that turns an em size into the `\fs` libass needs to draw it.
///
/// libass replaces the face's ascender and descender with the OS/2 win metrics
/// and then asks FreeType for `FT_SIZE_REQUEST_TYPE_REAL_DIM`, so `\fs` denotes
/// `unitsPerEm / (usWinAscent + usWinDescent)` ems instead of one.
pub fn libass_em_scale() -> f32 {
    let metrics = face_metrics();
    (metrics.win_ascent + metrics.win_descent) / metrics.upem
}

/// The `line_height` that lands a GPUI baseline where libass draws it.
///
/// libass aligns `\an7` with the top of its own line box, which puts the
/// baseline `usWinAscent` below the position. GPUI centers the line inside
/// `line_height` using the hhea (or OS/2 typo) metrics, so the padding has to be
/// balanced against that ascent instead.
pub fn baseline_line_height(font_size: f32) -> f32 {
    let metrics = face_metrics();
    font_size * (2. * metrics.win_ascent - metrics.ascent + metrics.descent) / metrics.upem
}

struct FaceMetrics {
    upem: f32,
    win_ascent: f32,
    win_descent: f32,
    ascent: f32,
    descent: f32,
}

fn face_metrics() -> &'static FaceMetrics {
    const USE_TYPO_METRICS: u16 = 1 << 7;

    static METRICS: std::sync::OnceLock<FaceMetrics> = std::sync::OnceLock::new();
    METRICS.get_or_init(|| {
        let font = bytes();
        let head = table(font, b"head").expect("the bundled font has a head table");
        let os2 = table(font, b"OS/2").expect("the bundled font has an OS/2 table");
        let hhea = table(font, b"hhea").expect("the bundled font has an hhea table");
        let upem = f32::from(be16(font, head + 18).expect("head.unitsPerEm"));
        let win_ascent = f32::from(be16(font, os2 + 74).expect("OS/2.usWinAscent"));
        let win_descent = f32::from(be16(font, os2 + 76).expect("OS/2.usWinDescent"));
        let typo = be16(font, os2 + 62).is_some_and(|flags| flags & USE_TYPO_METRICS != 0);
        let (ascent, descent) = if typo {
            (
                f32::from(be_i16(font, os2 + 68).expect("OS/2.sTypoAscender")),
                f32::from(be_i16(font, os2 + 70).expect("OS/2.sTypoDescender")),
            )
        } else {
            (
                f32::from(be_i16(font, hhea + 4).expect("hhea.ascender")),
                f32::from(be_i16(font, hhea + 6).expect("hhea.descender")),
            )
        };
        FaceMetrics {
            upem,
            win_ascent,
            win_descent,
            ascent,
            descent: -descent,
        }
    })
}

fn table(data: &[u8], tag: &[u8; 4]) -> Option<usize> {
    let count = be16(data, 4)?;
    for index in 0..count {
        let record = 12 + usize::from(index) * 16;
        if data.get(record..record + 4)? == tag {
            return be32(data, record + 8).map(|offset| offset as usize);
        }
    }
    None
}

fn be16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn be_i16(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn be32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{baseline_line_height, bytes, face_metrics, libass_em_scale};

    #[test]
    fn ships_the_ui_font() {
        assert!(bytes().len() > 1_000_000);
    }

    #[test]
    fn libass_shrinks_the_bundled_face() {
        let scale = libass_em_scale();
        assert!((scale - 1.195).abs() < 0.005, "{scale}");
    }

    #[test]
    fn the_pinned_line_height_puts_the_baseline_where_libass_draws_it() {
        let font_size = 100.;
        let face = face_metrics();
        let ascent = face.ascent / face.upem * font_size;
        let descent = face.descent / face.upem * font_size;
        let pinned = (baseline_line_height(font_size) - ascent - descent) / 2. + ascent;
        let libass = face.win_ascent / face.upem * font_size;
        assert!((pinned - libass).abs() < 0.01, "{pinned} vs {libass}");
    }
}
