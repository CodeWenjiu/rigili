use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(crate) const LIST_FOOTER_HEIGHT: Pixels = px(56.);
const PAGE_GUTTER_REMS: f32 = 1.5;

fn page_gutter(window: &Window) -> Pixels {
    rems(PAGE_GUTTER_REMS).to_pixels(window.rem_size())
}

fn list_content_size(sizes: &[gpui_kit::Size<Pixels>], gap: Pixels) -> gpui_kit::Size<Pixels> {
    let height = sizes.iter().map(|size| size.height.as_f32()).sum::<f32>()
        + gap.as_f32() * sizes.len().saturating_sub(1) as f32;
    size(px(0.), px(height))
}

fn virtual_list_scrollbar<H: ScrollbarHandle + Clone>(
    handle: &H,
    content_size: gpui_kit::Size<Pixels>,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .child(
            Scrollbar::vertical(handle)
                .scroll_size(content_size)
                .viewport_from_layout(),
        )
        .into_any_element()
}

pub(crate) fn list_footer(
    retry_id: impl Into<ElementId>,
    more_error: Option<String>,
    muted: Hsla,
    danger: Hsla,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let content = if let Some(error) = more_error {
        h_flex()
            .items_center()
            .gap_3()
            .child(div().text_sm().text_color(danger).child(error))
            .child(
                Button::new(retry_id)
                    .small()
                    .label("重试")
                    .on_click(on_retry),
            )
            .into_any_element()
    } else {
        h_flex()
            .items_center()
            .gap_2()
            .child(Spinner::new().small())
            .child(div().text_sm().text_color(muted).child("正在加载…"))
            .into_any_element()
    };

    h_flex()
        .h(LIST_FOOTER_HEIGHT)
        .items_center()
        .justify_center()
        .child(content)
        .into_any_element()
}

pub(crate) trait ListWidth: 'static {
    fn list_width(&self) -> Pixels;
    fn set_list_width(&mut self, width: Pixels);
}

pub(crate) fn track_children_width<T: ListWidth>(
    known_width: Pixels,
    weak: WeakEntity<T>,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, window, cx| {
        let Some(width) = bounds.first().map(|bounds| bounds.size.width) else {
            return;
        };
        update_list_width(&weak, known_width, width, window, cx);
    }
}

pub(crate) fn track_element_width<T: ListWidth>(
    known_width: Pixels,
    weak: WeakEntity<T>,
) -> impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static {
    move |bounds, window, cx| {
        update_list_width(&weak, known_width, bounds.size.width, window, cx);
    }
}

fn update_list_width<T: ListWidth>(
    weak: &WeakEntity<T>,
    known_width: Pixels,
    width: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if width == known_width {
        return;
    }
    let weak = weak.clone();
    // Notifying during prepaint does not schedule another frame.
    window.defer(cx, move |_, cx| {
        let _ = weak.update(cx, |this, cx| {
            if this.list_width() != width {
                this.set_list_width(width);
                cx.notify();
            }
        });
    });
}

#[derive(Clone, Copy)]
pub(crate) struct Rows {
    rows: usize,
    row_height: Pixels,
    header: Option<Pixels>,
    footer: Option<Pixels>,
    gap: Rems,
}

impl Rows {
    pub(crate) fn new(rows: usize, row_height: Pixels) -> Self {
        Self {
            rows,
            row_height,
            header: None,
            footer: None,
            gap: rems(1.5),
        }
    }

    pub(crate) fn header_if(mut self, present: bool, height: Pixels) -> Self {
        if present {
            self.header = Some(height);
        }
        self
    }

    pub(crate) fn footer_if(mut self, present: bool, height: Pixels) -> Self {
        if present {
            self.footer = Some(height);
        }
        self
    }

    pub(crate) fn gap(mut self, gap: Rems) -> Self {
        self.gap = gap;
        self
    }
}

pub(crate) fn virtual_rows<V, R>(
    cx: &mut Context<V>,
    window: &Window,
    id: impl Into<ElementId>,
    rows: Rows,
    scroll: &VirtualListScrollHandle,
    render: R,
) -> AnyElement
where
    V: Render + 'static,
    R: Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<AnyElement> + 'static,
{
    virtual_rows_inner(
        cx,
        window,
        id,
        rows,
        scroll,
        None::<fn(Vec<Bounds<Pixels>>, &mut Window, &mut App)>,
        render,
    )
}

pub(crate) fn virtual_rows_tracked<V, R>(
    cx: &mut Context<V>,
    window: &Window,
    id: impl Into<ElementId>,
    rows: Rows,
    known_width: Pixels,
    scroll: &VirtualListScrollHandle,
    render: R,
) -> AnyElement
where
    V: Render + ListWidth,
    R: Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<AnyElement> + 'static,
{
    let tracker = track_children_width(known_width, cx.entity().downgrade());
    virtual_rows_inner(cx, window, id, rows, scroll, Some(tracker), render)
}

fn virtual_rows_inner<V, R, T>(
    cx: &mut Context<V>,
    window: &Window,
    id: impl Into<ElementId>,
    rows: Rows,
    scroll: &VirtualListScrollHandle,
    tracker: Option<T>,
    render: R,
) -> AnyElement
where
    V: Render + 'static,
    R: Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<AnyElement> + 'static,
    T: Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static,
{
    let mut sizes = Vec::with_capacity(rows.rows + 2);
    if let Some(height) = rows.header {
        sizes.push(size(px(0.), height));
    }
    sizes.extend(std::iter::repeat_n(
        size(px(0.), rows.row_height),
        rows.rows,
    ));
    if let Some(height) = rows.footer {
        sizes.push(size(px(0.), height));
    }

    let gap = rows.gap.to_pixels(window.rem_size());
    let content_size = list_content_size(&sizes, gap);
    let gutter = page_gutter(window);
    let list = v_virtual_list(
        cx.entity(),
        id,
        Rc::new(sizes),
        move |this, range, window, cx| render(this, range, window, cx),
    )
    .gap(gap)
    .track_scroll(scroll);

    div()
        .relative()
        .flex_1()
        .min_h_0()
        .mr(px(-gutter.as_f32()))
        .pr(gutter)
        .overflow_hidden()
        .when_some(tracker, |this, tracker| {
            this.on_children_prepainted(tracker)
        })
        .child(list)
        .child(virtual_list_scrollbar(scroll, content_size))
        .into_any_element()
}
