rigili_macro::mod_prv!(card);

use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::DynamicItem;

use super::super::comment;
use super::SpaceView;
use crate::shared::browse::filter::{self, text_matches};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, list_footer};
use crate::shared::browse::paging::{Page, spawn_first, spawn_more};
use crate::shared::ui::state_view::{Layout, centered_spinner, empty_state, error_state};

const DYNAMIC_CARD_MAX_WIDTH: Pixels = px(840.);

impl SpaceView {
    pub(super) fn load_dynamics(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.dynamics.fail_first_reset(error);
                cx.notify();
                return;
            }
        };

        self.expanded_comments = None;
        self.comment_views.clear();
        self.comment_trigger_key = None;

        spawn_first(
            self,
            cx,
            |this| &mut this.dynamics,
            |item: &DynamicItem| item.id_str.clone(),
            async move {
                let page = client.dynamics(mid, "").await?;
                Ok(Page::new(page.items, page.has_more, page.offset))
            },
            |_| {},
        );
    }

    fn load_more_dynamics(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.dynamics.meta.is_empty() {
            return;
        }
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.dynamics.fail_more(error);
                cx.notify();
                return;
            }
        };

        spawn_more(
            self,
            cx,
            |this| &mut this.dynamics,
            |item: &DynamicItem| item.id_str.clone(),
            move |offset| async move {
                let page = client.dynamics(mid, &offset).await?;
                Ok(Page::new(page.items, page.has_more, page.offset))
            },
            |_| {},
        );
    }

    fn retry_more_dynamics(&mut self, cx: &mut Context<Self>) {
        self.dynamics.retry_more();
        self.load_more_dynamics(cx);
    }

    fn toggle_comments(
        &mut self,
        key: String,
        dynamic: DynamicItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.expanded_comments.as_deref() == Some(key.as_str()) {
            self.collapse_comments(window, cx);
            return;
        }

        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.comment_views.entry(key.clone())
        {
            let view = cx.new(|_| comment::CommentsView::new(dynamic));
            view.update(cx, |view, cx| view.load(true, cx));
            entry.insert(view);
        }

        self.expanded_comments = Some(key.clone());
        self.comment_trigger_key = Some(key);
        cx.notify();
        let focus = self.comments_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    fn collapse_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.expanded_comments.take().is_none() {
            return;
        }
        cx.notify();
        let focus = self.comment_trigger_focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    pub(super) fn render_dynamics(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.dynamics.loading && self.dynamics.items.is_empty() {
            return centered_spinner();
        }
        if let Some(error) = self.dynamics.error.clone() {
            return error_state(
                Layout::Centered,
                error,
                "space-dynamics-retry",
                danger,
                cx.listener(|this, _, _, cx| this.reload_tab(cx)),
            );
        }
        if self.dynamics.items.is_empty() {
            return empty_state(Layout::Centered, muted, "暂无动态");
        }

        let needle = self.search.needle();
        let filtered =
            filter::filter_indices(&self.dynamics.items, needle.as_deref(), dynamic_matches);
        let cards = match &filtered {
            Some(indices) => indices
                .iter()
                .map(|&index| self.render_dynamic_card(&self.dynamics.items[index], index, cx))
                .collect::<Vec<_>>(),
            None => self
                .dynamics
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| self.render_dynamic_card(item, index, cx))
                .collect::<Vec<_>>(),
        };
        if cards.is_empty() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().text_sm().text_color(muted).child("未找到匹配动态"))
                .child(self.render_dynamics_footer(cx))
                .into_any_element();
        }
        let footer = self.render_dynamics_footer(cx);

        crate::shared::ui::scroll::attach(
            div()
                .id("space-dynamics-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex().w_full().items_start().justify_center().child(
                        v_flex()
                            .w_full()
                            .max_w(DYNAMIC_CARD_MAX_WIDTH)
                            .gap_4()
                            .pb_4()
                            .children(cards)
                            .child(footer),
                    ),
                ),
            &self.dynamics_scroll,
            cx,
        )
        .into_any_element()
    }

    fn render_dynamics_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.dynamics.loading_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element()
        } else if let Some(error) = self.dynamics.more_error.clone() {
            list_footer(
                "space-dynamics-more",
                Some(error),
                muted,
                danger,
                cx.listener(|this, _, _, cx| this.retry_more_dynamics(cx)),
            )
        } else if self.dynamics.has_more {
            h_flex()
                .h(LIST_FOOTER_HEIGHT)
                .justify_center()
                .child(
                    Button::new("space-dynamics-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_dynamics(cx))),
                )
                .into_any_element()
        } else {
            div().h(LIST_FOOTER_HEIGHT).into_any_element()
        }
    }
}

fn dynamic_matches(item: &DynamicItem, lowercase_needle: &str) -> bool {
    text_matches(item.text(), lowercase_needle)
        || text_matches(&item.author().name, lowercase_needle)
        || item
            .archive()
            .is_some_and(|archive| text_matches(&archive.title, lowercase_needle))
        || item
            .forward()
            .is_some_and(|forward| dynamic_matches(forward, lowercase_needle))
}

#[cfg(test)]
mod tests {
    use rigili_bilibili::{
        DynamicArchive, DynamicAuthor, DynamicContent, DynamicDesc, DynamicItem, DynamicMajor,
        DynamicModules,
    };

    use super::dynamic_matches;

    fn dynamic(text: &str, author: &str, archive: Option<&str>) -> DynamicItem {
        DynamicItem {
            modules: DynamicModules {
                module_author: DynamicAuthor {
                    name: author.into(),
                    ..Default::default()
                },
                module_dynamic: DynamicContent {
                    desc: Some(DynamicDesc { text: text.into() }),
                    major: archive.map(|title| DynamicMajor {
                        archive: Some(DynamicArchive {
                            title: title.into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn matches_dynamic_text_author_and_video_title() {
        let item = dynamic("今天聊聊 Rust", "小明", Some("GPUI 入门"));

        assert!(dynamic_matches(&item, "rust"));
        assert!(dynamic_matches(&item, "小明"));
        assert!(dynamic_matches(&item, "gpui"));
        assert!(!dynamic_matches(&item, "kotlin"));
    }

    #[test]
    fn matches_forwarded_dynamic() {
        let mut item = dynamic("转发", "UP主", None);
        item.orig = Some(Box::new(dynamic("", "", Some("被转发的视频"))));

        assert!(dynamic_matches(&item, "被转发"));
    }
}
