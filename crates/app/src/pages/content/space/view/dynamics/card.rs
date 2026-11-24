use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{DynamicArchive, DynamicItem, thumbnail};
use rigili_core::{OpenVideo, format_count};

use super::super::SpaceView;

impl SpaceView {
    pub(super) fn render_dynamic_card(
        &self,
        item: &DynamicItem,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let radius_lg = cx.theme().radius_lg;
        let card_bg = cx.theme().muted.opacity(0.25);
        let author = item.author();
        let text = item.text();
        let images = item.images();
        let archive = item.archive();
        let tip = item.forward_tip().map(str::to_owned);
        let forward = item.forward();
        let stat = item.stat();
        let comment_count = stat.map(|stat| stat.comment.count).unwrap_or(0);
        let like_count = stat.map(|stat| stat.like.count).unwrap_or(0);
        let key = comment_key(item);
        let expanded = self.expanded_comments.as_deref() == Some(key.as_str());
        let is_trigger = self.comment_trigger_key.as_deref() == Some(key.as_str());
        let comment_color = if expanded { primary } else { muted };
        let on_comments = {
            let dynamic = item.clone();
            let key = key.clone();
            cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.toggle_comments(key.clone(), dynamic.clone(), window, cx);
            })
        };

        v_flex()
            .id(("space-dynamic", index))
            .gap_3()
            .p_4()
            .rounded(radius_lg)
            .border_1()
            .border_color(border)
            .bg(card_bg)
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Avatar::new()
                            .src(author.face.clone())
                            .name(author.name.clone())
                            .size(px(36.)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(primary)
                                    .child(author.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(author.pub_time.clone()),
                            ),
                    )
                    .when(!author.pub_action.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(author.pub_action.clone()),
                        )
                    }),
            )
            .when(!text.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .line_height(relative(1.6))
                        .child(text.to_owned()),
                )
            })
            .when_some(images.first().copied(), |this, image| {
                this.child(dynamic_image(image, images.len(), radius))
            })
            .when_some(archive, |this, archive| {
                this.child(self.render_dynamic_archive(archive, index, cx))
            })
            .when_some(tip.clone(), |this, tip| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .p_3()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(Icon::new(IconName::CircleAlert).with_size(px(16.)))
                        .child(tip),
                )
            })
            .when(forward.is_some() && tip.is_none(), |this| {
                if let Some(forward) = forward {
                    this.child(self.render_dynamic_forward(forward, index, cx))
                } else {
                    this
                }
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .pt_3()
                    .text_sm()
                    .text_color(muted)
                    .child(action_item(IconName::Share, "转发".to_owned()))
                    .child(
                        div().flex_1().flex().justify_center().child(
                            Button::new(("space-dynamic-comments", index))
                                .text()
                                .small()
                                .px_2()
                                .when(is_trigger, |button| {
                                    button.track_focus(&self.comment_trigger_focus)
                                })
                                .accessibility_label(if expanded {
                                    "收起评论"
                                } else {
                                    "查看评论"
                                })
                                .tooltip(if expanded {
                                    "收起评论"
                                } else {
                                    "查看评论"
                                })
                                .on_click(on_comments)
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(comment_color)
                                        .child(
                                            Icon::new(IconName::MessageCircle).with_size(px(16.)),
                                        )
                                        .child(format_count(comment_count)),
                                ),
                        ),
                    )
                    .child(action_item(IconName::ThumbsUp, format_count(like_count))),
            )
            .when(expanded, |this| {
                this.child(
                    v_flex()
                        .id(("space-inline-comments", index))
                        .w_full()
                        .track_focus(&self.comments_focus)
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key.as_str() == "escape" {
                                this.collapse_comments(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .gap_3()
                        .border_t_1()
                        .border_color(border)
                        .pt_4()
                        .child(
                            h_flex().w_full().justify_end().gap_2().child(
                                Button::new(("space-comments-collapse", index))
                                    .small()
                                    .ghost()
                                    .icon(IconName::ChevronUp)
                                    .label("收起")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.collapse_comments(window, cx)
                                    })),
                            ),
                        )
                        .when_some(self.comment_views.get(&key), |this, view| {
                            this.child(view.clone())
                        }),
                )
            })
            .into_any_element()
    }

    fn render_dynamic_archive(
        &self,
        archive: &DynamicArchive,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let open = OpenVideo {
            bvid: archive.bvid.clone(),
            cid: None,
            title: archive.title.clone(),
            new_tab: false,
            episode: None,
        };
        let on_open = cx.listener(move |_, event: &ClickEvent, _, cx| {
            let mut open = open.clone();
            open.new_tab = rigili_core::opens_new_tab(event);
            cx.emit(open);
        });

        h_flex()
            .id(("space-dynamic-archive", index))
            .items_center()
            .gap_3()
            .p_2()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .on_click(on_open)
            .child(
                div()
                    .w(px(160.))
                    .aspect_ratio(16. / 9.)
                    .rounded(radius)
                    .overflow_hidden()
                    .child(
                        img(thumbnail(&archive.cover, 320))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(div().text_sm().line_clamp(2).child(archive.title.clone()))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::Play)
                                    .child(archive.stat.play.clone()),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::MessageCircle)
                                    .child(archive.stat.danmaku.clone()),
                            )
                            .when(!archive.duration_text.is_empty(), |this| {
                                this.child(archive.duration_text.clone())
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_dynamic_forward(
        &self,
        forward: &DynamicItem,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = cx.theme().primary;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let author = forward.author();
        let name = if author.name.is_empty() {
            "转发动态".to_owned()
        } else {
            author.name.clone()
        };

        v_flex()
            .id(("space-dynamic-forward", index))
            .gap_2()
            .p_3()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .child(div().text_xs().text_color(primary).child(name))
            .when(!forward.text().is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .line_height(relative(1.5))
                        .child(forward.text().to_owned()),
                )
            })
            .when_some(forward.archive(), |this, archive| {
                this.child(self.render_dynamic_archive(archive, index, cx))
            })
            .into_any_element()
    }
}

fn comment_key(item: &DynamicItem) -> String {
    let (kind, oid) = item.comment_target();
    format!("{kind}:{oid}")
}

fn dynamic_image(image: &str, count: usize, radius: Pixels) -> AnyElement {
    div()
        .w_full()
        .aspect_ratio(16. / 9.)
        .relative()
        .rounded(radius)
        .overflow_hidden()
        .child(
            img(thumbnail(image, 720))
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .when(count > 1, |this| {
            this.child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .px_2()
                    .py_1()
                    .rounded(radius)
                    .bg(hsla(0., 0., 0., 0.55))
                    .text_xs()
                    .text_color(Hsla::white())
                    .child(format!("共 {count} 张")),
            )
        })
        .into_any_element()
}

fn action_item(icon: IconName, count: String) -> AnyElement {
    h_flex()
        .flex_1()
        .justify_center()
        .items_center()
        .gap_2()
        .child(icon)
        .child(count)
        .into_any_element()
}
