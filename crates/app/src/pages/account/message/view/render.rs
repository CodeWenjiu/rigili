use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::login as login_state;
use rigili_core::{OpenSpace, Refresh};

use super::{MessageTab, MessagesView};
use crate::shared::browse::list::{LIST_FOOTER_HEIGHT, Rows, virtual_rows};
use crate::shared::ui::state_view::{Layout, centered_small_spinner, empty_state, error_state};
use crate::shared::ui::tabs::TabStrip;

const ROW_HEIGHT: Pixels = px(88.);

impl MessagesView {
    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::MEDIUM)
                    .child("消息"),
            )
            .child(
                h_flex()
                    .items_center()
                    .when(login_state::user(cx).is_some(), |this| {
                        this.child(
                            Button::new("messages-refresh")
                                .ghost()
                                .icon(IconName::RefreshCw)
                                .accessibility_label("刷新消息")
                                .tooltip("刷新消息")
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        TabStrip::new("message-tab")
            .gap(px(20.))
            .padding_x(px(8.))
            .underline_width(px(32.))
            .render_nav(self, cx, MessageTab::label, |_| None)
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if login_state::user(cx).is_none() {
            return self.render_login_prompt(cx);
        }

        let tab = self.tab;
        let state = self.state(tab);
        let error = state.error.clone();
        let loaded = state.loaded;
        let empty = state.items.is_empty();
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if let Some(error) = error {
            return error_state(
                Layout::Top,
                error,
                "messages-retry",
                danger,
                cx.listener(|this, _, _, cx| this.retry(cx)),
            );
        }

        if !loaded && empty {
            return centered_small_spinner();
        }

        if empty {
            return empty_state(Layout::Top, muted, "暂无消息");
        }

        self.render_list(window, cx)
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.tab;
        let state = self.state(tab);
        let rows = state.items.len();
        let footer = state.footer_visible();
        let sizes = Rows::new(rows, ROW_HEIGHT)
            .gap(rems(0.))
            .footer_if(footer, LIST_FOOTER_HEIGHT);

        virtual_rows(
            cx,
            window,
            tab.list_id(),
            sizes,
            &self.state(tab).scroll,
            move |this, range, _, cx| {
                this.maybe_load_more(tab, range.end.saturating_sub(1), rows, cx);
                range
                    .map(|index| {
                        if footer && index == rows {
                            this.render_footer(tab, cx)
                        } else {
                            this.render_row(tab, index, cx)
                        }
                    })
                    .collect()
            },
        )
    }

    fn render_footer(&self, tab: MessageTab, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state(tab);
        let more_error = state.more_error.clone();
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        h_flex()
            .w_full()
            .h(LIST_FOOTER_HEIGHT)
            .items_center()
            .justify_center()
            .gap_2()
            .when_some(more_error, |this, error| {
                this.child(
                    Button::new("messages-more-retry")
                        .text()
                        .on_click(cx.listener(move |this, _, _, cx| this.retry_more(tab, cx)))
                        .child(div().text_xs().text_color(danger).child(error)),
                )
            })
            .when(state.more_error.is_none(), |this| {
                this.child(Spinner::new().small())
                    .child(div().text_xs().text_color(muted).child("加载中"))
            })
            .into_any_element()
    }

    fn render_row(&self, tab: MessageTab, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.state(tab).items[index];
        let key = SharedString::from(row.key.clone());
        let name = row.name.clone();
        let avatar = row.avatar.clone();
        let preview = row.preview.clone();
        let tooltip_preview = preview.clone();
        let single_line = preview.replace(['\n', '\r', '\t'], " ");
        let time = row.time.clone();
        let unread = row.unread;
        let mid = row.mid;
        let click_name = name.clone();
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;
        let danger = cx.theme().danger;
        let danger_foreground = cx.theme().danger_foreground;

        h_flex()
            .id(ElementId::Name(key))
            .w_full()
            .h(ROW_HEIGHT)
            .items_center()
            .gap_3()
            .px_3()
            .rounded(radius)
            .tooltip(move |window, cx| Tooltip::new(tooltip_preview.clone()).build(window, cx))
            .when(mid.is_some(), move |this| {
                this.cursor_pointer().hover(move |this| this.bg(hover_bg))
            })
            .when_some(mid, move |this, mid| {
                this.on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(OpenSpace {
                        mid,
                        name: click_name.clone(),
                    });
                }))
            })
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child({
                        let mut avatar_element = Avatar::new().name(name.clone()).size(px(48.));
                        if !avatar.is_empty() {
                            avatar_element = avatar_element.src(avatar);
                        }
                        avatar_element
                    })
                    .when(unread > 0, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-2.))
                                .right(px(-2.))
                                .min_w(px(18.))
                                .h(px(18.))
                                .px(px(4.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(danger)
                                .text_color(danger_foreground)
                                .text_xs()
                                .child(if unread > 99 {
                                    "99+".to_owned()
                                } else {
                                    unread.to_string()
                                }),
                        )
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(foreground)
                            .truncate()
                            .child(name),
                    )
                    .child({
                        div()
                            .w_full()
                            .min_w_0()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(single_line)
                    }),
            )
            .child(div().text_xs().text_color(muted).child(time))
            .when(mid.is_some(), |this| {
                this.child(
                    Icon::new(IconName::ChevronRight)
                        .with_size(px(16.))
                        .text_color(muted),
                )
            })
            .into_any_element()
    }

    fn render_login_prompt(&self, cx: &mut Context<Self>) -> AnyElement {
        crate::pages::account::login::Prompt::new("messages", IconName::Mail, "登录后查看消息")
            .centered()
            .render(cx)
    }
}

impl Render for MessagesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_user(window, cx);

        let background = cx.theme().popover;
        let foreground = cx.theme().foreground;
        let radius = cx.theme().radius_lg;
        let content = self.render_content(window, cx);

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_6()
            .rounded(radius)
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(cx))
            .child(self.render_tabs(cx))
            .child(content)
    }
}
