use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{RecommendVideo, VideoPage, thumbnail};
use rigili_core::{format_count, format_duration};

use super::favorite::FavoriteDialog;
use super::{InfoEvent, InfoPanel, RELATED_THUMB_HEIGHT, RELATED_THUMB_WIDTH, VideoDetails};

impl InfoPanel {
    fn toggle_description(&mut self, cx: &mut Context<Self>) {
        self.description_expanded = !self.description_expanded;
        cx.notify();
    }

    fn render_pages(&self, details: &VideoDetails, cx: &mut Context<Self>) -> AnyElement {
        let selected = details
            .pages
            .iter()
            .position(|page| Some(page.cid) == self.selected_cid);
        let count = match selected {
            Some(index) => format!("{}/{}", index + 1, details.pages.len()),
            None => format!("共 {} P", details.pages.len()),
        };
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;
        let weak = cx.entity().downgrade();

        let rows = details.pages.iter().map(|page| {
            let cid = page.cid;
            let current = Some(cid) == self.selected_cid;
            let label = short_label(page);
            let duration = format_duration(page.duration);
            let panel = weak.clone();

            ListItem::new(("page", cid))
                .text_sm()
                .px_2()
                .py_1()
                .rounded(radius)
                .selected(current)
                .disabled(self.busy)
                .accessibility_label(label.clone())
                .on_click(move |_, _, cx| {
                    let _ = panel.update(cx, |_, cx| cx.emit(InfoEvent::SelectPage(cid)));
                })
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(label))
                        .child(div().text_xs().text_color(muted).child(duration)),
                )
        });

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("选集"),
                    )
                    .child(div().text_xs().text_color(muted).child(count)),
            )
            .child(v_flex().gap_1().children(rows))
            .into_any_element()
    }

    pub(super) fn render_info_tab(
        &self,
        details: &VideoDetails,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let link = theme.link;
        let link_hover = theme.link_hover;
        let danger = theme.danger;

        let weak = cx.entity().downgrade();
        let aid = details.aid;
        let owner_mid = details.owner_mid;
        let followed = self.followed;
        let liked = self.liked;
        let like_busy = self.like_busy;
        let coins_given = self.coins_given;
        let coin_busy = self.coin_busy;
        let favorited = self.favorited;
        let share_url = format!("https://www.bilibili.com/video/{}", details.bvid);

        let owner_meta: SharedString = match (self.followers, self.owner_likes) {
            (Some(followers), Some(likes)) => {
                let followers = format_count(followers);
                let likes = format_count(likes);
                format!("{followers}粉丝 · {likes}点赞").into()
            }
            (Some(followers), None) => format!("{}粉丝", format_count(followers)).into(),
            _ => format!("UID {owner_mid}").into(),
        };

        let follow_button = if followed {
            Button::new("follow").ghost()
        } else {
            Button::new("follow").primary()
        }
        .small()
        .icon(if followed {
            IconName::Check
        } else {
            IconName::Plus
        })
        .label(if followed { "已关注" } else { "关注" })
        .disabled(self.follow_busy)
        .on_click({
            let weak = weak.clone();
            move |_, _, cx| {
                let _ = weak.update(cx, |_, cx| {
                    cx.emit(InfoEvent::ToggleFollow {
                        mid: owner_mid,
                        follow: !followed,
                    });
                });
            }
        });

        let has_description = !details.description.is_empty();
        let expanded = (has_description && self.description_expanded).then(|| {
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(details.bvid.clone())
                        .when(details.no_reprint, |this| {
                            this.child(Icon::new(IconName::Ban).size(px(12.)).text_color(danger))
                                .child("未经作者授权，禁止转载")
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(details.description.clone()),
                )
        });

        let pages = (details.pages.len() > 1).then(|| self.render_pages(details, cx));

        let related = (!self.related.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .children(self.related.iter().enumerate().map(|(index, video)| {
                    let bvid = video.bvid.clone();
                    let cid = video.cid;
                    let title = video.title.clone();
                    let weak = weak.clone();
                    related_row(index, video, cx, move |_, _, cx| {
                        let _ = weak.update(cx, |_, cx| {
                            cx.emit(InfoEvent::PlayRelated {
                                bvid: bvid.clone(),
                                cid,
                                title: title.clone(),
                            });
                        });
                    })
                }))
        });

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("player-info-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        v_flex()
                            .gap_4()
                            .p_4()
                            .pb_6()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Avatar::new()
                                            .src(details.avatar.clone())
                                            .name(details.owner.clone())
                                            .size(px(40.)),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .truncate()
                                                    .child(details.owner.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .truncate()
                                                    .child(owner_meta),
                                            ),
                                    )
                                    .child(follow_button),
                            )
                            .child(
                                h_flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_base()
                                            .font_weight(FontWeight::MEDIUM)
                                            .line_clamp(2)
                                            .child(details.title.clone()),
                                    )
                                    .when(has_description, |this| {
                                        this.child(
                                            h_flex()
                                                .id("toggle-description")
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(link)
                                                .hover(move |this| this.text_color(link_hover))
                                                .child(if self.description_expanded {
                                                    "收起"
                                                } else {
                                                    "展开"
                                                })
                                                .child(
                                                    Icon::new(if self.description_expanded {
                                                        IconName::ChevronUp
                                                    } else {
                                                        IconName::ChevronDown
                                                    })
                                                    .size(px(12.)),
                                                )
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak.update(cx, |this, cx| {
                                                            this.toggle_description(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_4()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(stat(IconName::Play, details.views.clone()))
                                    .child(stat(IconName::MessageCircle, details.danmaku.clone()))
                                    .child(stat(IconName::Calendar, details.pubdate.clone())),
                            )
                            .when_some(expanded, |this, expanded| this.child(expanded))
                            .when_some(self.refresh_error.clone(), |this, error| {
                                this.child(div().text_xs().text_color(danger).child(error))
                            })
                            .child(
                                h_flex()
                                    .w_full()
                                    .items_stretch()
                                    .child(
                                        action_button(
                                            ("like", aid),
                                            IconName::ThumbsUp,
                                            format_count(self.like_count).into(),
                                            liked,
                                            self.like_busy,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                if like_busy {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |_, cx| {
                                                    cx.emit(InfoEvent::ToggleLike {
                                                        aid,
                                                        like: !liked,
                                                    });
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("coin", aid),
                                            IconName::Coins,
                                            format_count(self.coin_count).into(),
                                            coins_given > 0,
                                            self.coin_busy,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                if coins_given >= 2 || coin_busy {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |_, cx| {
                                                    cx.emit(InfoEvent::AddCoin { aid });
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("favorite", aid),
                                            IconName::Star,
                                            format_count(self.favorite_count).into(),
                                            favorited,
                                            false,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    let Some(details) = this.details.clone() else {
                                                        return;
                                                    };
                                                    let panel = cx.entity();
                                                    let content = cx.new(|cx| {
                                                        FavoriteDialog::new(
                                                            window,
                                                            cx,
                                                            details.aid,
                                                            panel,
                                                        )
                                                    });
                                                    window.open_dialog(cx, move |dialog, _, _| {
                                                        dialog
                                                            .w(px(360.))
                                                            .title("选择收藏夹")
                                                            .child(content.clone())
                                                    });
                                                });
                                            }
                                        }),
                                    )
                                    .child(action_button(
                                        ("cache", aid),
                                        IconName::Download,
                                        "缓存".into(),
                                        false,
                                        true,
                                        cx,
                                    ))
                                    .child(
                                        action_button(
                                            ("share", aid),
                                            IconName::Share2,
                                            format_count(self.share_count).into(),
                                            false,
                                            false,
                                            cx,
                                        )
                                        .on_click({
                                            let url = share_url.clone();
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    url.clone(),
                                                ));
                                            }
                                        }),
                                    ),
                            )
                            .child(div().h(px(1.)).w_full().bg(border))
                            .when_some(pages, |this, pages| this.child(pages))
                            .when_some(related, |this, related| this.child(related)),
                    ),
            )
            .child(Scrollbar::vertical(&self.scroll))
            .into_any_element()
    }
}

