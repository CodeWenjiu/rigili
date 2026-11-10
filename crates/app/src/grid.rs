use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{PopularVideo, RecommendVideo, SearchVideo, thumbnail};

use crate::format::{format_count, format_date, format_duration};
use crate::platform::theme::cover_scrim;

pub(crate) const CARD_GAP: Pixels = px(16.);
pub(crate) const MIN_CARD_WIDTH: f32 = 200.;
const CARD_TEXT_HEIGHT_REMS: f32 = 4.25;
const CARD_BODY_GAP: f32 = 8.;

#[derive(Clone)]
pub(crate) struct OpenVideo {
    pub(crate) bvid: String,
    pub(crate) cid: Option<u64>,
    pub(crate) title: String,
}

#[derive(Clone)]
pub(crate) struct VideoCard {
    bvid: String,
    cid: Option<u64>,
    cover: String,
    title: String,
    meta: String,
    duration: String,
    views: String,
    danmaku: String,
}

impl VideoCard {
    pub(crate) fn bvid(&self) -> &str {
        &self.bvid
    }

    pub(crate) fn cover(&self) -> &str {
        &self.cover
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn meta(&self) -> &str {
        &self.meta
    }

    pub(crate) fn duration(&self) -> &str {
        &self.duration
    }

    pub(crate) fn views(&self) -> &str {
        &self.views
    }

    pub(crate) fn danmaku(&self) -> &str {
        &self.danmaku
    }

    pub(crate) fn open(&self) -> OpenVideo {
        OpenVideo {
            bvid: self.bvid.clone(),
            cid: self.cid,
            title: self.title.clone(),
        }
    }
}

impl From<RecommendVideo> for VideoCard {
    fn from(video: RecommendVideo) -> Self {
        let RecommendVideo {
            bvid,
            cid,
            pic,
            title,
            duration,
            pubdate,
            owner,
            stat,
        } = video;

        Self {
            bvid,
            cid: (cid != 0).then_some(cid),
            cover: pic,
            title,
            meta: format!("{} · {}", owner.name, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(stat.view),
            danmaku: format_count(stat.danmaku),
        }
    }
}

impl From<PopularVideo> for VideoCard {
    fn from(video: PopularVideo) -> Self {
        let PopularVideo {
            bvid,
            cid,
            pic,
            title,
            duration,
            pubdate,
            owner,
            stat,
            ..
        } = video;

        Self {
            bvid,
            cid: (cid != 0).then_some(cid),
            cover: pic,
            title,
            meta: format!("{} · {}", owner.name, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(stat.view),
            danmaku: format_count(stat.danmaku),
        }
    }
}

impl From<SearchVideo> for VideoCard {
    fn from(video: SearchVideo) -> Self {
        let SearchVideo {
            bvid,
            title,
            pic,
            author,
            duration,
            pubdate,
            play,
            danmaku,
        } = video;

        Self {
            bvid,
            cid: None,
            cover: pic,
            title,
            meta: format!("{} · {}", author, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(play),
            danmaku: format_count(danmaku),
        }
    }
}

pub(crate) fn append_unique(videos: &mut Vec<VideoCard>, incoming: Vec<VideoCard>) {
    let mut seen = videos
        .iter()
        .map(|video| video.bvid().to_owned())
        .collect::<HashSet<_>>();
    for video in incoming {
        if seen.insert(video.bvid().to_owned()) {
            videos.push(video);
        }
    }
}

pub(crate) fn card_text_height(window: &Window) -> Pixels {
    rems(CARD_TEXT_HEIGHT_REMS).to_pixels(window.rem_size())
}

pub(crate) fn card_height(width: Pixels, text_height: Pixels) -> Pixels {
    px(width.as_f32() * 9. / 16.) + px(CARD_BODY_GAP) + text_height
}

#[derive(Clone, Copy)]
pub(crate) struct CardMetrics {
    pub(crate) width: Pixels,
    pub(crate) text_height: Pixels,
}

impl CardMetrics {
    pub(crate) fn new(width: Pixels, text_height: Pixels) -> Self {
        Self { width, text_height }
    }

