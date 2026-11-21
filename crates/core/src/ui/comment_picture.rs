use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};

use futures::AsyncReadExt;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::http_client::{AsyncBody, HttpClient, http};
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{CommentPicture, normalize_url, scaled, scaled_png};

use super::animated_image::AnimatedImage;

const PICTURE_LIMIT: usize = 9;
const PICTURE_MIN_EDGE: f32 = 32.;
const PICTURE_THUMBNAIL_HEIGHT: f32 = 80.;
const PICTURE_THUMBNAIL_MAX_WIDTH: f32 = 128.;
const PICTURE_SINGLE_MAX_WIDTH: f32 = 240.;
const PICTURE_SINGLE_MAX_HEIGHT: f32 = 320.;
const PICTURE_ANIMATED_MAX_WIDTH: f32 = 160.;
const PICTURE_PIXEL_RATIO: f32 = 2.;
const FULL_WIDTH: u32 = 1280;
const FULL_HEIGHT: u32 = 8192;
const FULL_PIXELS: f32 = 6_000_000.;
const MAX_COPY_BYTES: usize = 20 * 1024 * 1024;

/// Renders the pictures attached to a comment as a bounded thumbnail grid.
pub fn grid(rpid: u64, pictures: &[CommentPicture], cx: &App) -> AnyElement {
    let placeholder = cx.theme().muted;
    let images = crate::image_cache::comment_pictures(cx);
    let single = pictures.len() == 1;
    h_flex()
        .flex_wrap()
        .gap_1()
        .children(
            pictures
                .iter()
                .take(PICTURE_LIMIT)
                .enumerate()
                .map(|(index, picture)| {
                    let all = pictures.to_vec();
                    let src = picture.img_src.clone();
                    let (width, height) = tile_size(picture.img_width, picture.img_height, single);
                    let thumbnail = thumbnail_url(&src, width, picture.img_width);
                    let image_id = ("comment-picture-image", rpid * 10 + index as u64);
                    div()
                        .id(("comment-picture", rpid * 10 + index as u64))
                        .test_support()
                        .role(Role::Button)
                        .aria_label("查看图片")
                        .w(px(width))
                        .h(px(height))
                        .max_w_full()
                        .rounded(px(4.))
                        .overflow_hidden()
                        .bg(placeholder)
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            show(all.clone(), index, window, cx);
                        })
                        .child(
                            AnimatedImage::new(
                                image_id,
                                SharedUri::from(thumbnail),
                                images.clone(),
                            )
                            .radius(px(4.)),
                        )
                }),
        )
        .into_any_element()
}

fn tile_size(width: u32, height: u32, single: bool) -> (f32, f32) {
    let (width, height) = if width == 0 || height == 0 {
        (4., 3.)
    } else {
        (width as f32, height as f32)
    };
    let (max_width, max_height) = if single {
        (PICTURE_SINGLE_MAX_WIDTH, PICTURE_SINGLE_MAX_HEIGHT)
    } else {
        (PICTURE_THUMBNAIL_MAX_WIDTH, PICTURE_THUMBNAIL_HEIGHT)
    };
    let scale = (max_width / width).min(max_height / height);
    let scale = if single { scale.min(1.) } else { scale };
    (
        (width * scale).round().max(PICTURE_MIN_EDGE),
        (height * scale).round().max(PICTURE_MIN_EDGE),
    )
}

fn thumbnail_url(src: &str, width: f32, native: u32) -> String {
    if is_animated_source(src) {
        let width = width.min(PICTURE_ANIMATED_MAX_WIDTH);
        scaled(src, request_width(width, native, 1.))
    } else {
        scaled_png(src, request_width(width, native, PICTURE_PIXEL_RATIO))
    }
}

fn is_animated_source(src: &str) -> bool {
    src.split(['?', '#', '@'])
        .next()
        .unwrap_or(src)
        .rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif") || ext.eq_ignore_ascii_case("webp"))
}

fn request_width(width: f32, native: u32, ratio: f32) -> u32 {
    let width = (width * ratio).round().max(1.) as u32;
    if native == 0 {
        width
    } else {
        width.min(native)
    }
}

fn show(pictures: Vec<CommentPicture>, index: usize, window: &mut Window, cx: &mut App) {
    let viewer = cx.new(|cx| PictureViewer::new(pictures, index, cx));
    window.open_dialog(cx, move |dialog, window, cx| {
        let width = viewer.read(cx).stage(window.viewport_size()).0;
        dialog
            .entrance(DialogEntrance::None)
            .w(px(width + 32.))
            .child(viewer.clone())
    });
}