fn stat(icon: IconName, value: SharedString) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).size(px(14.)))
        .child(value)
}

fn action_button(
    id: (&'static str, u64),
    icon: IconName,
    label: SharedString,
    active: bool,
    dimmed: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let color = if active {
        theme.primary
    } else {
        theme.muted_foreground
    };
    v_flex()
        .id(id)
        .flex_1()
        .items_center()
        .gap_1()
        .py_2()
        .rounded(theme.radius)
        .cursor_pointer()
        .opacity(if dimmed { 0.45 } else { 1. })
        .when(!dimmed, |this| {
            this.hover(move |this| this.bg(theme.accent))
        })
        .child(Icon::new(icon).size(px(22.)).text_color(color))
        .child(div().text_xs().text_color(color).child(label))
}

fn related_row(
    index: usize,
    video: &RecommendVideo,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let radius = theme.radius;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let cover = thumbnail(&video.pic, 240);
    let title: SharedString = video.title.clone().into();
    let owner: SharedString = video.owner.name.clone().into();
    let views: SharedString = format_count(video.stat.view).into();
    let danmaku: SharedString = format_count(video.stat.danmaku).into();
    let duration = format_duration(video.duration);

    h_flex()
        .id(("related", index))
        .w_full()
        .items_start()
        .gap_2()
        .p_1()
        .rounded(radius)
        .cursor_pointer()
        .hover(move |this| this.bg(accent))
        .on_click(on_click)
        .child(
            div()
                .relative()
                .w(RELATED_THUMB_WIDTH)
                .h(RELATED_THUMB_HEIGHT)
                .flex_shrink_0()
                .rounded(radius)
                .overflow_hidden()
                .bg(muted.opacity(0.12))
                .child(img(cover).size_full().object_fit(ObjectFit::Cover))
                .child(
                    div()
                        .absolute()
                        .right_1()
                        .bottom_1()
                        .px_1()
                        .rounded(px(2.))
                        .bg(hsla(0., 0., 0., 0.65))
                        .text_size(px(10.))
                        .text_color(Hsla::white())
                        .child(duration),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(div().text_sm().line_clamp(2).child(title))
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            div()
                                .px(px(3.))
                                .rounded(px(2.))
                                .border_1()
                                .border_color(muted.opacity(0.4))
                                .text_size(px(9.))
                                .child("UP"),
                        )
                        .child(div().min_w_0().truncate().child(owner)),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(stat(IconName::Play, views))
                        .child(stat(IconName::MessageCircle, danmaku)),
                ),
        )
}

fn short_label(page: &VideoPage) -> String {
    if page.part.is_empty() {
        format!("P{}", page.page)
    } else {
        format!("P{} · {}", page.page, page.part)
    }
}

#[cfg(test)]
mod tests {
    use super::short_label;
    use rigili_bilibili::VideoPage;

    fn page(number: u32, part: &str) -> VideoPage {
        VideoPage {
            cid: 42,
            page: number,
            part: part.into(),
            duration: 0,
        }
    }

    #[test]
    fn labels_pages() {
        assert_eq!(short_label(&page(2, "第二局")), "P2 · 第二局");
        assert_eq!(short_label(&page(1, "")), "P1");
    }
}
