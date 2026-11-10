gpuili_macro::mod_prv!(feed, list);

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::backend::{self, Backend};
use crate::grid::{OpenVideo, VideoCard};

pub struct HomeView {
    backend: Option<Backend>,
    videos: Vec<VideoCard>,
    page: u32,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
    list_width: Pixels,
    scroll: VirtualListScrollHandle,
}

impl EventEmitter<OpenVideo> for HomeView {}

impl HomeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (backend, error) = match backend::get(cx) {
            Ok(backend) => (Some(backend), None),
            Err(error) => (None, Some(error)),
        };
        let mut view = Self {
            backend,
            videos: Vec::new(),
            page: 0,
            has_more: false,
            loading: false,
            loading_more: false,
            error,
            more_error: None,
            generation: 0,
            list_width: px(0.),
            scroll: VirtualListScrollHandle::new(),
        };
        if view.backend.is_some() {
            view.load(cx);
        }
        view
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let weak = cx.entity().downgrade();
        let content = self.render_content(window, cx);

        v_flex()
            .size_full()
            .gap_4()
            .px_6()
            .pt_5()
            .pb_6()
            .bg(background)
            .text_color(foreground)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("热门"),
                    )
                    .child(
                        Button::new("refresh")
                            .ghost()
                            .icon(IconName::RefreshCw)
                            .label("刷新")
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| this.load(cx));
                            }),
                    ),
            )
            .child(content)
    }
}