struct PictureViewer {
    pictures: Vec<CommentPicture>,
    index: usize,
    focus: FocusHandle,
    focused: bool,
    scroll: ScrollHandle,
    /// Dropping the handle cancels the copy, so it is held until the next one.
    copy: Option<Task<()>>,
}

impl PictureViewer {
    fn new(pictures: Vec<CommentPicture>, index: usize, cx: &mut Context<Self>) -> Self {
        Self {
            index: index.min(pictures.len().saturating_sub(1)),
            pictures,
            focus: cx.focus_handle(),
            focused: false,
            scroll: ScrollHandle::new(),
            copy: None,
        }
    }

    fn stage(&self, viewport: gpui_kit::Size<Pixels>) -> (f32, f32) {
        let (_, max_height) = limits(viewport);
        let (width, height) = self
            .pictures
            .iter()
            .map(|picture| fit(picture.img_width, picture.img_height, viewport))
            .fold((0_f32, 0_f32), |(width, height), (w, h)| {
                (width.max(w), height.max(h))
            });
        (width, height.min(max_height))
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let total = self.pictures.len();
        if total < 2 {
            return;
        }
        self.index = (self.index as isize + delta).rem_euclid(total as isize) as usize;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "c" if event.keystroke.modifiers.control => self.copy(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picture) = self.pictures.get(self.index) else {
            return;
        };
        let url = copy_url(&picture.img_src, picture.img_width, picture.img_height);
        let client = cx.http_client();
        self.copy = Some(cx.spawn_in(window, async move |this, cx| {
            let note = match fetch(client, url).await {
                Some(bytes) => {
                    let copied = cx
                        .background_executor()
                        .spawn(async move { copy_bytes(&bytes) })
                        .await;
                    match copied {
                        Ok(()) => Notification::success("已复制图片"),
                        Err(error) => Notification::error(error),
                    }
                }
                None => Notification::error("复制图片失败"),
            };
            let _ = this.update_in(cx, |_, window, cx| crate::toast::push(window, cx, note));
        }));
    }
}

impl Render for PictureViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            let focus = self.focus.clone();
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }

        let viewport = window.viewport_size();
        let (stage_width, stage_height) = self.stage(viewport);
        let Some(picture) = self.pictures.get(self.index) else {
            return div().into_any_element();
        };
        let (width, height) = fit(picture.img_width, picture.img_height, viewport);
        let muted = cx.theme().muted_foreground;
        let background = cx.theme().background;
        let canvas = cx.theme().muted;
        let images = crate::image_cache::comment_previews(cx);
        let total = self.pictures.len();

        let image = div().w(px(width)).h(px(height)).child(AnimatedImage::new(
            ("comment-picture-preview", hash(&picture.img_src)),
            SharedUri::from(full_url(
                &picture.img_src,
                picture.img_width,
                picture.img_height,
            )),
            images.clone(),
        ));
        let stage = if height > stage_height {
            div()
                .id("comment-picture-scroll")
                .w(px(stage_width))
                .h(px(stage_height))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .bg(canvas)
                .child(v_flex().w_full().items_center().child(image))
                .into_any_element()
        } else {
            div()
                .w(px(stage_width))
                .h(px(stage_height))
                .flex()
                .items_center()
                .justify_center()
                .bg(canvas)
                .child(image)
                .into_any_element()
        };

        v_flex()
            .id("comment-picture-viewer")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(background)
            .gap_2()
            .child(stage)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(h_flex().items_center().gap_2().when(total > 1, |this| {
                        this.child(
                            Button::new("comment-picture-prev")
                                .ghost()
                                .small()
                                .icon(IconName::ChevronLeft)
                                .accessibility_label("上一张")
                                .tooltip("上一张")
                                .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("{} / {total}", self.index + 1)),
                        )
                        .child(
                            Button::new("comment-picture-next")
                                .ghost()
                                .small()
                                .icon(IconName::ChevronRight)
                                .accessibility_label("下一张")
                                .tooltip("下一张")
                                .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
                        )
                    }))
                    .child(
                        Button::new("comment-picture-copy")
                            .ghost()
                            .small()
                            .icon(IconName::Copy)
                            .accessibility_label("复制图片")
                            .tooltip("复制图片 (Ctrl+C)")
                            .on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))),
                    ),
            )
            .into_any_element()
    }
}

