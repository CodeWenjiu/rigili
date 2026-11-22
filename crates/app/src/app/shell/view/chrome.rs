use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::Popup;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{MainView, Page, SIDEBAR_WIDTH, SearchHost, TOP_BAR_HEIGHT};
use crate::pages::account::login;
use crate::pages::content::home::{HomeTab, HomeView};
use crate::shared::ui::tabs::{TabNav, TabStrip};

type NavHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavState {
    Idle,
    Active,
    Return,
}

fn home_state(page: Page, space_open: bool) -> NavState {
    match page {
        Page::Home if !space_open => NavState::Active,
        Page::Home | Page::Search => NavState::Return,
        Page::Mine => NavState::Idle,
    }
}

fn mine_state(page: Page, space_open: bool) -> NavState {
    match page {
        Page::Mine if space_open => NavState::Return,
        Page::Mine => NavState::Active,
        Page::Home | Page::Search => NavState::Idle,
    }
}

#[derive(Clone)]
struct NavItem {
    label: SharedString,
    icon: IconName,
    state: NavState,
    on_click: NavHandler,
}

impl NavItem {
    fn new(label: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            label: label.into(),
            icon,
            state: NavState::Idle,
            on_click: Rc::new(|_: &ClickEvent, _: &mut Window, _: &mut App| {}),
        }
    }

    fn state(mut self, state: NavState) -> Self {
        self.state = state;
        self
    }

    fn on_click(mut self, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Rc::new(on_click);
        self
    }

    fn render(self, id: impl Into<ElementId>, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let icon = self.icon;
        let label = self.label;
        let on_click = self.on_click;
        let active = state == NavState::Active;
        let returning = state == NavState::Return;
        let color = if active {
            cx.theme().sidebar_accent_foreground
        } else {
            cx.theme().sidebar_foreground
        };
        let dot = cx.theme().sidebar_accent_foreground;

        Button::new(id)
            .ghost()
            .w_full()
            .h(px(60.))
            .px_0()
            .py_0()
            .accessibility_label(label.clone())
            .when(returning, |this| this.tooltip("返回上一级"))
            .on_click(move |event, window, cx| on_click(event, window, cx))
            .child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_color(color)
                    .child(Icon::new(icon).with_size(px(22.)))
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_sm()
                            .when(active, |this| this.font_medium())
                            .child(label),
                    )
                    .child(
                        div()
                            .size(px(4.))
                            .rounded_full()
                            .when(returning, |this| this.bg(dot)),
                    ),
            )
    }
}

fn search_width(window: &Window) -> Pixels {
    (window.viewport_size().width - px(24.)).clamp(px(240.), px(420.))
}

fn messages_panel_width(window: &Window) -> Pixels {
    let insets = px(16.);
    (window.viewport_size().width - SIDEBAR_WIDTH - insets).min(px(600.))
}

impl MainView {
    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(user) = rigili_core::login::user(cx) {
            return Button::new("account-space")
                .ghost()
                .p_1()
                .rounded_full()
                .accessibility_label("个人空间")
                .tooltip("个人空间")
                .on_click(cx.listener(|this, _, window, cx| this.toggle_my_space(window, cx)))
                .child(Avatar::new().src(user.face).name(user.uname).size(px(32.)))
                .into_any_element();
        }

