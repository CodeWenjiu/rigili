use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{
    FavItem, HistoryItem, PopularVideo, RecommendVideo, SearchVideo, SpaceVideo, WatchLaterItem,
    thumbnail,
};
use rigili_core::{OpenVideo, format_count, format_date, format_duration, format_ymd};

use crate::platform::theme::cover_scrim;

pub(crate) const CARD_GAP: Pixels = px(16.);
pub(crate) const LIST_FOOTER_HEIGHT: Pixels = px(56.);
const CARD_TEXT_HEIGHT_REMS: f32 = 4.25;
const CARD_BODY_GAP: f32 = 8.;

pub(crate) trait CardGrid {
    const MIN_CARD_WIDTH: f32;

    fn columns(width: Pixels) -> usize {
        if width <= px(0.) {
            return 1;
        }
        let gap = CARD_GAP.as_f32();
        (((width.as_f32() + gap) / (Self::MIN_CARD_WIDTH + gap)).floor() as usize).max(1)
    }

    fn card_width(width: Pixels, columns: usize, scale_factor: f32) -> Pixels {
        if width <= px(0.) || columns == 0 {
            return px(Self::MIN_CARD_WIDTH);
        }
        // GPUI rounds authored lengths to device pixels before Taffy lays out
        // the row; resize by one device pixel so the last column still fits.
        let gaps_dp = round_to_device_pixel(CARD_GAP.as_f32(), scale_factor) * (columns - 1) as f32;
        let available_dp = (width.as_f32() * scale_factor - 1.).max(0.);
        let card_dp = ((available_dp - gaps_dp) / columns as f32).floor().max(0.);
        px(card_dp / scale_factor)
    }

    fn metrics(width: Pixels, text_height: Pixels, scale_factor: f32) -> CardMetrics {
        let columns = Self::columns(width);
        CardMetrics::new(Self::card_width(width, columns, scale_factor), text_height)
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

#[derive(Clone)]
pub(crate) struct VideoCard {
    bvid: String,
    cid: Option<u64>,
    cover: String,
    title: String,
    author: String,
    author_mid: Option<u64>,
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

    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    pub(crate) fn author_mid(&self) -> Option<u64> {
        self.author_mid
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
            author: owner.name.clone(),
            author_mid: (owner.mid != 0).then_some(owner.mid),
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
            author: owner.name.clone(),
            author_mid: (owner.mid != 0).then_some(owner.mid),
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
            mid,
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
            author: author.clone(),
            author_mid: (mid != 0).then_some(mid),
            meta: format!("{} · {}", author, format_date(pubdate)),
            duration: format_duration(duration),
            views: format_count(play),
            danmaku: format_count(danmaku),
            like: format_count(like),
        }
    }
}

impl From<SpaceVideo> for VideoCard {
    fn from(video: SpaceVideo) -> Self {
        Self {
            bvid: video.bvid,
            cid: None,
            cover: video.pic,
            title: video.title,
            author: video.author.clone(),
            author_mid: None,
            meta: format!("{} · {}", video.author, format_ymd(video.created)),
            duration: video.length,
            views: format_count(video.play),
            danmaku: format_count(video.video_review),
            like: String::new(),
        }
    }
}

impl From<FavItem> for VideoCard {
    fn from(item: FavItem) -> Self {
        Self {
            bvid: item.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.upper.name.clone(),
            author_mid: None,
            meta: format!("{} · {}", item.upper.name, format_date(item.pubtime)),
            duration: format_duration(item.duration),
            views: format_count(item.cnt_info.play),
            danmaku: format_count(item.cnt_info.danmaku),
            like: String::new(),
        }
    }
}

impl From<HistoryItem> for VideoCard {
    fn from(item: HistoryItem) -> Self {
        Self {
            bvid: item.history.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.author_name.clone(),
            author_mid: None,
            meta: format!("{} · {}", item.author_name, format_date(item.view_at)),
            duration: format_duration(item.duration),
            views: String::new(),
            danmaku: String::new(),
            like: String::new(),
        }
    }
}

impl From<WatchLaterItem> for VideoCard {
    fn from(item: WatchLaterItem) -> Self {
        Self {
            bvid: item.bvid,
            cid: (item.cid != 0).then_some(item.cid),
            cover: item.pic,
            title: item.title,
            author: item.owner.name.clone(),
            author_mid: (item.owner.mid != 0).then_some(item.owner.mid),
            meta: format!("{} · {}", item.owner.name, format_date(item.pubdate)),
            duration: format_duration(item.duration),
            views: format_count(item.stat.view),
            danmaku: format_count(item.stat.danmaku),
            like: format_count(item.stat.like),
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
    pub(crate) author: Option<MediaAuthor>,
}

pub(crate) struct MediaAuthor {
    pub(crate) on_click: Rc<OpenHandler>,
}

pub(crate) struct CardOverlay<'a> {
    pub(crate) leading: Option<(IconName, &'a str)>,
    pub(crate) secondary: Option<(Option<IconName>, &'a str)>,
    pub(crate) trailing: Option<&'a str>,
    pub(crate) badge: Option<&'a str>,
}

pub(crate) type OpenHandler = dyn Fn(&ClickEvent, &mut Window, &mut App);

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
        author,
    } = card;

    let on_open: Rc<OpenHandler> = Rc::new(on_open);
    let navigate = |handler: Rc<OpenHandler>| {
        move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            handler.as_ref()(event, window, cx);
        }
    };

