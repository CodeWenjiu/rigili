use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::thumbnail;

use super::items::{ArticleItem, UserItem};
use crate::shared::browse::grid::{
    CardGrid, CardOverlay, CardStyle, CoverChrome, MediaCard as CardLayout, MediaVideo, VideoCard,
    VideoCards, card_height,
};

pub(crate) const ROW_HEIGHT: Pixels = px(96.);

const PREVIEW_TEXT_HEIGHT: f32 = 22.;
const USER_BLOCK_CHROME: f32 = 156.;

pub(crate) fn article_row(
    article: &ArticleItem,
    style: CardStyle,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    h_flex()
        .id(ElementId::NamedInteger("search-article".into(), article.id))
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
        .id(ElementId::NamedInteger("search-user".into(), user.mid))
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
    crate::shared::ui::profile::level_badge(level, style.primary().opacity(0.12), style.primary())
}

pub(crate) fn user_block_height(width: Pixels) -> Pixels {
    let columns = VideoCards::columns(width);
    let card_width = VideoCards::card_width(width, columns, 1.);
    px(USER_BLOCK_CHROME) + card_height(card_width, px(PREVIEW_TEXT_HEIGHT))
}

pub(crate) fn preview_card(
    video: &VideoCard,
    style: CardStyle,
    width: Pixels,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    crate::shared::browse::grid::media_card(
        CardLayout {
            id: video.id().clone(),
            cover: video.cover(),
            title: video.title(),
            footer: None,
            footer_icon: None,
            title_lines: 1,
            thumb_width: 480,
            author: None,
            video: Some(MediaVideo {
                bvid: video.id().clone(),
                cid: video.cid(),
            }),
            chrome: CoverChrome::OnHover,
            overlay: CardOverlay {
                leading: Some((IconName::Play, video.views())),
                secondary: None,
                trailing: Some(video.duration()),
                badge: None,
            },
        },
        style,
        width,
        px(PREVIEW_TEXT_HEIGHT),
        window,
        cx,
        on_open,
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
    use super::{PREVIEW_TEXT_HEIGHT, USER_BLOCK_CHROME, user_block_height};
    use crate::shared::browse::grid::{CardGrid, VideoCards, card_height};
    use gpui_kit::px;

    #[test]
    fn fits_the_preview_grid_inside_the_user_block() {
        let width = px(1_000.);
        let columns = VideoCards::columns(width);
        let card_width = VideoCards::card_width(width, columns, 1.);
        let row = card_width.as_f32() * columns as f32
            + VideoCards::GAP.as_f32() * columns.saturating_sub(1) as f32;

        assert!(row <= width.as_f32());
        assert_eq!(
            user_block_height(width),
            px(USER_BLOCK_CHROME) + card_height(card_width, px(PREVIEW_TEXT_HEIGHT))
        );
    }
}
