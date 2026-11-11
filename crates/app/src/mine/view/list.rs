use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{FavoriteView, FolderSource, MineTab, MineView};

use crate::grid::{
    BangumiCards, CARD_GAP, CardFrame, CardGrid, CardMetrics, CardOverlay, CardStyle, FolderCards,
    LIST_FOOTER_HEIGHT, MediaCard, VideoCards, card_text_height, list_content_size, list_footer,
    media_card, page_gutter, skeleton_card, virtual_list_scrollbar,
};
use crate::platform::theme::cover_scrim;
use gpui_kit::component::spinner::Spinner;
use rigili_bilibili::{BangumiFollow, FavFolder, thumbnail};
use rigili_core::format_ymd;

impl MineView {
    pub(super) fn render_content(
        &mut self,
        tab: MineTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(tab);
        let mut loading = state.loading && state.items.is_empty();
        let mut error = state.error.clone();
        let mut empty = state.items.is_empty() && !state.loading;

        if tab == MineTab::Favorites && self.favorites_view == FavoriteView::Folder {
            if self.folders.loading && state.items.is_empty() {
                loading = true;
                empty = false;
            }
            if state.items.is_empty()
                && let Some(folder_error) = &self.folders.error
            {
                error = Some(folder_error.clone());
                empty = false;
            }
        }

        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;

        if loading {
            let columns = VideoCards::columns(self.list_width);
            let metrics = VideoCards::metrics(
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
            );
            return div()
                .id(("mine-loading", tab.index()))
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
                .into_any_element();
        }

        if let Some(error) = error {
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new(("mine-retry", tab.index()))
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(move |this, _, _, cx| this.reload_tab(tab, cx))),
                )
                .into_any_element();
        }

        if empty {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("暂无内容")
                .into_any_element();
        }

        self.render_list(tab, window, cx)
    }

    pub(super) fn render_folder_grid(
        &mut self,
        source: FolderSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let (folders, loading, error) = match source {
            FolderSource::Created => (
                &self.folders.folders,
                self.folders.loading,
                self.folders.error.clone(),
            ),
            FolderSource::Collected => (
                &self.collected.folders,
                self.collected.loading,
                self.collected.error.clone(),
            ),
        };

        if loading && folders.is_empty() {
            let columns = FolderCards::columns(self.list_width).max(2);
            let width = FolderCards::card_width(self.list_width, columns, window.scale_factor());
            let rows = (0..2)
                .map(|_| {
                    h_flex()
                        .gap_x(CARD_GAP)
                        .children((0..columns).map(|_| skeleton_card(width, radius)))
                        .into_any_element()
                })
                .collect::<Vec<_>>();
            return div()
                .id("mine-folder-grid-loading")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(v_flex().gap_y_6().children(rows))
                .into_any_element();
        }

        if let Some(error) = error {
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("mine-folders-retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.retry_folder_source(source, cx);
                        })),
                )
                .into_any_element();
        }

        let visible = folders
            .iter()
            .filter(|folder| match source {
                FolderSource::Created => !folder.is_default(),
                FolderSource::Collected => folder.is_valid(),
            })
            .collect::<Vec<_>>();
        if visible.is_empty() {
            let empty = match source {
                FolderSource::Created => "暂无收藏夹",
                FolderSource::Collected => "暂无订阅内容",
            };
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child(empty)
                .into_any_element();
        }

        let columns = FolderCards::columns(self.list_width);
        let width = FolderCards::card_width(self.list_width, columns, window.scale_factor());
        let known_width = self.list_width;
        let weak = cx.entity().downgrade();
        let rows = (0..visible.len().div_ceil(columns))
            .map(|row| {
                let start = row * columns;
                let end = (start + columns).min(visible.len());
                h_flex()
                    .gap_x(CARD_GAP)
                    .children(
                        visible[start..end]
                            .iter()
                            .enumerate()
                            .map(|(offset, folder)| {
                                folder_card(folder, start + offset, source, width, cx)
                            }),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .id("mine-folder-grid")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .on_prepaint(move |bounds, window, cx| {
                let width = bounds.size.width;
                if width == known_width {
                    return;
                }
                let weak = weak.clone();
                window.defer(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.list_width != width {
                            this.list_width = width;
                            cx.notify();
                        }
                    });
                });
            })
            .child(v_flex().gap_y_6().children(rows))
            .into_any_element()
    }

    fn retry_folder_source(&mut self, source: FolderSource, cx: &mut Context<Self>) {
        match source {
            FolderSource::Created => self.load_folders(cx),
            FolderSource::Collected => self.load_collected(cx),
        }
    }

    pub(super) fn render_bangumi(
        &mut self,
        cinema: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.bangumi_state(cinema);
        let loading = state.loading && state.items.is_empty();
        let error = state.error.clone();
        let empty = state.items.is_empty() && !state.loading;
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;

        if loading {
            let columns = BangumiCards::columns(self.list_width);
            let metrics = BangumiCards::metrics(
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
            );
            return div()
                .id("mine-bangumi-loading")
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
                .into_any_element();
        }

        if let Some(error) = error {
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("mine-bangumi-retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(move |this, _, _, cx| this.load_bangumi(cinema, cx))),
                )
                .into_any_element();
        }

        if empty {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("暂无内容")
                .into_any_element();
        }

        self.render_bangumi_list(cinema, window, cx)
    }

    fn render_bangumi_row(
        &self,
        cinema: bool,
        row: usize,
        columns: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let CardFrame { style, metrics } = frame;
        let state = self.bangumi_state(cinema);
        let start = row * columns;
        let end = (start + columns).min(state.items.len());
        let opening = self.opening;

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let item = &state.items[index];
                let season_id = item.season_id;
                let is_opening = opening == Some(season_id);
                let on_open = cx.listener(move |this, _, _, cx| {
                    this.open_season(season_id, cx);
                });
                bangumi_card(item, is_opening, style, metrics, window, cx, on_open)
            }))
            .into_any_element()
    }

    fn render_bangumi_footer(&self, cinema: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.bangumi_state(cinema).more_error.clone();
        let retry_id = if cinema {
            ("mine-cinema-retry", 0usize)
        } else {
            ("mine-bangumi-retry", 0usize)
        };

        list_footer(
            retry_id,
            more_error,
            muted,
            danger,
            cx.listener(move |this, _, _, cx| this.retry_bangumi_more(cinema, cx)),
        )
    }

    fn render_bangumi_list(
        &mut self,
        cinema: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = CardStyle::new(cx);
        let gutter = page_gutter(window);
        let state = self.bangumi_state(cinema);
        let columns = BangumiCards::columns(self.list_width);
        let metrics = BangumiCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let frame = CardFrame { style, metrics };
        let row_height = metrics.height();
        let rows = state.items.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = state.loading_more || state.more_error.is_some();
        let footer_index = rows;

        let mut sizes = vec![size(px(0.), row_height); rows];
        if footer {
            sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let content_size = list_content_size(&sizes, rems(1.5).to_pixels(window.rem_size()));
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list_id = if cinema {
            "mine-cinema-list"
        } else {
            "mine-bangumi-list"
        };
        let list = v_virtual_list(
            view,
            list_id,
            sizes,
            move |this, visible_range, window, cx| {
                this.maybe_load_more_bangumi(cinema, visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_bangumi_footer(cinema, cx)
                        } else {
                            this.render_bangumi_row(cinema, row, columns, frame, window, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .gap_y_6()
        .track_scroll(&self.bangumi_state(cinema).scroll);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .mr(px(-gutter.as_f32()))
            .pr(gutter)
            .overflow_hidden()
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(width) = bounds.first().map(|bounds| bounds.size.width) else {
                    return;
                };
                if width == known_width {
                    return;
                }
                let weak = weak.clone();
                // Notifying during prepaint does not schedule another frame.
                window.defer(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.list_width != width {
                            this.list_width = width;
                            cx.notify();
                        }
                    });
                });
            })
            .child(list)
            .child(virtual_list_scrollbar(
                &self.bangumi_state(cinema).scroll,
                content_size,
            ))
            .into_any_element()
    }

    fn render_row(
        &self,
        tab: MineTab,
        row: usize,
        columns: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let CardFrame { style, metrics } = frame;
        let state = self.state(tab);
        let start = row * columns;
        let end = (start + columns).min(state.items.len());

        h_flex()
            .h(metrics.height())
            .gap(CARD_GAP)
            .children((start..end).map(|index| {
                let card = &state.items[index];
                let open = card.open();
                let on_open = cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(open.clone());
                });
                crate::grid::video_card(card, style, metrics, window, cx, on_open)
            }))
            .into_any_element()
    }

    fn render_footer(&self, tab: MineTab, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let more_error = self.state(tab).more_error.clone();

        list_footer(
            ("mine-retry-more", tab.index()),
            more_error,
            muted,
            danger,
            cx.listener(move |this, _, _, cx| this.retry_more(tab, cx)),
        )
    }

    fn render_list(
        &mut self,
        tab: MineTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = CardStyle::new(cx);
        let gutter = page_gutter(window);
        let state = self.state(tab);
        let columns = VideoCards::columns(self.list_width);
        let metrics = VideoCards::metrics(
            self.list_width,
            card_text_height(window),
            window.scale_factor(),
        );
        let frame = CardFrame { style, metrics };
        let row_height = metrics.height();
        let rows = state.items.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = state.loading_more || state.more_error.is_some();
        let footer_index = rows;

        let mut sizes = vec![size(px(0.), row_height); rows];
        if footer {
            sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let content_size = list_content_size(&sizes, rems(1.5).to_pixels(window.rem_size()));
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list = v_virtual_list(
            view,
            tab.list_id(),
            sizes,
            move |this, visible_range, window, cx| {
                this.maybe_load_more(tab, visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, row, columns, frame, window, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            },
        )
        .gap_y_6()
        .track_scroll(&self.state(tab).scroll);

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .mr(px(-gutter.as_f32()))
            .pr(gutter)
            .overflow_hidden()
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(width) = bounds.first().map(|bounds| bounds.size.width) else {
                    return;
                };
                if width == known_width {
                    return;
                }
                let weak = weak.clone();
                // Notifying during prepaint does not schedule another frame.
                window.defer(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.list_width != width {
                            this.list_width = width;
                            cx.notify();
                        }
                    });
                });
            })
            .child(list)
            .child(virtual_list_scrollbar(
                &self.state(tab).scroll,
                content_size,
            ))
            .into_any_element()
    }
}