async fn fetch(client: Arc<dyn HttpClient>, url: String) -> Option<Vec<u8>> {
    let response = client.get(&url, AsyncBody::empty(), true).await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let image = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("image/"));
    if !image {
        return None;
    }
    let mut body = response.into_body().take(MAX_COPY_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes).await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_COPY_BYTES {
        return None;
    }
    Some(bytes)
}

fn copy_url(src: &str, width: u32, height: u32) -> String {
    let width = if width == 0 || height == 0 {
        FULL_WIDTH
    } else {
        preview_size(width, height).0
    };
    scaled_png(src, width)
}

fn copy_bytes(bytes: &[u8]) -> Result<(), &'static str> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut command = if wayland {
        let mut command = Command::new("wl-copy");
        command.args(["--type", "image/png"]);
        command
    } else {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard", "-t", "image/png", "-i"]);
        command
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            if wayland {
                "复制图片需要 wl-clipboard"
            } else {
                "复制图片需要 xclip"
            }
        })?;
    let copied = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(bytes).is_ok());
    if !copied {
        let _ = child.kill();
        return Err("复制图片失败");
    }
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        _ => Err("复制图片失败"),
    }
}

fn full_url(src: &str, width: u32, height: u32) -> String {
    // A tall picture decodes to tens of megabytes at its native size, and a
    // height past the GPU's texture limit leaves the viewer blank. Bound the
    // long edge, the height and the pixel count so the CDN returns a
    // proportional copy that still fits the viewer's budget.
    if width == 0 || height == 0 {
        return scaled(src, FULL_WIDTH);
    }
    if preview_scale(width, height) >= 1. {
        return normalize_url(src);
    }
    scaled(src, preview_size(width, height).0)
}

fn preview_scale(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 {
        return 1.;
    }
    let by_width = FULL_WIDTH as f32 / width as f32;
    let by_height = FULL_HEIGHT as f32 / height as f32;
    let by_pixels = (FULL_PIXELS / (width as f32 * height as f32)).sqrt();
    by_width.min(by_height).min(by_pixels).min(1.)
}

fn preview_size(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (4, 3);
    }
    let scale = preview_scale(width, height);
    let w = ((width as f32 * scale).floor() as u32).max(1);
    let h = ((height as f32 * w as f32 / width as f32).round() as u32).max(1);
    (w, h)
}

fn limits(viewport: gpui_kit::Size<Pixels>) -> (f32, f32) {
    // Room for the dialog's 16px padding, its close button and window margins.
    let max_width = (f32::from(viewport.width) - 96.).max(200.);
    let max_height = (f32::from(viewport.height) - 160.).max(160.);
    (max_width, max_height)
}

