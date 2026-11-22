use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::thumbnail;
use rigili_core::video::{video_meta, video_title};
use rigili_core::{OpenSpace, OpenVideo};

use super::{
    CARD_BODY_GAP, CARD_GAP, CardGrid, CardMetrics, CardStyle, LiveCard, PgcCard, VideoCard,
    card_height,
};
use crate::app::platform::theme::cover_scrim;
use crate::shared::media::preview::{self, PreviewSource};
use crate::shared::media::watch_later;

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
    pub(crate) chrome: CoverChrome,
    pub(crate) selected: bool,
    pub(crate) active: bool,
}

/// How the cover's hover chrome joins the tree. A grid that tracks hover
/// skips it for every card but the one under the pointer, which keeps the
/// per-frame element count down while scrolling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoverChrome {
    Hidden,
    Shown,
    /// Dimmed and revealed by the cover's own hover, for lists that do not
    /// track hover themselves.
    OnHover,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CardView {
    pub(crate) chrome: CoverChrome,
    pub(crate) selected: bool,
    pub(crate) active: bool,
}

pub(crate) fn selection_outline(primary: Hsla, radius: Pixels, active: bool) -> impl IntoElement {
    let color = if active {
        primary
    } else {
        primary.opacity(0.4)
    };
    div()
        .absolute()
        .inset_0()
        .rounded(radius)
        .border_2()
        .border_color(color)
}

#[derive(Clone, Copy)]
pub(crate) struct CardRowView {
    pub(crate) frame: super::CardFrame,
    pub(crate) selection: Option<usize>,
    pub(crate) active: bool,
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
        chrome,
        selected,
        active,
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
    let meta_click = match author {
        Some(author) => navigate(author.on_click),
        None => navigate(on_open.clone()),
    };
    let reveal_now = chrome == CoverChrome::Shown;
    let reveal_on_hover = chrome == CoverChrome::OnHover;
    let chrome_visible = reveal_now || reveal_on_hover;

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
                .when(chrome_visible, |this| {
                    let scrim = div().absolute().inset_0().bg(hsla(0., 0., 0., 0.28));
                    this.child(if reveal_on_hover {
                        scrim
                            .opacity(0.)
                            .group_hover("media-cover", |this| this.opacity(1.))
                    } else {
                        scrim
                    })
                })
                .when(chrome_visible && preview.is_none(), |this| {
                    let play = div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
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
                        );
                    this.child(if reveal_on_hover {
                        play.opacity(0.)
                            .group_hover("media-cover", |this| this.opacity(1.))
                    } else {
                        play
                    })
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
                .when(selected, |this| {
                    this.child(selection_outline(style.primary, style.radius, active))
                })
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
                    video_title(title_id, title.to_owned(), title_lines)
                        .text_color(style.foreground)
                        .cursor_pointer()
                        .hover(|this| this.text_color(style.primary))
                        .on_click(navigate(on_open.clone())),
                )
                .when_some(footer, |this, footer| {
                    this.child(
                        video_meta(meta_id, footer.to_owned(), footer_icon)
                            .text_color(style.muted)
                            .cursor_pointer()
                            .hover(|this| this.text_color(style.primary))
                            .on_click(meta_click),
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
    frame: super::CardFrame,
    view: CardView,
    window: &mut Window,
    cx: &mut Context<V>,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + 'static,
{
    let CardView {
        chrome,
        selected,
        active,
    } = view;
    let super::CardFrame { style, metrics } = frame;
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
            chrome,
            selected,
            active,
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
    view: CardRowView,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + EventEmitter<OpenVideo> + 'static,
{
    let super::CardFrame { style, metrics } = view.frame;
    let start = row * columns;
    let end = (start + columns).min(cards.len());

    h_flex()
        .h(metrics.height())
        .gap(CARD_GAP)
        .children((start..end).map(|index| {
            video_card_cell(
                &cards[index],
                style,
                metrics,
                view.selection == Some(index),
                view.active,
                window,
                cx,
            )
        }))
        .into_any_element()
}

pub(crate) fn video_card_row_indexed<V>(
    cards: &[VideoCard],
    indices: &[usize],
    row: usize,
    columns: usize,
    view: CardRowView,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + EventEmitter<OpenVideo> + 'static,
{
    let super::CardFrame { style, metrics } = view.frame;
    let start = row * columns;
    let end = (start + columns).min(indices.len());

    h_flex()
        .h(metrics.height())
        .gap(CARD_GAP)
        .children((start..end).map(|index| {
            video_card_cell(
                &cards[indices[index]],
                style,
                metrics,
                view.selection == Some(index),
                view.active,
                window,
                cx,
            )
        }))
        .into_any_element()
}

fn video_card_cell<V>(
    card: &VideoCard,
    style: CardStyle,
    metrics: CardMetrics,
    selected: bool,
    active: bool,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EventEmitter<OpenSpace> + EventEmitter<OpenVideo> + 'static,
{
    let open = card.open();
    let on_open = cx.listener(move |_, event: &ClickEvent, _, cx| {
        let mut open = open.clone();
        open.new_tab = rigili_core::opens_new_tab(event);
        cx.emit(open);
    });
    video_card(
        card,
        VideoCardKind::Default,
        super::CardFrame { style, metrics },
        CardView {
            chrome: CoverChrome::OnHover,
            selected,
            active,
        },
        window,
        cx,
        on_open,
    )
}

pub(crate) fn live_card(
    card: &LiveCard,
    frame: super::CardFrame,
    footer_icon: Option<IconName>,
    view: CardView,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let super::CardFrame { style, metrics } = frame;
    let CardView {
        chrome,
        selected,
        active,
    } = view;
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
            chrome,
            selected,
            active,
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
    frame: super::CardFrame,
    footer_icon: Option<IconName>,
    view: CardView,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let super::CardFrame { style, metrics } = frame;
    let CardView {
        chrome,
        selected,
        active,
    } = view;
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
            chrome,
            selected,
            active,
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

pub(crate) fn with_opening_overlay(
    card: AnyElement,
    width: Pixels,
    height: Pixels,
    opening: bool,
) -> AnyElement {
    if !opening {
        return card;
    }

    div()
        .relative()
        .w(width)
        .h(height)
        .child(card)
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0., 0.45))
                .child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .child(Spinner::new().small())
                        .child(div().text_xs().text_color(Hsla::white()).child("正在打开…")),
                ),
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