    let cover_id = ElementId::Name(format!("{id}-cover").into());
    let title_id = ElementId::Name(format!("{id}-title").into());
    let meta_id = ElementId::Name(format!("{id}-meta").into());
    let title_tooltip = title.to_owned();
    let meta_tooltip = footer.to_owned();
    let meta_click = match author {
        Some(author) => navigate(author.on_click),
        None => navigate(on_open.clone()),
    };

    v_flex()
        .w(width)
        .h(card_height(width, text_height))
        .gap(px(CARD_BODY_GAP))
        .id(id)
        .cursor_pointer()
        .on_click({
            let on_open = on_open.clone();
            move |event, window, cx| on_open.as_ref()(event, window, cx)
        })
        .child(
            div()
                .id(cover_id)
                .group("media-cover")
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(style.radius)
                .overflow_hidden()
                .bg(style.placeholder)
                .cursor_pointer()
                .on_click(navigate(on_open.clone()))
                .child(
                    img(thumbnail(cover, 480))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(hsla(0., 0., 0., 0.28))
                        .opacity(0.)
                        .group_hover("media-cover", |this| this.opacity(1.)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(0.)
                        .group_hover("media-cover", |this| this.opacity(1.))
                        .child(
                            div()
                                .size(px(40.))
                                .rounded_full()
                                .bg(Hsla::white().opacity(0.92))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Play)
                                        .with_size(px(18.))
                                        .text_color(Hsla::black().opacity(0.72)),
                                ),
                        ),
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
                        .id(title_id)
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .line_clamp(2)
                        .text_ellipsis()
                        .text_color(style.foreground)
                        .cursor_pointer()
                        .hover(|this| this.text_color(style.primary))
                        .on_click(navigate(on_open.clone()))
                        .tooltip(move |window, cx| {
                            Tooltip::new(title_tooltip.clone()).build(window, cx)
                        })
                        .child(title.to_owned()),
                )
                .child(
                    h_flex()
                        .id(meta_id)
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_xs()
                        .text_color(style.muted)
                        .cursor_pointer()
                        .hover(|this| this.text_color(style.primary))
                        .on_click(meta_click)
                        .when(!meta_tooltip.is_empty(), |this| {
                            this.tooltip(move |window, cx| {
                                Tooltip::new(meta_tooltip.clone()).build(window, cx)
                            })
                        })
                        .when_some(footer_icon, |this, icon| {
                            this.child(Icon::new(icon).with_size(px(12.)))
                        })
                        .child(div().min_w_0().truncate().child(footer.to_owned())),
                ),
        )
}

pub(crate) fn video_card(
    card: &VideoCard,
    id_prefix: &str,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let views = card.views();
    let danmaku = card.danmaku();
    let duration = card.duration();

    media_card(
        MediaCard {
            id: SharedString::from(format!("{id_prefix}-{}", card.bvid())),
            cover: card.cover(),
            title: card.title(),
            footer: card.meta(),
            footer_icon: None,
            author: None,
            overlay: CardOverlay {
                leading: (!views.is_empty()).then_some((IconName::Play, views)),
                secondary: (!danmaku.is_empty())
                    .then_some((Some(IconName::MessageCircle), danmaku)),
                trailing: (!duration.is_empty()).then_some(duration),
                badge: None,
            },
        },
        style,
        metrics.width,
        metrics.text_height,
        on_open,
    )
    .into_any_element()
}

pub(crate) fn profile_badge(
    label: impl Into<SharedString>,
    background: Hsla,
    foreground: Hsla,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_2()
        .py_1()
        .rounded(px(4.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .child(label.into())
        .into_any_element()
}

pub(crate) fn profile_stat(label: &'static str, value: String, cx: &App) -> AnyElement {
    v_flex()
        .items_center()
        .gap_1()
        .child(div().text_lg().child(value))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .into_any_element()
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

#[cfg(test)]
mod tests {
    use super::{
        CARD_GAP, CardGrid, FolderCards, VideoCard, VideoCards, append_unique_by,
        round_to_device_pixel,
    };
    use gpui_kit::px;
    use rigili_bilibili::SearchVideo;

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
    }

    #[test]
    fn folder_cards_use_a_wider_minimum_than_video_cards() {
        let width = px(1000.);
        assert!(FolderCards::columns(width) < VideoCards::columns(width));

        let columns = FolderCards::columns(width);
        assert_eq!(columns, 3);
        let card_width = FolderCards::card_width(width, columns, 1.);
        assert!(
            card_width.as_f32() * columns as f32 + CARD_GAP.as_f32() * (columns - 1) as f32
                <= 1000.
        );
    }

    #[test]
    fn exact_fit_keeps_the_last_column_on_the_same_row() {
        let width = px(1433.);
        let columns = FolderCards::columns(width);
        let card_width = FolderCards::card_width(width, columns, 1.);
        assert_eq!(columns, 5);
        assert_eq!(card_width, px(273.));
        assert!(
            card_width.as_f32() * columns as f32 + CARD_GAP.as_f32() * (columns - 1) as f32
                <= width.as_f32()
        );
    }

    #[test]
    fn fractional_scale_keeps_the_last_column_on_the_same_row() {
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
            mid: 0,
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
