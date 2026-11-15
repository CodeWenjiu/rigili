use gpui_kit::base::TextSelection;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{INFO_WIDTH, InfoPanel, InfoTab};

impl Render for InfoPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let details = self.details.clone();

        let content = match (details.as_ref(), self.tab) {
            (Some(details), InfoTab::Info) => self.render_info_tab(details, cx),
            (Some(_), InfoTab::Comments) => self.render_comments_tab(window, cx),
            (None, _) => self.render_placeholder(cx),
        };

        v_flex()
            .id("info-panel")
            .test_support()
            .w(INFO_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(background)
            .text_color(foreground)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.clear_spanning_selection(window, cx);
                if event.pressed_button == Some(MouseButton::Left)
                    && this.tab == InfoTab::Comments
                    && TextSelection::has_selection(window, cx)
                {
                    window.refresh();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.clear_spanning_selection(window, cx);
                }),
            )
            .child(self.render_header(cx))
            .child(content)
            .into_any_element()
    }
}

impl InfoPanel {
    fn render_placeholder(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        h_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .child(if self.busy {
                Spinner::new().into_any_element()
            } else {
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("暂无视频信息")
                    .into_any_element()
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{INFO_WIDTH, InfoPanel};
    use crate::info::{InfoSelections, InfoTab};
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};

    #[gpui_kit::test]
    fn keeps_the_panel_mounted_without_details(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(500.), px(300.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|_| InfoPanel::new(selections))
                },
            )
            .expect("open info panel window")
        });

        let width = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.find("info-panel").bounds().size.width
            })
            .expect("info panel bounds");
        assert_eq!(width, INFO_WIDTH);
    }

    #[gpui_kit::test]
    fn keeps_the_comments_tab_when_clearing(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, panel) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(500.), px(300.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|_| InfoPanel::new(selections))
                },
            )
            .expect("open info panel window")
        });

        panel.update(cx, |panel, cx| {
            panel.tab = InfoTab::Comments;
            panel.clear(cx);
            assert_eq!(panel.tab, InfoTab::Comments);
        });
    }
}
