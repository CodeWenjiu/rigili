use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(crate) fn stepped<T: Copy + PartialEq>(tabs: &[T], current: T, delta: isize) -> Option<T> {
    let index = tabs.iter().position(|tab| *tab == current)?;
    tabs.get(index.checked_add_signed(delta)?).copied()
}

/// The primary tab strip of a view. Implementing this wires `[` / `]`.
pub(crate) trait TabNav: Sized + 'static {
    type Tab: Copy + PartialEq + 'static;

    const TABS: &'static [Self::Tab];

    fn active_tab(&self) -> Self::Tab;
    fn select_tab(&mut self, tab: Self::Tab, cx: &mut Context<Self>);

    fn step_tab(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(tab) = stepped(Self::TABS, self.active_tab(), delta) else {
            return false;
        };
        self.select_tab(tab, cx);
        true
    }
}

/// The secondary tab strip of a view. Implementing this wires `{` / `}`.
pub(crate) trait SubTabNav: Sized + 'static {
    type SubTab: Copy + PartialEq + 'static;

    const SUB_TABS: &'static [Self::SubTab];

    fn active_sub_tab(&self) -> Option<Self::SubTab>;
    fn select_sub_tab(&mut self, tab: Self::SubTab, cx: &mut Context<Self>);

    fn step_sub_tab(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(current) = self.active_sub_tab() else {
            return false;
        };
        let Some(tab) = stepped(Self::SUB_TABS, current, delta) else {
            return false;
        };
        self.select_sub_tab(tab, cx);
        true
    }
}

#[derive(Clone, Copy)]
enum TabTextSize {
    Small,
    Base,
}

#[derive(Clone, Copy)]
pub(crate) struct TabStrip {
    id: &'static str,
    gap: Pixels,
    padding_x: Pixels,
    underline_width: Pixels,
    text_size: TabTextSize,
    divider: bool,
    items_end: bool,
}

impl TabStrip {
    pub(crate) fn new(id: &'static str) -> Self {
        Self {
            id,
            gap: px(16.),
            padding_x: px(8.),
            underline_width: px(24.),
            text_size: TabTextSize::Small,
            divider: true,
            items_end: true,
        }
    }

    pub(crate) fn text_base(mut self) -> Self {
        self.text_size = TabTextSize::Base;
        self
    }

    pub(crate) fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    pub(crate) fn padding_x(mut self, padding_x: Pixels) -> Self {
        self.padding_x = padding_x;
        self
    }

    pub(crate) fn underline_width(mut self, width: Pixels) -> Self {
        self.underline_width = width;
        self
    }

    pub(crate) fn divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }

    pub(crate) fn items_end(mut self, items_end: bool) -> Self {
        self.items_end = items_end;
        self
    }

    pub(crate) fn render<V, T>(
        self,
        cx: &Context<V>,
        tabs: &[T],
        active: T,
        label: impl Fn(T) -> &'static str,
        badge: impl Fn(T) -> Option<String>,
        on_select: impl Fn(&mut V, T, &mut Context<V>) + Clone + 'static,
    ) -> AnyElement
    where
        V: 'static,
        T: Copy + PartialEq + 'static,
    {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .when(self.items_end, |this| this.items_end())
            .when(!self.items_end, |this| this.items_center())
            .gap(self.gap)
            .when(self.divider, |this| this.border_b_1().border_color(border))
            .children(tabs.iter().copied().enumerate().map(|(index, tab)| {
                let active_tab = tab == active;
                let group = SharedString::from(format!("{}-group-{index}", self.id));
                let label = label(tab);
                let text = match self.text_size {
                    TabTextSize::Base => div().text_base(),
                    TabTextSize::Small => div().text_sm(),
                }
                .text_color(if active_tab { primary } else { muted })
                .group_hover(group.clone(), move |this| this.text_color(primary))
                .child(label);
                let content = match badge(tab) {
                    Some(count) => h_flex()
                        .items_center()
                        .gap_1()
                        .child(text)
                        .child(
                            div()
                                .text_xs()
                                .text_color(if active_tab {
                                    primary
                                } else {
                                    muted.opacity(0.8)
                                })
                                .child(count),
                        )
                        .into_any_element(),
                    None => text.into_any_element(),
                };
                let select = on_select.clone();

                v_flex()
                    .group(group)
                    .items_center()
                    .gap_1()
                    .rounded(radius)
                    .when(!active_tab, |this| {
                        this.hover(move |this| this.bg(hover_bg))
                    })
                    .child(
                        Button::new((self.id, index))
                            .text()
                            .px(self.padding_x)
                            .py_1()
                            .accessibility_label(label)
                            .on_click(cx.listener(move |this, _, _, cx| select(this, tab, cx)))
                            .child(content),
                    )
                    .child(div().h(px(2.)).w(self.underline_width).rounded(px(1.)).bg(
                        if active_tab {
                            primary
                        } else {
                            hsla(0., 0., 0., 0.)
                        },
                    ))
            }))
            .into_any_element()
    }

    pub(crate) fn render_nav<V>(
        self,
        view: &V,
        cx: &Context<V>,
        label: impl Fn(V::Tab) -> &'static str,
        badge: impl Fn(V::Tab) -> Option<String>,
    ) -> AnyElement
    where
        V: TabNav,
    {
        self.render(
            cx,
            V::TABS,
            view.active_tab(),
            label,
            badge,
            |this, tab, cx| this.select_tab(tab, cx),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::stepped;

    #[test]
    fn steps_through_tabs_and_stops_at_the_edges() {
        let tabs = [0, 1, 2];

        assert_eq!(stepped(&tabs, 0, 1), Some(1));
        assert_eq!(stepped(&tabs, 1, 1), Some(2));
        assert_eq!(stepped(&tabs, 2, 1), None);
        assert_eq!(stepped(&tabs, 2, -1), Some(1));
        assert_eq!(stepped(&tabs, 0, -1), None);
        assert_eq!(stepped(&tabs, 9, 1), None);
    }
}
