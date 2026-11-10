use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SearchView;
use super::cards::{
    level_badge, preview_card, preview_card_width, preview_columns, user_block_height,
};
use super::items::UserItem;
use crate::grid::CardStyle;

impl SearchView {
    pub(super) fn render_user_block(&self, user: &UserItem, cx: &mut Context<Self>) -> AnyElement {
        let style = CardStyle::new(cx);
        let width = self.user_content_width();
        let columns = preview_columns(width);
        let card_width = preview_card_width(width, columns);
        let mid = user.mid;

        v_flex()
            .w_full()
            .h(user_block_height(width))
            .overflow_hidden()
            .p_4()
            .gap_4()
            .rounded(style.radius())
            .border_1()
            .border_color(style.border())
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
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
                                            .text_lg()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(style.foreground())
                                            .child(user.name.clone()),
                                    )
                                    .child(level_badge(user.level, style)),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(style.muted())
                                    .child(format!("粉丝: {}", user.fans))
                                    .child("·")
                                    .child(format!("视频: {}", user.videos))
                                    .child(div().ml_2().truncate().child(user.description.clone())),
                            ),
                    )
                    .child(
                        Button::new("search-user-follow")
                            .small()
                            .primary()
                            .icon(IconName::Plus)
                            .label("关注")
                            .tooltip("暂不支持关注"),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .children(user.preview.iter().take(columns).map(|video| {
                        let open = video.open();
                        let on_open =
                            cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(open.clone()));
                        preview_card(video, style, card_width, on_open)
                    })),
            )
            .child(
                h_flex().justify_center().child(
                    Button::new("search-user-videos")
                        .text()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.open_url(&format!("https://space.bilibili.com/{mid}"));
                        }))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .text_sm()
                                .text_color(style.primary())
                                .child("查看TA的所有稿件")
                                .child(Icon::new(IconName::ChevronRight).with_size(px(14.))),
                        ),
                ),
            )
            .into_any_element()
    }
}
