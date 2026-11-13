use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{
    FavItem, HistoryItem, LiveRoom, PgcItem, PopularVideo, RecommendVideo, SearchLiveRoom,
    SearchMedia, SearchVideo, SpaceVideo, WatchLaterItem, normalize_url, thumbnail,
};
use rigili_core::{OpenSpace, OpenVideo, format_count, format_date, format_duration, format_ymd};

use crate::platform::theme::cover_scrim;
use crate::preview::{self, PreviewSource};
use crate::watch_later;

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

#[derive(Clone)]
pub(crate) struct VideoCard {
    id: SharedString,
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
    pub(crate) fn key(&self) -> String {
        format!("video:{}", self.bvid)
    }

    pub(crate) fn id(&self) -> &SharedString {
        &self.id
    }

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

    pub(crate) fn cid(&self) -> Option<u64> {
        self.cid
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
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
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
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
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
        let id = SharedString::from(bvid.as_str());

        Self {
            id,
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
        let id = SharedString::from(video.bvid.as_str());

        Self {
            id,
            bvid: video.bvid,
            cid: None,
            cover: video.pic,
            title: video.title,
            author: video.author.clone(),
            author_mid: (video.mid != 0).then_some(video.mid),
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
        let id = SharedString::from(item.bvid.as_str());

        Self {
            id,
            bvid: item.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.upper.name.clone(),
            author_mid: (item.upper.mid != 0).then_some(item.upper.mid),
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
        let id = SharedString::from(item.history.bvid.as_str());

        Self {
            id,
            bvid: item.history.bvid,
            cid: None,
            cover: item.cover,
            title: item.title,
            author: item.author_name.clone(),
            author_mid: (item.author_mid != 0).then_some(item.author_mid),
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
        let id = SharedString::from(item.bvid.as_str());

        Self {
            id,
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

#[derive(Clone)]
pub(crate) struct LiveCard {
    pub(crate) id: SharedString,
    pub(crate) room_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) online: String,
}

impl LiveCard {
    pub(crate) fn key(&self) -> String {
        format!("live:{}", self.room_id)
    }
}

impl From<LiveRoom> for LiveCard {
    fn from(room: LiveRoom) -> Self {
        let cover = if room.cover.is_empty() {
            room.system_cover
        } else {
            room.cover
        };
        let meta = if room.area_name.is_empty() {
            room.uname
        } else {
            format!("{} · {}", room.uname, room.area_name)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.roomid)),
            room_id: room.roomid,
            cover: normalize_url(&cover),
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

impl From<SearchLiveRoom> for LiveCard {
    fn from(room: SearchLiveRoom) -> Self {
        let meta = if room.area.is_empty() {
            room.name
        } else {
            format!("{} · {}", room.name, room.area)
        };

        Self {
            id: SharedString::from(format!("live:{}", room.room_id)),
            room_id: room.room_id,
            cover: room.cover,
            title: room.title,
            meta,
            online: format_count(room.online),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PgcCard {
    pub(crate) id: SharedString,
    pub(crate) season_id: u64,
    pub(crate) cover: String,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) score: String,
    pub(crate) index_show: String,
    pub(crate) badge: Option<String>,
}

impl PgcCard {
    pub(crate) fn key(&self) -> String {
        format!("pgc:{}", self.season_id)
    }
}

impl From<PgcItem> for PgcCard {
    fn from(item: PgcItem) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: (!item.badge.is_empty()).then_some(item.badge),
        }
    }
}

impl From<SearchMedia> for PgcCard {
    fn from(item: SearchMedia) -> Self {
        Self {
            id: SharedString::from(format!("pgc:{}", item.season_id)),
            season_id: item.season_id,
            cover: item.cover,
            title: item.title,
            subtitle: item.subtitle,
            score: item.score,
            index_show: item.index_show,
            badge: item.badge,
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

pub(crate) struct MediaCard<'a> {
    pub(crate) id: SharedString,
    pub(crate) cover: &'a str,
    pub(crate) title: &'a str,
    pub(crate) footer: Option<&'a str>,
    pub(crate) footer_icon: Option<IconName>,
    pub(crate) title_lines: usize,
    pub(crate) thumb_width: u32,
    pub(crate) overlay: CardOverlay<'a>,
    pub(crate) author: Option<MediaAuthor>,
    pub(crate) video: Option<MediaVideo>,
}

#[derive(Clone)]
pub(crate) struct MediaVideo {
    pub(crate) bvid: SharedString,
    pub(crate) cid: Option<u64>,
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
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let MediaCard {
        id,
        cover,
        title,
        footer,
        footer_icon,
        title_lines,
        thumb_width,
        overlay,
        author,
        video,
    } = card;

    if video.is_some() {
        watch_later::ensure(window, cx);
    }
    let in_watch_later = video
        .as_ref()
        .is_some_and(|video| watch_later::contains(cx, &video.bvid));
    let watch_later_color = if in_watch_later {
        style.primary
    } else {
        Hsla::white()
    };

    let on_open: Rc<OpenHandler> = Rc::new(on_open);
    let navigate = |handler: Rc<OpenHandler>| {
        move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            handler.as_ref()(event, window, cx);
        }
    };

    let preview = video.as_ref().map(|video| {
        preview::slot(
            window,
            cx,
            ElementId::NamedInteger(id.clone(), 3),
            PreviewSource::new(video.bvid.clone(), video.cid),
        )
    });
    let cover_click = {
        let on_open = on_open.clone();
        let preview = preview.clone();
        move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            if let Some(preview) = &preview {
                preview.update(cx, |preview, cx| preview.stop(Some(&mut *window), cx));
            }
            cx.stop_propagation();
            on_open.as_ref()(event, window, cx);
        }
    };

    let cover_id = ElementId::NamedInteger(id.clone(), 0);
    let title_id = ElementId::NamedInteger(id.clone(), 1);
    let meta_id = ElementId::NamedInteger(id.clone(), 2);
    let watch_later_id = ElementId::NamedInteger(id.clone(), 4);
    let title_tooltip = title.to_owned();
    let meta_tooltip = footer.unwrap_or_default().to_owned();
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
                .on_click(cover_click)
                .when_some(preview.clone(), |this, preview| {
                    this.on_hover(move |hovered, window, cx| {
                        preview.update(cx, |preview, cx| {
                            preview.set_hovered(*hovered, window, cx);
                        });
                    })
                })
                .child(
                    img(thumbnail(cover, thumb_width))
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
                .when(preview.is_none(), |this| {
                    this.child(
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
                })
                .when_some(preview.clone(), |this, preview| this.child(preview))
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
                )
                .when_some(video, |this, video| {
                    this.child(
                        Button::new(watch_later_id)
                            .ghost()
                            .absolute()
                            .top_2()
                            .right_2()
                            .w(px(28.))
                            .h(px(28.))
                            .p_0()
                            .rounded(px(6.))
                            .bg(hsla(0., 0., 0., 0.55))
                            .text_color(watch_later_color)
                            .opacity(0.)
                            .group_hover("media-cover", |this| this.opacity(1.))
                            .focus_visible(|this| this.opacity(1.))
                            .tooltip(if in_watch_later {
                                "从稍后再看移除"
                            } else {
                                "稍后再看"
                            })
                            .accessibility_label(if in_watch_later {
                                "从稍后再看移除"
                            } else {
                                "稍后再看"
                            })
                            .on_click(move |_event, window, cx| {
                                cx.stop_propagation();
                                watch_later::toggle(video.bvid.clone(), window, cx);
                            })
                            .child(
                                Icon::new(IconName::Clock)
                                    .with_size(px(18.))
                                    .text_color(watch_later_color),
                            ),
                    )
                }),
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
                        .line_clamp(title_lines)
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
                .when_some(footer, |this, footer| {
                    this.child(
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
                    )
                }),
        )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VideoCardKind {
    #[default]
    Default,
    Search,
}

pub(crate) fn video_card<V>(
    card: &VideoCard,
    kind: VideoCardKind,
    style: CardStyle,
    metrics: CardMetrics,
    window: &mut Window,
    cx: &mut Context<V>,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + 'static,
{
    let views = card.views();
    let danmaku = card.danmaku();
    let duration = card.duration();
    let author = card.author_mid().map(|mid| {
        let name = card.author().to_owned();
        MediaAuthor {
            on_click: Rc::new(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(OpenSpace {
                    mid,
                    name: name.clone(),
                });
            })),
        }
    });
    let (footer_icon, secondary) = match kind {
        VideoCardKind::Default => (
            None,
            (!danmaku.is_empty()).then_some((Some(IconName::MessageCircle), danmaku)),
        ),
        VideoCardKind::Search => (
            Some(IconName::MonitorPlay),
            Some((Some(IconName::ThumbsUp), card.like())),
        ),
    };

    media_card(
        MediaCard {
            id: card.id().clone(),
            cover: card.cover(),
            title: card.title(),
            footer: Some(card.meta()),
            footer_icon,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            author,
            video: Some(MediaVideo {
                bvid: card.id().clone(),
                cid: card.cid(),
            }),
            overlay: CardOverlay {
                leading: (!views.is_empty()).then_some((IconName::Play, views)),
                secondary,
                trailing: (!duration.is_empty()).then_some(duration),
                badge: None,
            },
        },
        style,
        metrics.width,
        metrics.text_height,
        window,
        cx,
        on_open,
    )
    .into_any_element()
}

pub(crate) fn video_card_row<V>(
    cards: &[VideoCard],
    row: usize,
    columns: usize,
    frame: CardFrame,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + EventEmitter<OpenVideo> + 'static,
{
    let CardFrame { style, metrics } = frame;
    let start = row * columns;
    let end = (start + columns).min(cards.len());

    h_flex()
        .h(metrics.height())
        .gap(CARD_GAP)
        .children((start..end).map(|index| {
            let open = cards[index].open();
            let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(open.clone()));
            video_card(
                &cards[index],
                VideoCardKind::Default,
                style,
                metrics,
                window,
                cx,
                on_open,
            )
        }))
        .into_any_element()
}

pub(crate) fn live_card(
    card: &LiveCard,
    style: CardStyle,
    metrics: CardMetrics,
    footer_icon: Option<IconName>,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    media_card(
        MediaCard {
            id: card.id.clone(),
            cover: &card.cover,
            title: &card.title,
            footer: Some(&card.meta),
            footer_icon,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            overlay: CardOverlay {
                leading: Some((IconName::Eye, &card.online)),
                secondary: None,
                trailing: Some("直播中"),
                badge: None,
            },
            author: None,
            video: None,
        },
        style,
        metrics.width,
        metrics.text_height,
        window,
        cx,
        on_open,
    )
    .into_any_element()
}

pub(crate) fn pgc_card(
    card: &PgcCard,
    style: CardStyle,
    metrics: CardMetrics,
    footer_icon: Option<IconName>,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    media_card(
        MediaCard {
            id: card.id.clone(),
            cover: &card.cover,
            title: &card.title,
            footer: Some(&card.subtitle),
            footer_icon,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            overlay: CardOverlay {
                leading: (!card.score.is_empty()).then_some((IconName::Star, card.score.as_str())),
                secondary: None,
                trailing: (!card.index_show.is_empty()).then_some(card.index_show.as_str()),
                badge: card.badge.as_deref(),
            },
            author: None,
            video: None,
        },
        style,
        metrics.width,
        metrics.text_height,
        window,
        cx,
        on_open,
    )
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

pub(crate) fn card_grid_loading<G: CardGrid>(
    id: impl Into<ElementId>,
    width: Pixels,
    text_height: Pixels,
    scale_factor: f32,
    radius: Pixels,
) -> AnyElement {
    let columns = G::columns(width);
    let metrics = G::metrics(width, text_height, scale_factor);

    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            h_flex()
                .flex_wrap()
                .gap_x(CARD_GAP)
                .gap_y_6()
                .children((0..columns * 2).map(|_| skeleton_card(metrics.width, radius))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        CARD_GAP, CardGrid, FolderCards, LiveCard, VideoCard, VideoCards, round_to_device_pixel,
    };
    use crate::paging::{append_unique_by, index_by};
    use gpui_kit::px;
    use rigili_bilibili::{
        FavCounts, FavItem, FavUpper, HistoryItem, HistoryRef, LiveRoom, SearchVideo, SpaceVideo,
    };

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

    #[test]
    fn keeps_author_mids_for_profile_links() {
        let card = VideoCard::from(FavItem {
            bvid: "BV1".into(),
            title: String::new(),
            cover: String::new(),
            duration: 0,
            pubtime: 0,
            upper: FavUpper {
                mid: 42,
                name: "UP".into(),
            },
            cnt_info: FavCounts::default(),
        });
        assert_eq!(card.author_mid(), Some(42));

        let card = VideoCard::from(HistoryItem {
            title: String::new(),
            cover: String::new(),
            author_name: "UP".into(),
            author_mid: 43,
            duration: 0,
            view_at: 0,
            history: HistoryRef::default(),
        });
        assert_eq!(card.author_mid(), Some(43));

        let card = VideoCard::from(SpaceVideo {
            bvid: "BV3".into(),
            author: "UP".into(),
            mid: 44,
            ..Default::default()
        });
        assert_eq!(card.author_mid(), Some(44));
    }

    #[test]
    fn appends_only_new_items_by_key() {
        let mut items = vec!["BV1".to_owned()];
        let mut seen = index_by(&items, |item| item.clone());
        append_unique_by(
            &mut items,
            &mut seen,
            vec!["BV1".to_owned(), "BV2".to_owned()],
            |item| item.clone(),
        );
        assert_eq!(items, ["BV1", "BV2"]);
    }

    #[test]
    fn normalizes_live_room_cards() {
        let card = LiveCard::from(LiveRoom {
            roomid: 7,
            title: "直播".into(),
            uname: "UP主".into(),
            online: 12_345,
            cover: "//i0.hdslb.com/cover.jpg".into(),
            system_cover: String::new(),
            area_name: "游戏".into(),
            face: String::new(),
        });

        assert_eq!(card.id.as_ref(), "live:7");
        assert_eq!(card.cover, "https://i0.hdslb.com/cover.jpg");
        assert_eq!(card.meta, "UP主 · 游戏");
        assert_eq!(card.online, "1.2万");
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
