use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{CardRow, FavoriteView, FolderSource, MineTab, MineView};

use crate::app::platform::theme::cover_scrim;
use crate::shared::browse::grid::{
    CARD_GAP, CardFrame, CardGrid, CardRowView, FolderCards, VideoCards, card_grid_loading,
    card_text_height, folder_card_height, selection_outline, skeleton_card, video_card_row,
    video_card_row_indexed,
};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, Rows, paged_footer, virtual_rows_tracked};
use crate::shared::ui::state_view::{Layout, empty_state, error_state};
use rigili_bilibili::{FavFolder, thumbnail};
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
            return card_grid_loading::<VideoCards>(
                ("mine-loading", tab.index()),
                self.list_width,
                card_text_height(window),
                window.scale_factor(),
                radius,
            );
        }

        if let Some(error) = error {
            return error_state(
                Layout::Top,
                error,
                ("mine-retry", tab.index()),
                danger,
                cx.listener(move |this, _, _, cx| this.reload_tab(tab, cx)),
            );
        }

        if empty {
            let message = if self.search.is_empty() {
                "暂无内容"
            } else {
                "未找到匹配内容"
            };
            return empty_state(Layout::Top, muted, message);
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
        let folders = self.folder_items(source);
        let (loading, error) = match source {
            FolderSource::Created => (self.folders.loading, self.folders.error.clone()),
            FolderSource::Collected => (self.collected.loading, self.collected.error.clone()),
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
            return error_state(
                Layout::Top,
                error,
                "mine-folders-retry",
                danger,
                cx.listener(move |this, _, _, cx| {
                    this.retry_folder_source(source, cx);
                }),
            );
        }

        let needle = self.search.needle();
        let indices = self.folder_indices(source);
        if indices.is_empty() {
            let empty = if needle.is_some() {
                match source {
                    FolderSource::Created => "未找到匹配收藏夹",
                    FolderSource::Collected => "未找到匹配订阅",
                }
            } else {
                match source {
                    FolderSource::Created => "暂无收藏夹",
                    FolderSource::Collected => "暂无订阅内容",
                }
            };
            return empty_state(Layout::Top, muted, empty);
        }

        let columns = FolderCards::columns(self.list_width);
        let width = FolderCards::card_width(self.list_width, columns, window.scale_factor());
        let row_height = folder_card_height(width, window);
        let indices = Rc::new(indices);
        let rows = Rows::new(indices.len().div_ceil(columns), row_height);
        let render_indices = indices.clone();
        virtual_rows_tracked(
            cx,
            window,
            (
                "mine-folder-grid",
                matches!(source, FolderSource::Collected) as usize,
            ),
            rows,
            self.list_width,
            &self.folder_scroll,
            move |this, range, window, cx| {
                this.cards.set_first_row(range.start);
                let selection = this.cards.index();
                let active = this.cards.active(window);
                let folders = this.folder_items(source);
                range
                    .map(|row| {
                        let start = row * columns;
                        let end = (start + columns).min(render_indices.len());
                        h_flex()
                            .h(row_height)
                            .gap_x(CARD_GAP)
                            .children(render_indices[start..end].iter().enumerate().map(
                                |(offset, &index)| {
                                    let position = start + offset;
                                    folder_card(
                                        &folders[index],
                                        index,
                                        source,
                                        width,
                                        selection == Some(position),
                                        active,
                                        cx,
                                    )
                                },
                            ))
                            .into_any_element()
                    })
                    .collect()
            },
        )
        .into_any_element()
    }

    fn retry_folder_source(&mut self, source: FolderSource, cx: &mut Context<Self>) {
        match source {
            FolderSource::Created => self.load_folders(cx),
            FolderSource::Collected => self.load_collected(cx),
        }
    }

    fn render_row(
        &self,
        tab: MineTab,
        line: CardRow<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let CardRow {
            filtered,
            row,
            columns,
            frame,
        } = line;
        let state = self.state(tab);
        let view = CardRowView {
            frame,
            selection: self.cards.index(),
            active: self.cards.active(window),
        };
        match filtered {
            Some(indices) => {
                video_card_row_indexed(&state.items, indices, row, columns, view, window, cx)
            }
            None => video_card_row(&state.items, row, columns, view, window, cx),
        }
    }

    fn render_footer(&self, tab: MineTab, cx: &mut Context<Self>) -> AnyElement {
        paged_footer(
            self.state(tab),
            ("mine-retry-more", tab.index()),
            cx,
            move |this, cx| this.retry_more(tab, cx),
        )
    }

    fn render_list(
        &mut self,
        tab: MineTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(tab);
        let filtered = self.video_indices(tab);
        if filtered.as_ref().is_some_and(Vec::is_empty) {
            return empty_state(Layout::Top, cx.theme().muted_foreground, "未找到匹配内容");
        }
        let count = filtered.as_ref().map_or(state.items.len(), Vec::len);
        let columns = VideoCards::columns(self.list_width);
        let frame = CardFrame::video(cx, window, self.list_width);
        let rows = count.div_ceil(columns);
        let footer = state.footer_visible();
        let sizes = Rows::new(rows, frame.metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows_tracked(
            cx,
            window,
            tab.list_id(),
            sizes,
            self.list_width,
            &self.state(tab).scroll,
            move |this, range, window, cx| {
                this.cards.set_first_row(range.start);
                this.maybe_load_more(tab, range.end.saturating_sub(1), rows, cx);
                range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(
                                tab,
                                CardRow {
                                    filtered: filtered.as_deref(),
                                    row,
                                    columns,
                                    frame,
                                },
                                window,
                                cx,
                            )
                        }
                    })
                    .collect()
            },
        )
    }
}

fn folder_card(
    folder: &FavFolder,
    index: usize,
    source: FolderSource,
    width: Pixels,
    selected: bool,
    active: bool,
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
        .relative()
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
        .when(selected, |this| {
            this.child(selection_outline(primary, radius, active))
        })
        .into_any_element()
}
