use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{INFO_WIDTH, InfoPanel, InfoTab};

impl Render for InfoPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let Some(details) = self.details.clone() else {
            return div().into_any_element();
        };

        let content = match self.tab {
            InfoTab::Info => self.render_info_tab(&details, cx),
            InfoTab::Comments => self.render_comments_tab(window, cx),
        };

        v_flex()
            .w(INFO_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(cx))
            .child(content)
            .into_any_element()
    }
}