    pub(crate) fn height(self) -> Pixels {
        card_height(self.width, self.text_height)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CardStyle {
    foreground: Hsla,
    muted: Hsla,
    placeholder: Hsla,
    primary: Hsla,
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
            radius: theme.radius,
        }
    }
}

pub(crate) struct MediaCard<'a> {
    pub(crate) id: SharedString,
    pub(crate) cover: &'a str,
    pub(crate) title: &'a str,
    pub(crate) footer: &'a str,
    pub(crate) overlay: CardOverlay<'a>,
}

pub(crate) struct CardOverlay<'a> {
    pub(crate) leading: Option<(IconName, &'a str)>,
    pub(crate) secondary: Option<(Option<IconName>, &'a str)>,
    pub(crate) trailing: Option<&'a str>,
    pub(crate) badge: Option<&'a str>,
}

pub(crate) fn media_card(
    card: MediaCard<'_>,
    style: CardStyle,
    width: Pixels,
    text_height: Pixels,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let MediaCard {
        id,
        cover,
        title,
        footer,
        overlay,
    } = card;

    v_flex()
        .group("media-card")
        .w(width)
        .h(card_height(width, text_height))
        .gap(px(CARD_BODY_GAP))
        .id(id)
        .cursor_pointer()
        .on_click(on_open)
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(style.radius)
                .overflow_hidden()
                .bg(style.placeholder)
                .child(
                    img(thumbnail(cover, 480))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .when_some(overlay.badge, |this, badge| {
                    this.child(
                        div()
                            .absolute()
                            .top_2()
                            .left_2()
                            .px_2()
                            .py_1()
                            .rounded(style.radius)
                            .bg(style.primary)
                            .text_xs()
                            .text_color(Hsla::white())
                            .child(badge.to_owned()),
                    )
                })
                .child(
                    h_flex()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .pt_4()
                        .pb_1()
                        .text_xs()
                        .text_color(Hsla::white())
                        .bg(cover_scrim())
                        .when_some(overlay.leading, |this, (icon, text)| {
                            this.child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(icon)
                                    .child(text.to_owned()),
                            )
                        })
                        .when_some(overlay.secondary, |this, (icon, text)| {
                            this.child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .when_some(icon, |this, icon| this.child(icon))
                                    .child(text.to_owned()),
                            )
                        })
                        .child(div().flex_1())
                        .when_some(overlay.trailing, |this, trailing| {
                            this.child(trailing.to_owned())
                        }),
                ),
        )
        .child(
            v_flex()
                .h(text_height)
                .gap_2()
                .justify_between()
                .overflow_hidden()
                .child(
                    div()
                        .text_sm()
                        .line_clamp(2)
                        .text_color(style.foreground)
                        .group_hover("media-card", |this| this.text_color(style.primary))
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted)
                        .truncate()
                        .child(footer.to_owned()),
                ),
        )
}

pub(crate) fn video_card(
    video: &VideoCard,
    style: CardStyle,
    width: Pixels,
    text_height: Pixels,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    media_card(
        MediaCard {
            id: SharedString::from(video.bvid.clone()),
            cover: &video.cover,
            title: &video.title,
            footer: &video.meta,
            overlay: CardOverlay {
                leading: Some((IconName::Play, &video.views)),
                secondary: Some((Some(IconName::MessageCircle), &video.danmaku)),
                trailing: Some(&video.duration),
                badge: None,
            },
        },
        style,
        width,
        text_height,
        on_open,
    )
}

pub(crate) fn skeleton_card(width: Pixels, radius: Pixels) -> impl IntoElement {
    v_flex()
        .w(width)
        .gap_2()
        .child(
            Skeleton::new()
                .w_full()
                .h_auto()
                .aspect_ratio(16. / 9.)
                .rounded(radius),
        )
        .child(Skeleton::new().h_4().w_3_4())
        .child(Skeleton::new().h_3().w_1_3())
}

pub(crate) fn columns_for_width(width: Pixels) -> usize {
    if width <= px(0.) {
        return 1;
    }
    let gap = CARD_GAP.as_f32();
    (((width.as_f32() + gap) / (MIN_CARD_WIDTH + gap)).floor() as usize).max(1)
}

pub(crate) fn card_width_for_width(width: Pixels, columns: usize) -> Pixels {
    if width <= px(0.) || columns == 0 {
        return px(MIN_CARD_WIDTH);
    }
    let gaps = CARD_GAP.as_f32() * (columns - 1) as f32;
    px(((width.as_f32() - gaps) / columns as f32).max(0.))
}

#[cfg(test)]
mod tests {
    use super::{
        CARD_GAP, MIN_CARD_WIDTH, VideoCard, append_unique, card_width_for_width, columns_for_width,
    };
    use gpui_kit::px;
    use rigili_bilibili::SearchVideo;

