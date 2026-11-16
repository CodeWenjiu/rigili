rigili_macro::mod_flat!(cards, render);

pub(crate) use rigili_core::video::VideoCard;

use gpui_kit::component::*;
use gpui_kit::*;

pub(crate) const CARD_GAP: Pixels = px(16.);
const CARD_TEXT_HEIGHT_REMS: f32 = 4.25;
const CARD_BODY_GAP: f32 = 8.;

pub(crate) trait CardGrid {
    const MIN_CARD_WIDTH: f32;
    const GAP: Pixels = CARD_GAP;
    const MAX_COLUMNS: usize = usize::MAX;

    fn columns(width: Pixels) -> usize {
        if width <= px(0.) {
            return 1;
        }
        let gap = Self::GAP.as_f32();
        (((width.as_f32() + gap) / (Self::MIN_CARD_WIDTH + gap)).floor() as usize)
            .clamp(1, Self::MAX_COLUMNS)
    }

    fn card_width(width: Pixels, columns: usize, scale_factor: f32) -> Pixels {
        if width <= px(0.) || columns == 0 {
            return px(Self::MIN_CARD_WIDTH);
        }
        // GPUI rounds authored lengths to device pixels before Taffy lays out
        // the row; resize by one device pixel so the last column still fits.
        let gaps_dp =
            round_to_device_pixel(Self::GAP.as_f32(), scale_factor) * (columns - 1) as f32;
        let available_dp = (width.as_f32() * scale_factor - 1.).max(0.);
        let card_dp = ((available_dp - gaps_dp) / columns as f32).floor().max(0.);
        px(card_dp / scale_factor)
    }

    fn metrics(width: Pixels, text_height: Pixels, scale_factor: f32) -> CardMetrics {
        let columns = Self::columns(width);
        let card_width = Self::card_width(width, columns, scale_factor);
        CardMetrics {
            width: card_width,
            text_height,
            thumb_width: thumbnail_width(card_width, scale_factor),
        }
    }
}

fn round_to_device_pixel(logical: f32, scale_factor: f32) -> f32 {
    (logical * scale_factor - 0.5).ceil()
}

pub(crate) struct VideoCards;

impl CardGrid for VideoCards {
    const MIN_CARD_WIDTH: f32 = 200.;
}

pub(crate) struct FolderCards;

impl CardGrid for FolderCards {
    const MIN_CARD_WIDTH: f32 = 260.;
}

pub(crate) struct BangumiCards;

impl CardGrid for BangumiCards {
    const MIN_CARD_WIDTH: f32 = 200.;
}

pub(crate) fn card_text_height(window: &Window) -> Pixels {
    rems(CARD_TEXT_HEIGHT_REMS).to_pixels(window.rem_size())
}

pub(crate) fn card_height(width: Pixels, text_height: Pixels) -> Pixels {
    px(width.as_f32() * 9. / 16.) + px(CARD_BODY_GAP) + text_height
}

pub(crate) fn folder_card_height(width: Pixels, window: &Window) -> Pixels {
    let gap = rems(0.5).to_pixels(window.rem_size());
    let line = window.line_height();
    let text = line * 0.875 + gap + line * 0.75;
    px((width * 9. / 16. + gap + text).as_f32().ceil())
}

#[derive(Clone, Copy)]
pub(crate) struct CardMetrics {
    pub(crate) width: Pixels,
    pub(crate) text_height: Pixels,
    pub(crate) thumb_width: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct CardFrame {
    pub(crate) style: CardStyle,
    pub(crate) metrics: CardMetrics,
}

impl CardMetrics {
    pub(crate) fn height(self) -> Pixels {
        card_height(self.width, self.text_height)
    }
}

fn thumbnail_width(card_width: Pixels, scale_factor: f32) -> u32 {
    const BUCKET: u32 = 32;
    const MIN_WIDTH: u32 = 160;
    const MAX_WIDTH: u32 = 960;

    let device_width = (card_width.as_f32() * scale_factor).ceil().max(0.) as u32;
    device_width
        .div_ceil(BUCKET)
        .saturating_mul(BUCKET)
        .clamp(MIN_WIDTH, MAX_WIDTH)
}

#[derive(Clone, Copy)]
pub(crate) struct CardStyle {
    foreground: Hsla,
    muted: Hsla,
    placeholder: Hsla,
    primary: Hsla,
    border: Hsla,
    hover: Hsla,
    radius: Pixels,
}

impl CardStyle {
    pub(crate) fn new(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            foreground: theme.foreground,
            muted: theme.muted_foreground,
            placeholder: theme.muted,
            primary: theme.primary,
            border: theme.border,
            hover: theme.accent,
            radius: theme.radius,
        }
    }