        Button::new("sign-in")
            .ghost()
            .small()
            .icon(IconName::CircleUser)
            .accessibility_label("登录")
            .tooltip("登录")
            .on_click(cx.listener(|_, _, window, cx| {
                login::open(window, cx);
            }))
            .into_any_element()
    }

    pub(super) fn render_messages_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .left(SIDEBAR_WIDTH + px(8.))
            .top(TOP_BAR_HEIGHT + px(8.))
            .bottom(px(8.))
            .w(messages_panel_width(window))
            .occlude()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_xl()
            .child(self.messages.clone())
            .into_any_element()
    }

    fn render_logo(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h(px(40.))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::default()
                    .data(include_bytes!("../../../../assets/rigili-mark.svg"))
                    .with_size(px(36.))
                    .text_color(cx.theme().sidebar_primary),
            )
            .into_any_element()
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let weak = cx.entity().downgrade();
        let space_open = !self.spaces.is_empty();

        v_flex()
            .w(SIDEBAR_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(border)
            .child(div().w_full().pt_3().child(self.render_logo(cx)))
            .child(
                v_flex()
                    .w_full()
                    .pt_3()
                    .gap_y_3()
                    .child(
                        NavItem::new("首页", IconName::House)
                            .state(home_state(self.page, space_open))
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update_in(cx, |this, window, cx| {
                                        this.show_home(cx);
                                        this.focus.focus(window, cx);
                                    });
                                }
                            })
                            .render("nav-home", cx),
                    )
                    .child(
                        NavItem::new("我的", IconName::CircleUser)
                            .state(mine_state(self.page, space_open))
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update_in(cx, |this, window, cx| {
                                        this.show_mine(cx);
                                        this.focus.focus(window, cx);
                                    });
                                }
                            })
                            .render("nav-mine", cx),
                    ),
            )
            .child(div().flex_1())
            .child(
                v_flex()
                    .w_full()
                    .pb_3()
                    .items_center()
                    .gap_1()
                    .child(self.account(cx))
                    .child(
                        NavItem::new("消息", IconName::Mail)
                            .state(if self.messages_open {
                                NavState::Active
                            } else {
                                NavState::Idle
                            })
                            .on_click({
                                let weak = weak.clone();
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| this.toggle_messages(cx));
                                }
                            })
                            .render("nav-messages", cx),
                    ),
            )
            .into_any_element()
    }

    fn render_home_tabs(&self, selected: HomeTab, cx: &mut Context<Self>) -> AnyElement {
        TabStrip::new("top-tab")
            .text_base()
            .gap(px(8.))
            .padding_x(px(12.))
            .divider(false)
            .items_end(false)
            .render(
                cx,
                HomeView::TABS,
                selected,
                HomeTab::label,
                |_| None,
                |this, tab, cx| {
                    this.messages_open = false;
                    this.page = Page::Home;
                    this.spaces.clear();
                    this.home.update(cx, |home, cx| home.select_tab(tab, cx));
                    cx.notify();
                },
            )
    }

    pub(super) fn render_top_bar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let home_tab = self.home.read(cx).active_tab();

        h_flex()
            .h(TOP_BAR_HEIGHT)
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(border)
            .bg(background)
            .text_color(foreground)
            .px_6()
            .child(
                div()
                    .flex_shrink_0()
                    .when(
                        matches!(self.page, Page::Mine) || !self.spaces.is_empty(),
                        |this| this.invisible(),
                    )
                    .child(self.render_home_tabs(home_tab, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_center()
                    .child(self.render_search(window, cx)),
            )
            .child(
                Button::new("window-close")
                    .ghost()
                    .icon(IconName::Close)
                    .accessibility_label("关闭窗口")
                    .tooltip("关闭")
                    .on_click(|_, window, _| window.remove_window()),
            )
            .into_any_element()
    }

    fn render_search(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let width = search_width(window);
        let host = self.search_host();
        let content = match host {
            SearchHost::Global => self.render_global_search(width, cx),
            SearchHost::Mine => self.mine.update(cx, |mine, cx| mine.search_field(cx)),
            SearchHost::Space => self.space.update(cx, |space, cx| space.search_field(cx)),
        };

        div()
            .w_full()
            .max_w(width)
            .key_context(rigili_core::TEXT_INPUT_CONTEXT)
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let global = host == SearchHost::Global;
                let handled = match event.keystroke.key.as_str() {
                    "escape" => {
                        if global {
                            this.dismiss_search_box(window, cx);
                            this.focus.focus(window, cx);
                        } else {
                            this.blur_search(window, cx);
                        }
                        true
                    }
                    "down" if global => this.move_suggestion_selection(true, cx),
                    "up" if global => this.move_suggestion_selection(false, cx),
                    "enter" if global => this.pick_active_suggestion(window, cx),
                    _ => false,
                };
                if handled {
                    cx.stop_propagation();
                }
            }))
            .child(content)
            .into_any_element()
    }

    fn render_global_search(&self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        // The search input owns Enter and Space; Popover's trigger
        // would bind both to Confirm before the input sees them.
        let focus = self.search_box.focus_handle(cx);
        let mut popup = Popup::new(
            "search-popover",
            Input::new(self.search_box.input())
                .w_full()
                .large()
                .h(px(40.))
                .cleanable(true)
                .suffix(
                    Button::new("search-submit")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Search)
                        .accessibility_label("搜索")
                        .tooltip("搜索")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_search(window, cx);
                        })),
                ),
        )
        .offset(px(6.))
        .w_full()
        .max_w(width)
        .track_focus(&focus)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if !this.search_box_is_open() {
                    this.open_search_box(cx);
                }
            }),
        );
        if self.search_box_is_open() {
            let weak = cx.entity().downgrade();
            let panel = self.render_search_panel(&weak, width, cx);
            popup = popup.content(
                div()
                    .id("search-popover-content")
                    .role(Role::Dialog)
                    .tab_group()
                    // Tracking the field's focus handle here would
                    // move its focus node to this panel, which is
                    // a sibling of the input; key bindings resolve
                    // along that node's contexts, so backspace and
                    // the other editing keys would stop reaching
                    // the input.
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        this.dismiss_search_box(window, cx);
                    }))
                    .child(panel),
            );
        }
        popup.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{NavState, Page, home_state, mine_state};

    #[test]
    fn home_is_active_only_at_its_root() {
        assert_eq!(home_state(Page::Home, false), NavState::Active);
        assert_eq!(home_state(Page::Search, false), NavState::Return);
        assert_eq!(home_state(Page::Mine, false), NavState::Idle);
        assert_eq!(home_state(Page::Home, true), NavState::Return);
        assert_eq!(home_state(Page::Search, true), NavState::Return);
        assert_eq!(home_state(Page::Mine, true), NavState::Idle);
    }

    #[test]
    fn mine_returns_only_from_its_own_spaces() {
        assert_eq!(mine_state(Page::Mine, false), NavState::Active);
        assert_eq!(mine_state(Page::Mine, true), NavState::Return);
        assert_eq!(mine_state(Page::Home, false), NavState::Idle);
        assert_eq!(mine_state(Page::Home, true), NavState::Idle);
        assert_eq!(mine_state(Page::Search, false), NavState::Idle);
        assert_eq!(mine_state(Page::Search, true), NavState::Idle);
    }
}
