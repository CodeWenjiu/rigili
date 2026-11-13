use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::search::SearchOrder;

use super::{SearchTab, SearchView};
use crate::tabs::TabStrip;

impl SearchView {
    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        TabStrip::new("search-tab")
            .gap(px(16.))
            .padding_x(px(8.))
            .render(
                cx,
                &SearchTab::ALL,
                self.tab,
                SearchTab::label,
                |tab| {
                    self.count(tab)
                        .filter(|count| *count > 0)
                        .map(tab_count_label)
                },
                |this, tab, cx| this.show_tab(tab, cx),
            )
    }

    pub(super) fn render_orders(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let opening = self.opening.is_some();

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .children(SearchOrder::ALL.map(|order| {
                let active = self.order == order;
                let id = order.as_str();

                if active {
                    Button::new(id)
                        .small()
                        .primary()
                        .label(order_label(order))
                        .on_click(cx.listener(move |this, _, _, cx| this.set_order(order, cx)))
                        .into_any_element()
                } else {
                    Button::new(id)
                        .small()
                        .ghost()
                        .text_color(muted)
                        .label(order_label(order))
                        .on_click(cx.listener(move |this, _, _, cx| this.set_order(order, cx)))
                        .into_any_element()
                }
            }))
            .child(div().flex_1())
            .when(opening, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Spinner::new().small())
                        .child(div().text_xs().text_color(muted).child("正在打开…")),
                )
            })
            .when(!opening, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(Icon::new(IconName::ListFilter).with_size(px(12.)))
                        .child("更多筛选"),
                )
            })
            .into_any_element()
    }
}

fn order_label(order: SearchOrder) -> &'static str {
    match order {
        SearchOrder::Default => "综合排序",
        SearchOrder::Clicks => "最多点击",
        SearchOrder::Newest => "最新发布",
        SearchOrder::Danmaku => "最多弹幕",
        SearchOrder::Favorites => "最多收藏",
    }
}

fn tab_count_label(count: u64) -> String {
    if count >= 100 {
        "99+".to_owned()
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{order_label, tab_count_label};
    use rigili_bilibili::api::search::SearchOrder;

    #[test]
    fn labels_tab_counts_and_orders() {
        assert_eq!(tab_count_label(0), "0");
        assert_eq!(tab_count_label(27), "27");
        assert_eq!(tab_count_label(99), "99");
        assert_eq!(tab_count_label(100), "99+");
        assert_eq!(tab_count_label(1_000), "99+");

        assert_eq!(
            SearchOrder::ALL.map(order_label),
            ["综合排序", "最多点击", "最新发布", "最多弹幕", "最多收藏"]
        );
    }
}