    pub(crate) fn foreground(&self) -> Hsla {
        self.foreground
    }

    pub(crate) fn muted(&self) -> Hsla {
        self.muted
    }

    pub(crate) fn primary(&self) -> Hsla {
        self.primary
    }

    pub(crate) fn placeholder(&self) -> Hsla {
        self.placeholder
    }

    pub(crate) fn border(&self) -> Hsla {
        self.border
    }

    pub(crate) fn hover(&self) -> Hsla {
        self.hover
    }

    pub(crate) fn radius(&self) -> Pixels {
        self.radius
    }
}

#[cfg(test)]
mod tests {
    use super::{CARD_GAP, CardGrid, FolderCards, VideoCards, round_to_device_pixel};
    use gpui_kit::px;

    #[test]
    fn fits_and_stretches_cards_into_the_available_width() {
        let min = VideoCards::MIN_CARD_WIDTH;
        let gap = CARD_GAP.as_f32();

        assert_eq!(VideoCards::columns(px(0.)), 1);
        assert_eq!(VideoCards::columns(px(min)), 1);
        assert_eq!(VideoCards::columns(px(min * 2. + gap - 1.)), 1);
        assert_eq!(VideoCards::columns(px(min * 2. + gap)), 2);
        assert_eq!(VideoCards::columns(px(min * 3. + gap * 2.)), 3);

        let width = px(min * 2. + gap + 1.);
        let columns = VideoCards::columns(width);
        let card_width = VideoCards::card_width(width, columns, 1.);
        assert_eq!(columns, 2);
        assert_eq!(card_width, px(min));
        assert!(
            card_width.as_f32() * columns as f32 + gap * (columns - 1) as f32 <= width.as_f32()
        );

        let width = px(min * 4. + gap * 3. + 1.);
        let columns = VideoCards::columns(width);
        let card_width = VideoCards::card_width(width, columns, 1.);
        assert_eq!(columns, 4);
        assert_eq!(card_width, px(min));

        assert_eq!(VideoCards::card_width(px(0.), 1, 1.), px(min));

        let width = px(1000.);
        assert!(FolderCards::columns(width) < VideoCards::columns(width));
        let columns = FolderCards::columns(width);
        assert_eq!(columns, 3);
        let card_width = FolderCards::card_width(width, columns, 1.);
        assert!(
            card_width.as_f32() * columns as f32 + gap * (columns - 1) as f32 <= width.as_f32()
        );
    }

    #[test]
    fn rounding_keeps_the_last_column_on_the_same_row() {
        let width = px(1433.);
        let columns = FolderCards::columns(width);
        let card_width = FolderCards::card_width(width, columns, 1.);
        assert_eq!(columns, 5);
        assert_eq!(card_width, px(273.));

        let gap = CARD_GAP.as_f32();
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            for width in [900., 1000., 1100., 1261., 1433., 1727.] {
                let width = px(width);
                let columns = FolderCards::columns(width);
                let card_width = FolderCards::card_width(width, columns, scale);
                let total_dp = round_to_device_pixel(card_width.as_f32(), scale) * columns as f32
                    + round_to_device_pixel(gap, scale) * (columns - 1) as f32;
                assert!(total_dp <= width.as_f32() * scale, "{width:?} @ {scale}x");
            }
        }
    }
}
