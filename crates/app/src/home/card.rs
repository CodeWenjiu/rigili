use gpui_kit::assets::IconName;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpuili_bilibili::{PopularVideo, thumbnail};

use crate::format::{format_count, format_date, format_duration};
use crate::platform::theme::cover_scrim;

#[derive(Clone, Copy)]
pub(super) struct CardStyle {
    pub(super) foreground: Hsla,
    pub(super) muted: Hsla,
    pub(super) placeholder: Hsla,
    pub(super) primary: Hsla,
    pub(super) radius: Pixels,
}

impl CardStyle {
    pub(super) fn new(cx: &App) -> Self {
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

pub(super) fn video_card(
    video: &PopularVideo,
    style: CardStyle,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let meta = format!("{} · {}", video.owner.name, format_date(video.pubdate));

    v_flex()
        .group("video-card")
        .w(px(240.))
        .gap_2()
        .id(SharedString::from(video.bvid.clone()))
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
                    img(thumbnail(&video.pic, 480))
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
                        .gap_3()
                        .px_2()
                        .pt_4()
                        .pb_1()
                        .text_xs()
                        .text_color(Hsla::white())
                        .bg(cover_scrim())
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(IconName::Play)
                                .child(format_count(video.stat.view)),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(IconName::MessageCircle)
                                .child(format_count(video.stat.danmaku)),
                        )
                        .child(div().flex_1())
                        .child(format_duration(video.duration)),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_clamp(2)
                .text_color(style.foreground)
                .group_hover("video-card", |this| this.text_color(style.primary))
                .child(video.title.clone()),
        )
        .child(div().text_xs().text_color(style.muted).child(meta))
}

pub(super) fn skeleton_card(radius: Pixels) -> impl IntoElement {
    v_flex()
        .w(px(240.))
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
