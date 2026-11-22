use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::SpaceVideos;
use rigili_bilibili::api::space::SpaceVideoOrder;

use super::{SpaceTab, SpaceView, VideoMeta};
use crate::shared::browse::grid::{
    CardFrame, CardGrid, CardRowView, VideoCards, video_card_row,
};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, Rows, paged_footer, virtual_rows_tracked};
use crate::shared::browse::paging::{Page, spawn_first, spawn_more};
use crate::shared::ui::state_view::{Layout, centered_spinner, empty_state, error_state};
use crate::shared::ui::tabs::SubTabNav;

const ORDERS: [SpaceVideoOrder; 2] = [SpaceVideoOrder::Latest, SpaceVideoOrder::Plays];

fn order_label(order: SpaceVideoOrder) -> &'static str {
    match order {
        SpaceVideoOrder::Latest => "最新发布",
        SpaceVideoOrder::Plays => "最多播放",
    }
}

impl SpaceView {
    pub(super) fn load_space_videos(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.space_videos.fail_first_reset(error);
                cx.notify();
                return;
            }
        };
        let order = self.space_order;
        let keyword = self.search.keyword().to_owned();

        spawn_first(
            self,
            cx,
            |this| &mut this.space_videos,
            |card: &crate::shared::browse::grid::VideoCard| card.bvid().to_owned(),
            async move { space_page(client.space_videos(mid, 1, order, &keyword).await?) },
            space_videos_applied,
        );
    }

    fn load_more_space_videos(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.space_videos.fail_more(error);
                cx.notify();
                return;
            }
        };
        let order = self.space_order;
        let keyword = self.search.keyword().to_owned();

        spawn_more(
            self,
            cx,
            |this| &mut this.space_videos,
            |card: &crate::shared::browse::grid::VideoCard| card.bvid().to_owned(),
            move |meta| {
                let page = meta.page + 1;
                async move { space_page(client.space_videos(mid, page, order, &keyword).await?) }
            },
            space_videos_applied,
        );
    }

    fn retry_more_space_videos(&mut self, cx: &mut Context<Self>) {
        self.space_videos.retry_more();
        self.load_more_space_videos(cx);
    }

    fn set_space_order(&mut self, order: SpaceVideoOrder, cx: &mut Context<Self>) {
        if self.space_order == order {
            return;
        }
        self.space_order = order;
        self.load_space_videos(cx);
    }

    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(
                h_flex().items_center().gap_2().children(
                    ORDERS.into_iter().enumerate().map(|(index, order)| {
                        let active = self.space_order == order;
                        let group = SharedString::from(format!("space-order-{index}"));
                        let label = order_label(order);
                        v_flex()
                            .group(group.clone())
                            .items_center()
                            .rounded(radius)
                            .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                            .child(
                                Button::new(("space-order", index))
                                    .text()
                                    .small()
                                    .px_2()
                                    .accessibility_label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_sub_tab(order, cx);
                                    }))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(if active { primary } else { muted })
                                            .group_hover(group, move |this| {
                                                this.text_color(primary)
                                            })
                                            .child(label),
                                    ),
                            )
                    }),
                ),
            )
            .when(self.space_videos.meta.total > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!("共 {} 个投稿", self.space_videos.meta.total)),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_space_videos_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.space_videos.loading && self.space_videos.items.is_empty() {
            return centered_spinner();
        }
        if let Some(error) = self.space_videos.error.clone() {
            return error_state(
                Layout::Centered,
                error,
                "space-videos-retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_tab(cx)),
            );
        }
        if self.space_videos.items.is_empty() {
            return empty_state(
                Layout::Centered,
                muted,
                if self.search.is_empty() {
                    "暂无投稿"
                } else {
                    "未找到匹配投稿"
                },
            );
        }

        let columns = VideoCards::columns(self.list_width);
        let frame = CardFrame::video(cx, window, self.list_width);
        let rows = self.space_videos.page_rows(columns);
        let footer = self.space_videos.footer_visible();
        let sizes = Rows::new(rows, frame.metrics.height()).footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows_tracked(
            cx,
            window,
            "space-video-list",
            sizes,
            self.list_width,
            &self.space_videos.scroll,
            move |this, range, window, cx| {
                this.cards.set_first_row(range.start);
                this.maybe_load_more_space_videos(range.end.saturating_sub(1), rows, cx);
                range
                    .map(|row| {
                        if footer && row == rows {
                            this.render_space_videos_footer(cx)
                        } else {
                            this.render_space_video_row(row, columns, frame, window, cx)
                        }
                    })
                    .collect()
            },
        )
    }

    fn maybe_load_more_space_videos(
        &mut self,
        last_visible: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        if rows > 0 && last_visible + 1 >= rows {
            self.load_more_space_videos(cx);
        }
    }

    fn render_space_video_row(
        &self,
        row: usize,
        columns: usize,
        frame: CardFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        video_card_row(
            &self.space_videos.items,
            row,
            columns,
            CardRowView {
                frame,
                selection: self.cards.index(),
                active: self.cards.active(window),
            },
            window,
            cx,
        )
    }

    fn render_space_videos_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        paged_footer(&self.space_videos, "space-videos-more", cx, |this, cx| {
            this.retry_more_space_videos(cx)
        })
    }
}

impl SubTabNav for SpaceView {
    type SubTab = SpaceVideoOrder;

    const SUB_TABS: &'static [SpaceVideoOrder] = &ORDERS;

    fn active_sub_tab(&self) -> Option<SpaceVideoOrder> {
        (self.tab == SpaceTab::Videos).then_some(self.space_order)
    }

    fn select_sub_tab(&mut self, order: SpaceVideoOrder, cx: &mut Context<Self>) {
        self.set_space_order(order, cx);
    }
}

fn space_page(
    page: SpaceVideos,
) -> Result<Page<crate::shared::browse::grid::VideoCard, VideoMeta>, rigili_bilibili::ApiError> {
    let number = page.page.pn;
    let total = page.page.count;
    let incoming = page
        .list
        .vlist
        .into_iter()
        .map(crate::shared::browse::grid::VideoCard::from)
        .collect::<Vec<_>>();
    let has_more = !incoming.is_empty();
    Ok(Page::new(
        incoming,
        has_more,
        VideoMeta {
            page: number,
            total,
        },
    ))
}

fn space_videos_applied(this: &mut SpaceView) {
    let state = &mut this.space_videos;
    state.has_more = state.has_more && (state.items.len() as u64) < state.meta.total;
}