fn folder_card(
    folder: &FavFolder,
    index: usize,
    source: FolderSource,
    width: Pixels,
    cx: &mut Context<MineView>,
) -> AnyElement {
    let muted_bg = cx.theme().muted;
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let primary = cx.theme().primary;
    let radius = cx.theme().radius;
    let id = folder.id;
    let title = folder.title.clone();
    let cover = folder.cover.clone();
    let count = folder.media_count;
    let private = folder.is_private();
    let created = format_ymd(folder.ctime);
    let footer = match source {
        FolderSource::Created => format!("创建于{created}"),
        FolderSource::Collected => {
            if folder.owner().is_empty() {
                format!("创建于{created}")
            } else {
                format!("来自{}", folder.owner())
            }
        }
    };
    let group = SharedString::from(format!("mine-folder-card-{index}"));

    v_flex()
        .group(group.clone())
        .id(("mine-folder-card", index))
        .w(width)
        .gap_2()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| this.open_folder(id, source, cx)))
        .child(
            div()
                .w_full()
                .aspect_ratio(16. / 9.)
                .relative()
                .rounded(radius)
                .overflow_hidden()
                .bg(muted_bg.opacity(0.6))
                .when(!cover.is_empty(), |this| {
                    this.child(
                        img(thumbnail(&cover, 480))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .when(cover.is_empty(), |this| {
                    this.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Folder)
                                    .with_size(px(32.))
                                    .text_color(muted),
                            ),
                    )
                })
                .child(
                    h_flex()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .items_center()
                        .px_2()
                        .pt_4()
                        .pb_1()
                        .text_xs()
                        .text_color(Hsla::white())
                        .bg(cover_scrim())
                        .child(format!("{count}个内容"))
                        .child(div().mx_1().child("·"))
                        .child(if private { "私密" } else { "公开" }),
                ),
        )
        .child(
            h_flex()
                .items_center()
                .gap_1()
                .when(private, |this| {
                    this.child(
                        Icon::new(IconName::Lock)
                            .with_size(px(12.))
                            .text_color(muted),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(foreground)
                        .group_hover(group, move |this| this.text_color(primary))
                        .child(title),
                ),
        )
        .child(div().text_xs().text_color(muted).child(footer))
        .into_any_element()
}

fn bangumi_card(
    item: &BangumiFollow,
    opening: bool,
    style: CardStyle,
    metrics: CardMetrics,
    window: &mut Window,
    cx: &mut App,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let latest = item.latest_ep();
    let score = item.score().map(|score| format!("{score:.1}"));
    let footer = if item.progress.is_empty() {
        item.season_type_name.as_str()
    } else {
        item.progress.as_str()
    };
    let card = media_card(
        MediaCard {
            id: SharedString::from(format!("mine-bangumi-{}", item.season_id)),
            cover: &item.cover,
            title: &item.title,
            footer: Some(footer),
            footer_icon: None,
            title_lines: 2,
            thumb_width: metrics.thumb_width,
            author: None,
            video: None,
            overlay: CardOverlay {
                leading: score.as_deref().map(|score| (IconName::Star, score)),
                secondary: None,
                trailing: (!latest.is_empty()).then_some(latest),
                badge: (!item.badge.is_empty()).then_some(item.badge.as_str()),
            },
        },
        style,
        metrics.width,
        metrics.text_height,
        window,
        cx,
        on_open,
    );

    if !opening {
        return card.into_any_element();
    }

    div()
        .relative()
        .w(metrics.width)
        .h(metrics.height())
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
