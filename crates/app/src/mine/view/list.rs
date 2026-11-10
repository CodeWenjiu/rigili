use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{FavoriteView, MineTab, MineView};
use crate::format::format_ymd;
use crate::grid::VideoCard;
use crate::grid::{
    CARD_GAP, CardMetrics, CardOverlay, CardStyle, LIST_FOOTER_HEIGHT, MediaCard, card_text_height,
    card_width_for_width, columns_for_width, list_footer, media_card, skeleton_card,
};
use crate::platform::theme::cover_scrim;
use rigili_bilibili::{FavFolder, thumbnail};

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
            let columns = columns_for_width(self.list_width);
            let metrics = CardMetrics::new(
                card_width_for_width(self.list_width, columns),
                card_text_height(window),
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

    pub(super) fn render_folder_grid(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;

        if self.folders.loading && self.folders.folders.is_empty() {
            let columns = columns_for_width(self.list_width).max(2);
            let width = card_width_for_width(self.list_width, columns);
            return div()
                .id("mine-folder-grid-loading")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_x(CARD_GAP)
                        .gap_y_6()
                        .children((0..columns * 2).map(|_| skeleton_card(width, radius))),
                )
                .into_any_element();
        }

        if let Some(error) = self.folders.error.clone() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("mine-folders-retry")
                        .primary()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load_folders(cx))),
                )
                .into_any_element();
        }

        let folders = self
            .folders
            .folders
            .iter()
            .filter(|folder| !folder.is_default())
            .collect::<Vec<_>>();
        if folders.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("暂无收藏夹")
                .into_any_element();
        }

        let columns = columns_for_width(self.list_width).max(1);
        let width = card_width_for_width(self.list_width, columns);
        let known_width = self.list_width;
        let weak = cx.entity().downgrade();

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
            .child(
                h_flex().flex_wrap().gap_x(CARD_GAP).gap_y_6().children(
                    folders
                        .into_iter()
                        .enumerate()
                        .map(|(index, folder)| folder_card(folder, index, width, cx)),
                ),
            )
            .into_any_element()
    }

    fn render_row(
        &self,
        tab: MineTab,
        row: usize,
        columns: usize,
        metrics: CardMetrics,
        style: CardStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                video_card(card, style, metrics, on_open)
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
        let state = self.state(tab);
        let columns = columns_for_width(self.list_width);
        let metrics = CardMetrics::new(
            card_width_for_width(self.list_width, columns),
            card_text_height(window),
        );
        let row_height = metrics.height();
        let rows = state.items.len().div_ceil(columns);
        let known_width = self.list_width;
        let footer = state.loading_more || state.more_error.is_some();
        let footer_index = rows;

        let mut sizes = vec![size(px(0.), row_height); rows];
        if footer {
            sizes.push(size(px(0.), LIST_FOOTER_HEIGHT));
        }
        let sizes = Rc::new(sizes);

        let view = cx.entity();
        let weak = view.downgrade();
        let list = v_virtual_list(
            view,
            tab.list_id(),
            sizes,
            move |this, visible_range, _, cx| {
                this.maybe_load_more(tab, visible_range.end.saturating_sub(1), rows, cx);
                visible_range
                    .map(|row| {
                        if footer && row == footer_index {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, row, columns, metrics, style, cx)
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
            .child(Scrollbar::vertical(&self.state(tab).scroll).viewport_from_layout())
            .into_any_element()
    }
}

fn video_card(
    card: &VideoCard,
    style: CardStyle,
    metrics: CardMetrics,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let views = card.views();
    let danmaku = card.danmaku();
    let duration = card.duration();

    media_card(
        MediaCard {
            id: SharedString::from(format!("mine-video-{}", card.bvid())),
            cover: card.cover(),
            title: card.title(),
            footer: card.meta(),
            footer_icon: None,
            overlay: CardOverlay {
                leading: (!views.is_empty()).then_some((IconName::Play, views)),
                secondary: (!danmaku.is_empty())
                    .then_some((Some(IconName::MessageCircle), danmaku)),
                trailing: (!duration.is_empty()).then_some(duration),
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

fn folder_card(
    folder: &FavFolder,
    index: usize,
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
    let group = SharedString::from(format!("mine-folder-card-{index}"));

    v_flex()
        .group(group.clone())
        .id(("mine-folder-card", index))
        .w(width)
        .gap_2()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| this.open_folder(id, cx)))
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
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("创建于{created}")),
        )
        .into_any_element()
}
