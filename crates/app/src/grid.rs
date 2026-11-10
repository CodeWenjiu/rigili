use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{PopularVideo, RecommendVideo, SearchVideo, thumbnail};

use crate::format::{format_count, format_date, format_duration};
use crate::platform::theme::cover_scrim;

pub(crate) const CARD_GAP: Pixels = px(16.);
pub(crate) const LIST_FOOTER_HEIGHT: Pixels = px(56.);
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
    like: String,
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

    pub(crate) fn like(&self) -> &str {
        &self.like
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
            like: format_count(stat.like),
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
            like: format_count(stat.like),
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
            like,
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
            like: format_count(like),
        }
    }
}

pub(crate) fn card_text_height(window: &Window) -> Pixels {
    rems(CARD_TEXT_HEIGHT_REMS).to_pixels(window.rem_size())
}

pub(crate) fn card_height(width: Pixels, text_height: Pixels) -> Pixels {
    px(width.as_f32() * 9. / 16.) + px(CARD_BODY_GAP) + text_height
}

pub(crate) fn append_unique_by<T>(
    items: &mut Vec<T>,
    incoming: Vec<T>,
    key: impl Fn(&T) -> String,
) {
    let mut seen = items.iter().map(&key).collect::<HashSet<_>>();
    items.extend(incoming.into_iter().filter(|item| seen.insert(key(item))));
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

pub(crate) struct MediaCard<'a> {
    pub(crate) id: SharedString,
    pub(crate) cover: &'a str,
    pub(crate) title: &'a str,
    pub(crate) footer: &'a str,
    pub(crate) footer_icon: Option<IconName>,
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
        footer_icon,
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
                    h_flex()
                        .items_center()
                        .gap_1()
                        .overflow_hidden()
                        .text_xs()
                        .text_color(style.muted)
                        .when_some(footer_icon, |this, icon| {
                            this.child(Icon::new(icon).with_size(px(12.)))
                        })
                        .child(div().min_w_0().truncate().child(footer.to_owned())),
                ),
        )
}

pub(crate) fn list_footer(
    retry_id: impl Into<ElementId>,
    more_error: Option<String>,
    muted: Hsla,
    danger: Hsla,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let content = if let Some(error) = more_error {
        h_flex()
            .items_center()
            .gap_3()
            .child(div().text_sm().text_color(danger).child(error))
            .child(
                Button::new(retry_id)
                    .small()
                    .label("重试")
                    .on_click(on_retry),
            )
            .into_any_element()
    } else {
        h_flex()
            .items_center()
            .gap_2()
            .child(Spinner::new().small())
            .child(div().text_sm().text_color(muted).child("正在加载…"))
            .into_any_element()
    };

    h_flex()
        .h(LIST_FOOTER_HEIGHT)
        .items_center()
        .justify_center()
        .child(content)
        .into_any_element()
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
        CARD_GAP, MIN_CARD_WIDTH, VideoCard, append_unique_by, card_width_for_width,
        columns_for_width,
    };
    use gpui_kit::px;
    use rigili_bilibili::SearchVideo;

    #[test]
    fn fits_and_stretches_cards_into_the_available_width() {
        let min = MIN_CARD_WIDTH;
        let gap = CARD_GAP.as_f32();

        assert_eq!(columns_for_width(px(0.)), 1);
        assert_eq!(columns_for_width(px(min)), 1);
        assert_eq!(columns_for_width(px(min * 2. + gap - 1.)), 1);
        assert_eq!(columns_for_width(px(min * 2. + gap)), 2);
        assert_eq!(columns_for_width(px(min * 3. + gap * 2.)), 3);

        let width = px(min * 2. + gap + 1.);
        let columns = columns_for_width(width);
        let card_width = card_width_for_width(width, columns);
        assert_eq!(columns, 2);
        assert!((card_width.as_f32() - (min + 0.5)).abs() < 0.01);
        assert!(
            (card_width.as_f32() * columns as f32 + gap * (columns - 1) as f32 - width.as_f32())
                .abs()
                < 0.01
        );

        let width = px(min * 4. + gap * 3. + 1.);
        let columns = columns_for_width(width);
        let card_width = card_width_for_width(width, columns);
        assert_eq!(columns, 4);
        assert!((card_width.as_f32() - (min + 0.25)).abs() < 0.01);

        assert_eq!(card_width_for_width(px(0.), 1), px(min));
    }

    #[test]
    fn appends_only_new_items_by_key() {
        let mut items = vec!["BV1".to_owned()];
        append_unique_by(
            &mut items,
            vec!["BV1".to_owned(), "BV2".to_owned()],
            |item| item.clone(),
        );
        assert_eq!(items, ["BV1", "BV2"]);
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
            like: 90,
        });

        assert_eq!(card.meta, "UP主 · 11-9");
        assert_eq!(card.duration, "02:36");
        assert_eq!(card.views, "1.2万");
        assert_eq!(card.danmaku, "678");
        assert_eq!(card.like, "90");
    }
}
