use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::thumbnail;

use super::items::{ArticleItem, LiveItem, MediaItem, UserItem};
use crate::grid::{
    CardMetrics, CardOverlay, CardStyle, MediaAuthor, MediaCard as CardLayout, VideoCard,
};
use crate::platform::theme::cover_scrim;

pub(crate) const ROW_HEIGHT: Pixels = px(96.);

const PREVIEW_GAP: Pixels = px(12.);
const PREVIEW_MIN_WIDTH: f32 = 150.;
const PREVIEW_MAX_COLUMNS: usize = 5;
const PREVIEW_TEXT_HEIGHT: f32 = 22.;
const USER_BLOCK_CHROME: f32 = 156.;

pub(crate) fn video_card(
    video: &VideoCard,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_author: Option<MediaAuthor>,
) -> AnyElement {
    crate::grid::media_card(
        CardLayout {
            id: SharedString::from(video.bvid().to_owned()),
            cover: video.cover(),
            title: video.title(),
            footer: video.meta(),
            footer_icon: Some(IconName::MonitorPlay),
            author: on_author,
            overlay: CardOverlay {
                leading: Some((IconName::Play, video.views())),
                secondary: Some((Some(IconName::ThumbsUp), video.like())),
                trailing: Some(video.duration()),
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

pub(crate) fn media_item_card(
    item: &MediaItem,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    crate::grid::media_card(
        CardLayout {
            id: SharedString::from(format!("search-pgc-{}", item.season_id)),
            cover: &item.cover,
            title: &item.title,
            footer: &item.subtitle,
            footer_icon: Some(IconName::Tv),
            author: None,
            overlay: CardOverlay {
                leading: (!item.score.is_empty()).then_some((IconName::Star, item.score.as_str())),
                secondary: None,
                trailing: (!item.index_show.is_empty()).then_some(item.index_show.as_str()),
                badge: item.badge.as_deref(),
            },
        },
        style,
        metrics.width,
        metrics.text_height,
        on_open,
    )
    .into_any_element()
}

pub(crate) fn live_card(
    item: &LiveItem,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    crate::grid::media_card(
        CardLayout {
            id: SharedString::from(format!("search-live-{}", item.room_id)),
            cover: &item.cover,
            title: &item.title,
            footer: &item.meta,
            footer_icon: Some(IconName::Radio),
            author: None,
            overlay: CardOverlay {
                leading: Some((IconName::Eye, &item.online)),
                secondary: None,
                trailing: Some("直播中"),
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

pub(crate) fn article_row(
    article: &ArticleItem,
    style: CardStyle,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("search-article-{}", article.id)))
        .h(ROW_HEIGHT)
        .w_full()
        .items_center()
        .gap_4()
        .p_3()
        .rounded(style.radius())
        .border_1()
        .border_color(style.border())
        .cursor_pointer()
        .hover(move |this| this.bg(style.hover()))
        .on_click(on_open)
        .child(
            div()
                .w(px(128.))
                .h(px(72.))
                .flex_shrink_0()
                .rounded(style.radius())
                .overflow_hidden()
                .bg(style.placeholder())
                .child(
                    img(thumbnail(&article.cover, 320))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(style.foreground())
                        .line_clamp(1)
                        .child(article.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted())
                        .line_clamp(2)
                        .child(article.summary.clone()),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .text_xs()
                        .text_color(style.muted())
                        .child(stat(IconName::Eye, &article.views))
                        .child(stat(IconName::ThumbsUp, &article.likes))
                        .child(stat(IconName::MessageCircle, &article.replies))
                        .child(div().flex_1())
                        .child(article.meta.clone()),
                ),
        )
        .into_any_element()
}

pub(crate) fn user_row(
    user: &UserItem,
    style: CardStyle,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("search-user-{}", user.mid)))
        .h(ROW_HEIGHT)
        .w_full()
        .items_center()
        .gap_4()
        .px_4()
        .rounded(style.radius())
        .border_1()
        .border_color(style.border())
        .cursor_pointer()
        .hover(move |this| this.bg(style.hover()))
        .on_click(on_open)
        .child(
            Avatar::new()
                .src(user.face.clone())
                .name(user.name.clone())
                .size(px(64.)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(style.foreground())
                                .child(user.name.clone()),
                        )
                        .child(level_badge(user.level, style)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted())
                        .child(format!("粉丝: {} · 视频: {}", user.fans, user.videos)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(style.muted())
                        .truncate()
                        .child(user.description.clone()),
                ),
        )
        .child(
            Icon::new(IconName::ChevronRight)
                .with_size(px(16.))
                .text_color(style.muted()),
        )
        .into_any_element()
}

pub(crate) fn level_badge(level: u32, style: CardStyle) -> AnyElement {
    div()
        .px_1()
        .rounded(px(3.))
        .bg(style.primary().opacity(0.12))
        .text_color(style.primary())
        .text_xs()
        .child(format!("LV{level}"))
        .into_any_element()
}

pub(crate) fn preview_columns(width: Pixels) -> usize {
    if width <= px(0.) {
        return 1;
    }
    let gap = PREVIEW_GAP.as_f32();
    (((width.as_f32() + gap) / (PREVIEW_MIN_WIDTH + gap)).floor() as usize)
        .clamp(1, PREVIEW_MAX_COLUMNS)
}

pub(crate) fn preview_card_width(width: Pixels, columns: usize) -> Pixels {
    if width <= px(0.) || columns == 0 {
        return px(PREVIEW_MIN_WIDTH);
    }
    let gaps = PREVIEW_GAP.as_f32() * (columns - 1) as f32;
    px(((width.as_f32() - gaps) / columns as f32).max(0.))
}

pub(crate) fn preview_card_height(width: Pixels) -> Pixels {
    px(width.as_f32() * 9. / 16.) + px(PREVIEW_TEXT_HEIGHT + 8.)
}

pub(crate) fn user_block_height(width: Pixels) -> Pixels {
    let columns = preview_columns(width);
    let card_width = preview_card_width(width, columns);
    px(USER_BLOCK_CHROME) + preview_card_height(card_width)
}

pub(crate) fn preview_card(
    video: &VideoCard,
    style: CardStyle,
    width: Pixels,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    v_flex()
        .id(SharedString::from(format!("preview-{}", video.bvid())))
        .w(width)
        .gap_2()
        .cursor_pointer()
        .on_click(on_open)
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(style.radius())
                .overflow_hidden()
                .bg(style.placeholder())
                .child(
                    img(thumbnail(video.cover(), 320))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(
                    h_flex()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .pt_3()
                        .pb_1()
                        .text_xs()
                        .text_color(Hsla::white())
                        .bg(cover_scrim())
                        .child(Icon::new(IconName::Play))
                        .child(video.views().to_owned())
                        .child(div().flex_1())
                        .child(video.duration().to_owned()),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(style.foreground())
                .line_clamp(1)
                .child(video.title().to_owned()),
        )
        .into_any_element()
}

fn stat(icon: IconName, text: &str) -> AnyElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).with_size(px(12.)))
        .child(text.to_owned())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        PREVIEW_MAX_COLUMNS, preview_card_height, preview_card_width, preview_columns,
        user_block_height,
    };
    use gpui_kit::px;

    #[test]
    fn fits_preview_cards_into_the_available_width() {
        assert_eq!(preview_columns(px(0.)), 1);
        assert_eq!(preview_columns(px(300.)), 1);
        assert_eq!(preview_columns(px(1000.)), PREVIEW_MAX_COLUMNS);

        let width = px(1_000.);
        let columns = preview_columns(width);
        let card_width = preview_card_width(width, columns);
        assert!(card_width > px(0.));
        assert!(preview_card_height(card_width) > px(card_width.as_f32() * 9. / 16.));
        assert!(user_block_height(width) > preview_card_height(card_width));
    }
}
