use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};

use futures::AsyncReadExt;
use gpui_kit::component::button::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::http_client::{AsyncBody, HttpClient, http};
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{CommentPicture, normalize_url, scaled, scaled_png, square};

const PICTURE_SIZE: f32 = 80.;
const PICTURE_LIMIT: usize = 9;
const THUMBNAIL_WIDTH: u32 = 160;
const FULL_WIDTH: u32 = 1280;
const MAX_COPY_BYTES: usize = 20 * 1024 * 1024;

/// Renders the pictures attached to a comment as a bounded thumbnail grid.
pub fn grid(rpid: u64, pictures: &[CommentPicture], cx: &App) -> AnyElement {
    let placeholder = cx.theme().muted;
    let images = crate::image_cache::comment_pictures(cx);
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
                    div()
                        .id(("comment-picture", rpid * 10 + index as u64))
                        .role(Role::Button)
                        .aria_label("查看图片")
                        .size(px(PICTURE_SIZE))
                        .rounded(px(4.))
                        .overflow_hidden()
                        .bg(placeholder)
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            show(all.clone(), index, window, cx);
                        })
                        .child(
                            img(square(&src, THUMBNAIL_WIDTH))
                                .image_cache(&images)
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                }),
        )
        .into_any_element()
}

fn show(pictures: Vec<CommentPicture>, index: usize, window: &mut Window, cx: &mut App) {
    let viewer = cx.new(|cx| PictureViewer::new(pictures, index, cx));
    window.open_dialog(cx, move |dialog, window, cx| {
        let width = viewer.read(cx).stage(window.viewport_size()).0;
        dialog.w(px(width + 32.)).child(viewer.clone())
    });
}

struct PictureViewer {
    pictures: Vec<CommentPicture>,
    index: usize,
    focus: FocusHandle,
    focused: bool,
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
            copy: None,
        }
    }

    fn stage(&self, viewport: gpui_kit::Size<Pixels>) -> (f32, f32) {
        self.pictures
            .iter()
            .map(|picture| fit(picture.img_width, picture.img_height, viewport))
            .fold((0., 0.), |(width, height), (w, h)| {
                (width.max(w), height.max(h))
            })
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let total = self.pictures.len();
        if total < 2 {
            return;
        }
        self.index = (self.index as isize + delta).rem_euclid(total as isize) as usize;
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
            let _ = this.update_in(cx, |_, window, cx| window.push_notification(note, cx));
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
        let images = crate::image_cache::comment_pictures(cx);
        let total = self.pictures.len();

        v_flex()
            .id("comment-picture-viewer")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .gap_2()
            .child(
                div()
                    .w(px(stage_width))
                    .h(px(stage_height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(full_url(
                            &picture.img_src,
                            picture.img_width,
                            picture.img_height,
                        ))
                        .image_cache(&images)
                        .w(px(width))
                        .h(px(height))
                        .object_fit(ObjectFit::Contain),
                    ),
            )
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
        let scale = (FULL_WIDTH as f32 / width.max(height) as f32).min(1.);
        ((width as f32 * scale).round() as u32).max(1)
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
    // A larger original would decode to tens of megabytes; the CDN copy below
    // is still wider than the viewer draws at, so the loss is invisible.
    if width == 0 || height == 0 || width.max(height) > FULL_WIDTH {
        scaled(src, FULL_WIDTH)
    } else {
        normalize_url(src)
    }
}

fn fit(width: u32, height: u32, viewport: gpui_kit::Size<Pixels>) -> (f32, f32) {
    // Room for the dialog's 16px padding, its close button and window margins.
    let max_width = (f32::from(viewport.width) - 96.).clamp(200., 880.);
    let max_height = (f32::from(viewport.height) - 160.).clamp(160., 660.);
    let (width, height) = if width == 0 || height == 0 {
        (4., 3.)
    } else {
        (width as f32, height as f32)
    };
    let scale = (max_width / width).min(max_height / height).min(1.);
    ((width * scale).round(), (height * scale).round())
}

#[cfg(test)]
mod tests {
    use super::{PictureViewer, copy_url, fit, full_url};
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

    #[test]
    fn fits_pictures_inside_the_window() {
        let viewport = size(px(800.), px(600.));

        let (width, height) = fit(1000, 500, viewport);
        assert!(width <= 704. && height <= 440.);
        assert!((width / height - 2.).abs() < 0.01);

        assert_eq!(fit(320, 240, viewport), (320., 240.));

        let (width, height) = fit(0, 0, viewport);
        assert!(width <= 704. && height <= 440.);
        assert!((width / height - 4. / 3.).abs() < 0.01);
    }

    #[test]
    fn copies_png_at_a_bounded_width() {
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 800, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@800w_1c.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 2000, 600),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@1280w_1c.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 600, 2000),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@384w_1c.png"
        );
        assert_eq!(
            copy_url("//i0.hdslb.com/bfs/new_dyn/a.webp", 0, 0),
            "https://i0.hdslb.com/bfs/new_dyn/a.webp@1280w_1c.png"
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
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w_1c.webp"
        );
        assert_eq!(
            full_url("//i0.hdslb.com/bfs/new_dyn/a.png", 0, 0),
            "https://i0.hdslb.com/bfs/new_dyn/a.png@1280w_1c.webp"
        );
    }
}
