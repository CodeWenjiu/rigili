use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::backend;
use rigili_core::format_count;
use rigili_core::runtime::spawn_api;

use super::{InfoEvent, InfoPanel, InfoTab};

impl InfoPanel {
    fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let tab = if index == 1 {
            InfoTab::Comments
        } else {
            InfoTab::Info
        };
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        cx.notify();
        if tab == InfoTab::Comments && !self.comments_loaded && !self.comments_loading {
            self.load_comments(cx);
        }
    }

    pub(crate) fn show_tab(&mut self, tab: InfoTab, cx: &mut Context<Self>) {
        let index = match tab {
            InfoTab::Info => 0,
            InfoTab::Comments => 1,
        };
        self.select_tab(index, cx);
    }

    pub(crate) fn tab(&self) -> InfoTab {
        self.tab
    }

    pub(crate) fn reload_current(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            InfoTab::Info => {
                if self.refresh_busy {
                    return;
                }
                self.refresh_busy = true;
                self.refresh_error = None;
                cx.notify();
                cx.emit(InfoEvent::Reload);
            }
            InfoTab::Comments => self.load_comments(cx),
        }
    }

    fn toggle_watch_later(&mut self, cx: &mut Context<Self>) {
        if self.watch_later_busy {
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        let Ok(backend) = backend::get(cx) else {
            return;
        };
        let aid = details.aid;
        let bvid = details.bvid.to_string();
        let add = !self.watch_later;
        self.watch_later_busy = true;
        self.watch_later_touched = true;
        cx.notify();

        spawn_api(
            cx,
            async move {
                if add {
                    backend.watch_later_add(&bvid).await
                } else {
                    backend.watch_later_remove(aid).await
                }
            },
            move |this, result, cx| {
                if this.details.as_ref().map(|details| details.aid) != Some(aid) {
                    return;
                }
                this.watch_later_busy = false;
                if result.is_ok() {
                    this.watch_later = add;
                }
                cx.notify();
            },
        );
    }

    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let count: SharedString = format_count(self.comment_count).into();
        let watch_later = self.watch_later;
        let later_label: SharedString = if watch_later {
            "已加入稍后再看"
        } else {
            "稍后再看"
        }
        .into();
        let later_icon = if watch_later {
            Icon::new(IconName::Clock).text_color(primary)
        } else {
            Icon::new(IconName::Clock)
        };

        let reloading =
            self.refresh_busy || (self.tab == InfoTab::Comments && self.comments_loading);
        let watch_later_button = Button::new("player-watch-later")
            .ghost()
            .accessibility_label(later_label.clone())
            .tooltip(later_label)
            .icon(later_icon)
            .loading(self.watch_later_busy)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_watch_later(cx)));
        let reload_button = Button::new("player-reload")
            .ghost()
            .accessibility_label("重新加载")
            .tooltip("重新加载")
            .icon(IconName::RefreshCw)
            .loading(reloading)
            .on_click(cx.listener(|this, _, _, cx| this.reload(cx)));

        v_flex()
            .flex_shrink_0()
            .child(
                TabBar::new("player-info-tabs")
                    .underline()
                    .small()
                    .px_4()
                    .selected_index(if self.tab == InfoTab::Comments { 1 } else { 0 })
                    .child(Tab::new().label("简介"))
                    .child({
                        let comment_tab = Tab::new().label("评论");
                        if self.comment_count > 0 {
                            comment_tab.suffix(div().text_xs().text_color(muted).child(count))
                        } else {
                            comment_tab
                        }
                    })
                    .on_click(cx.listener(|this, index, _, cx| this.select_tab(*index, cx)))
                    .suffix(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(watch_later_button)
                            .child(reload_button),
                    ),
            )
            .into_any_element()
    }
}