fn fit(width: u32, height: u32, viewport: gpui_kit::Size<Pixels>) -> (f32, f32) {
    let (max_width, _) = limits(viewport);
    let (width, height) = preview_size(width, height);
    let scale = (max_width / width as f32).min(1.);
    (
        (width as f32 * scale).round(),
        (height as f32 * scale).round(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        PictureViewer, copy_url, fit, full_url, limits, request_width, thumbnail_url, tile_size,
    };
    use gpui_kit::base::v_flex;
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Bounds, Entity, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use rigili_bilibili::CommentPicture;

    fn picture() -> CommentPicture {
        CommentPicture {
            img_src: String::new(),
            img_width: 800,
            img_height: 600,
        }
    }

    struct ViewerHarness {
        viewer: Entity<PictureViewer>,
    }

    struct GridHarness {
        width: gpui_kit::Pixels,
        pictures: Vec<CommentPicture>,
    }

    impl Render for GridHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .w(self.width)
                .child(super::grid(0, &self.pictures, cx))
        }
    }

    impl Render for ViewerHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.viewer.clone())
        }
    }

    #[gpui_kit::test]
    fn arrow_keys_step_through_pictures(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(crate::image_cache::init);
        let viewer = cx.update(|cx| {
            cx.new(|cx| PictureViewer::new(vec![picture(), picture(), picture()], 0, cx))
        });
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(800.), px(600.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|_| ViewerHarness {
                        viewer: viewer.clone(),
                    })
                },
            )
            .expect("open viewer window")
        });

        for _ in 0..2 {
            cx.update_window(window, |_, window, cx| window.render_frame(cx))
                .expect("render viewer");
        }

        cx.simulate_keystrokes(window, "right");
        assert_eq!(cx.update(|cx| viewer.read(cx).index), 1);

        cx.simulate_keystrokes(window, "left");
        assert_eq!(cx.update(|cx| viewer.read(cx).index), 0);

        cx.simulate_keystrokes(window, "left");
        assert_eq!(cx.update(|cx| viewer.read(cx).index), 2);
    }

    #[gpui_kit::test]
    fn keeps_a_wide_picture_inside_a_narrow_body(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(crate::image_cache::init);
        let pictures = vec![CommentPicture {
            img_src: String::new(),
            img_width: 1920,
            img_height: 1080,
        }];
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(800.), px(600.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|_| GridHarness {
                        width: px(200.),
                        pictures,
                    })
                },
            )
            .expect("open grid window")
        });

        let bounds = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.find(("comment-picture", 0u64)).bounds()
            })
            .expect("grid bounds");
        assert_eq!(bounds.size.width, px(200.));
        assert_eq!(bounds.size.height, px(135.));
    }

    #[test]
    fn sizes_tiles_by_aspect_ratio() {
        assert_eq!(tile_size(1920, 1080, false), (128., 72.));
        assert_eq!(tile_size(1080, 1920, false), (45., 80.));
        assert_eq!(tile_size(1920, 1080, true), (240., 135.));
        assert_eq!(tile_size(1080, 1920, true), (180., 320.));
        assert_eq!(tile_size(40, 40, true), (40., 40.));
        assert_eq!(tile_size(4000, 100, false), (128., 32.));
        assert_eq!(tile_size(0, 0, false), (107., 80.));
    }

    #[test]
    fn requests_still_and_animated_thumbnails_at_their_sizes() {
        assert_eq!(request_width(240., 1920, 2.), 480);
        assert_eq!(request_width(40., 40, 2.), 40);
        assert_eq!(request_width(160., 0, 1.), 160);
    }

    #[test]
    fn animates_dynamic_thumbnails() {
        assert_eq!(
            thumbnail_url("//i0.hdslb.com/bfs/new_dyn/a.gif", 240., 1600),
            "https://i0.hdslb.com/bfs/new_dyn/a.gif@160w.webp"
        );
        assert_eq!(
            thumbnail_url("//i0.hdslb.com/bfs/new_dyn/a.gif@320w_1c.webp", 128., 1600),
            "https://i0.hdslb.com/bfs/new_dyn/a.gif@128w.webp"
        );
        assert_eq!(
            thumbnail_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 128., 1600),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@128w.webp"
        );
        assert_eq!(
            thumbnail_url("//i0.hdslb.com/bfs/new_dyn/a.jpg", 240., 1920),
            "https://i0.hdslb.com/bfs/new_dyn/a.jpg@480w.png"
        );
    }

    #[test]
    fn lets_large_pictures_use_the_window() {
        assert_eq!(limits(size(px(1600.), px(1000.))), (1504., 840.));
    }

    #[test]
    fn fits_pictures_to_the_viewer_width() {
        let viewport = size(px(800.), px(600.));

        let (width, height) = fit(1000, 500, viewport);
        assert!(width <= 704.);
        assert!((width / height - 2.).abs() < 0.01);

        assert_eq!(fit(320, 240, viewport), (320., 240.));

        let (width, height) = fit(0, 0, viewport);
        assert!(width <= 704.);
        assert!((width / height - 4. / 3.).abs() < 0.01);

        // A tall picture keeps a readable width and scrolls instead of
        // shrinking to the dialog's height.
        assert_eq!(fit(800, 20_000, viewport), (327., 8175.));
    }

    #[test]
    fn copies_png_at_a_bounded_width() {
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 800, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@800w.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 2000, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@1280w.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 600, 2000),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@600w.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 0, 0),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@1280w.png"
        );
    }

    #[test]
    fn resizes_only_pictures_larger_than_the_viewer() {
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 800, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.png"
        );
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 2000, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w.webp"
        );
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 600, 2000),
            "https://i0.hdslb.com/bfs/new_dyn/a.png"
        );
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 800, 20_000),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@327w.webp"
        );
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 0, 0),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w.webp"
        );
    }
}
