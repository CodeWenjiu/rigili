use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

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
}
