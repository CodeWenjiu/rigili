use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{RecommendVideo, VideoPage, avatar, thumbnail};
use rigili_core::video::{VideoCard, video_meta, video_title};
use rigili_core::{format_count, format_duration};

use super::coin::CoinDialog;
use super::favorite::FavoriteDialog;
use super::{
    InfoEvent, InfoPanel, RELATED_THUMB_HEIGHT, RELATED_THUMB_WIDTH, VideoDetails, dialog,
};

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
        let followed = self.follow.active;
        let liked = self.like.active;
        let like_busy = self.like.busy;
        let coins_given = self.coin.given;
        let favorited = self.favorite.active;
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
        .disabled(self.follow.busy)
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
                        .child(
                            div()
                                .cursor(CursorStyle::IBeam)
                                .child(SelectableText::with_handle(
                                    ("info-bvid", aid),
                                    self.selections.bvid.clone(),
                                    details.bvid.clone(),
                                )),
                        )
                        .when(details.no_reprint, |this| {
                            this.child(Icon::new(IconName::Ban).size(px(12.)).text_color(danger))
                                .child(div().cursor(CursorStyle::IBeam).child(
                                    SelectableText::with_handle(
                                        ("info-no-reprint", aid),
                                        self.selections.no_reprint.clone(),
                                        "未经作者授权，禁止转载",
                                    ),
                                ))
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .cursor(CursorStyle::IBeam)
                        .child(SelectableText::with_handle(
                            ("info-description", aid),
                            self.selections.description.clone(),
                            details.description.clone(),
                        )),
                )
        });

        let pages = (details.pages.len() > 1).then(|| self.render_pages(details, cx));

        let related = (!self.related.is_empty()).then(|| {
            v_flex()
                .image_cache(rigili_core::image_cache::related(cx))
                .gap_1()
                .children(self.related.iter().enumerate().map(|(index, video)| {
                    let bvid = video.bvid.clone();
                    let cid = video.cid;
                    let title = video.title.clone();
                    let owner_mid = video.owner.mid;
                    let owner_name = video.owner.name.clone();
                    let play_weak = weak.clone();
                    let space_weak = weak.clone();
                    related_row(
                        index,
                        video,
                        cx,
                        move |_, _, cx| {
                            let _ = play_weak.update(cx, |_, cx| {
                                cx.emit(InfoEvent::PlayRelated {
                                    bvid: bvid.clone(),
                                    cid,
                                    title: title.clone(),
                                });
                            });
                        },
                        move |_, _, cx| {
                            let _ = space_weak.update(cx, |_, cx| {
                                cx.emit(InfoEvent::OpenSpace {
                                    mid: owner_mid,
                                    name: owner_name.clone(),
                                });
                            });
                        },
                    )
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
                                            .src(avatar(&details.avatar, 96))
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
                                                    .id(("info-owner", aid))
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .truncate()
                                                    .cursor_pointer()
                                                    .hover(move |this| this.text_color(link_hover))
                                                    .on_click({
                                                        let weak = weak.clone();
                                                        let name = details.owner.to_string();
                                                        move |_, _, cx| {
                                                            let _ = weak.update(cx, |_, cx| {
                                                                cx.emit(InfoEvent::OpenSpace {
                                                                    mid: owner_mid,
                                                                    name: name.clone(),
                                                                });
                                                            });
                                                        }
                                                    })
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
                                            .cursor(CursorStyle::IBeam)
                                            .child(SelectableText::with_handle(
                                                ("info-title", aid),
                                                self.selections.title.clone(),
                                                details.title.clone(),
                                            )),
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
                                    .child(plain_stat(IconName::Play, details.views.clone()))
                                    .child(plain_stat(
                                        IconName::MessageCircle,
                                        details.danmaku.clone(),
                                    ))
                                    .child(plain_stat(IconName::Calendar, details.pubdate.clone())),
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
                                            format_count(self.like.count).into(),
                                            liked,
                                            self.like.busy,
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
                                            format_count(self.coin.count).into(),
                                            coins_given > 0,
                                            coins_given >= 2,
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                if coins_given >= 2 {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |this, cx| {
                                                    let Some(details) = this.details.clone() else {
                                                        return;
                                                    };
                                                    let panel = cx.entity();
                                                    let given = this.coin.given;
                                                    let liked = this.like.active;
                                                    let content = cx.new(|_| {
                                                        CoinDialog::new(
                                                            window,
                                                            details.aid,
                                                            panel,
                                                            given,
                                                            liked,
                                                        )
                                                    });
                                                    dialog::open(window, cx, "投币", content);
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("favorite", aid),
                                            IconName::Star,
                                            format_count(self.favorite.count).into(),
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
                                                    dialog::open(window, cx, "选择收藏夹", content);
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

fn plain_stat(icon: IconName, value: SharedString) -> impl IntoElement {
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
    on_author_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let radius = theme.radius;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let primary = theme.primary;
    let card = VideoCard::from(video.clone());
    let cover = thumbnail(card.cover(), 240);
    let title = card.title().to_owned();
    let author = card.author().to_owned();
    let author_mid = card.author_mid();
    let duration = card.duration().to_owned();
    let views = card.views().to_owned();
    let danmaku = card.danmaku().to_owned();

    let author_meta = video_meta(("related-author", index as u64), author, None).text_color(muted);
    let author_meta = if author_mid.is_some() {
        author_meta
            .cursor_pointer()
            .hover(move |this| this.text_color(primary))
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                on_author_click(event, window, cx);
            })
    } else {
        author_meta
    };

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
                .child(
                    video_title(("related-title", index as u64), title, 2)
                        .text_color(theme.foreground)
                        .hover(move |this| this.text_color(primary)),
                )
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
                        .child(author_meta),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(plain_stat(IconName::Play, views.into()))
                        .child(plain_stat(IconName::MessageCircle, danmaku.into())),
                ),
        )
}

pub(crate) fn short_label(page: &VideoPage) -> String {
    if page.part.is_empty() {
        format!("P{}", page.page)
    } else {
        format!("P{} · {}", page.page, page.part)
    }
}

#[cfg(test)]
mod tests {
    use super::{SelectableText, short_label};
    use crate::info::InfoSelections;
    use gpui_kit::base::{TextSelection, TextSelectionHandle};
    use gpui_kit::component::*;
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Bounds, Context, CursorStyle, IntoElement, MouseButton, Render, TestAppContext,
        TestSupportExt as _, Window, WindowBounds, WindowOptions, div, point, px, size,
    };
    use rigili_bilibili::VideoPage;

    const BVID: &str = "BV1TgWp6MEHJ";

    struct Fixture {
        selection: TextSelectionHandle,
    }

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            v_flex().p_4().gap_2().child(
                h_flex().text_xs().child(
                    div()
                        .id("bvid-wrap")
                        .cursor(CursorStyle::IBeam)
                        .test_support()
                        .child(SelectableText::with_handle(
                            "bvid",
                            self.selection.clone(),
                            BVID,
                        )),
                ),
            )
        }
    }

    fn page(number: u32, part: &str) -> VideoPage {
        VideoPage {
            cid: 42,
            page: number,
            part: part.into(),
            duration: 0,
        }
    }

    struct GuardFixture {
        selections: InfoSelections,
    }

    impl Render for GuardFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .id("guard-root")
                .p_4()
                .gap_2()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.selections.clear_spanning_in(window, cx);
                    }),
                )
                .child(
                    div()
                        .id("guard-first")
                        .test_support()
                        .child(SelectableText::with_handle(
                            "guard-title-text",
                            self.selections.title.clone(),
                            "alpha",
                        )),
                )
                .child(
                    div()
                        .id("guard-second")
                        .test_support()
                        .child(SelectableText::with_handle(
                            "guard-description-text",
                            self.selections.description.clone(),
                            "beta",
                        )),
                )
        }
    }

    #[gpui_kit::test]
    fn clears_selections_that_span_blocks(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(400.), px(200.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|_| GuardFixture { selections })
                },
            )
            .expect("open guard window")
        });

        let (first, second) = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                (
                    window.find("guard-first").bounds(),
                    window.find("guard-second").bounds(),
                )
            })
            .expect("guard bounds");
        let start = point(
            first.origin.x + px(1.),
            first.origin.y + first.size.height / 2.,
        );
        let end = point(
            second.origin.x + second.size.width - px(1.),
            second.origin.y + second.size.height / 2.,
        );

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, end, cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("guard drag");
        assert_eq!(selected, "");

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, point(start.x + px(20.), start.y), cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("inside drag");
        assert!(!selected.is_empty());
    }

    #[test]
    fn labels_pages() {
        assert_eq!(short_label(&page(2, "第二局")), "P2 · 第二局");
        assert_eq!(short_label(&page(1, "")), "P1");
    }

    #[gpui_kit::test]
    fn selects_a_short_text_run(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(400.), px(200.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selection = TextSelectionHandle::new("", cx);
                    selection.refresh_window_on_change(window, cx).detach();
                    cx.new(|_| Fixture { selection })
                },
            )
            .expect("open selection window")
        });

        let bounds = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.find("bvid-wrap").bounds()
            })
            .expect("bvid bounds");
        let y = bounds.origin.y + bounds.size.height / 2.;
        let start = point(bounds.origin.x + px(1.), y);
        let end = point(bounds.origin.x + bounds.size.width - px(1.), y);

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, end, cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("select bvid");
        assert_eq!(selected, BVID);
    }
}