    fn video(bvid: &str) -> VideoCard {
        VideoCard {
            bvid: bvid.into(),
            cid: None,
            cover: String::new(),
            title: String::new(),
            meta: String::new(),
            duration: String::new(),
            views: String::new(),
            danmaku: String::new(),
        }
    }

    #[test]
    fn fits_cards_into_the_available_width() {
        let min = MIN_CARD_WIDTH;
        let gap = CARD_GAP.as_f32();

        assert_eq!(columns_for_width(px(0.)), 1);
        assert_eq!(columns_for_width(px(min)), 1);
        assert_eq!(columns_for_width(px(min * 2. + gap - 1.)), 1);
        assert_eq!(columns_for_width(px(min * 2. + gap)), 2);
        assert_eq!(columns_for_width(px(min * 3. + gap * 2.)), 3);
    }

    #[test]
    fn stretches_cards_to_fill_the_row() {
        let gap = CARD_GAP.as_f32();

        let width = px(MIN_CARD_WIDTH * 2. + gap + 1.);
        let columns = columns_for_width(width);
        let card_width = card_width_for_width(width, columns);
        assert_eq!(columns, 2);
        assert!((card_width.as_f32() - (MIN_CARD_WIDTH + 0.5)).abs() < 0.01);
        assert!(
            (card_width.as_f32() * columns as f32 + gap * (columns - 1) as f32 - width.as_f32())
                .abs()
                < 0.01
        );

        let width = px(MIN_CARD_WIDTH * 4. + gap * 3. + 1.);
        let columns = columns_for_width(width);
        let card_width = card_width_for_width(width, columns);
        assert_eq!(columns, 4);
        assert!((card_width.as_f32() - (MIN_CARD_WIDTH + 0.25)).abs() < 0.01);

        assert_eq!(card_width_for_width(px(0.), 1), px(MIN_CARD_WIDTH));
    }

    #[test]
    fn appends_only_new_videos() {
        let mut videos = vec![video("BV1")];
        append_unique(&mut videos, vec![video("BV1"), video("BV2")]);
        assert_eq!(
            videos.iter().map(|video| video.bvid()).collect::<Vec<_>>(),
            ["BV1", "BV2"]
        );
    }

    #[test]
    fn formats_search_cards() {
        let card = VideoCard::from(SearchVideo {
            bvid: "BV1".into(),
            title: "标题".into(),
            pic: "https://example.invalid/cover.jpg".into(),
            author: "UP主".into(),
            duration: 156,
            pubdate: 1_762_646_400,
            play: 12_345,
            danmaku: 678,
        });

        assert_eq!(card.meta, "UP主 · 11-9");
        assert_eq!(card.duration, "02:36");
        assert_eq!(card.views, "1.2万");
        assert_eq!(card.danmaku, "678");
    }
}
