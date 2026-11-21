use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use gpui_kit::ImageCache;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::image_cache::LruImageCache;

const MIN_FRAME_DELAY: Duration = Duration::from_millis(20);

struct AnimationState {
    source: u64,
    frame: usize,
    tick: u64,
    counter: Arc<AtomicU64>,
    timer: Option<Task<()>>,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            source: 0,
            frame: 0,
            tick: 0,
            counter: Arc::new(AtomicU64::new(0)),
            timer: None,
        }
    }
}

pub(super) struct LayoutState {
    image: Option<Arc<RenderImage>>,
    frame: usize,
}

pub(super) struct AnimatedImage<C: ImageCache = LruImageCache> {
    id: ElementId,
    source: SharedUri,
    cache: Entity<C>,
    radius: Pixels,
}

impl<C: ImageCache> AnimatedImage<C> {
    pub(super) fn new(id: impl Into<ElementId>, source: SharedUri, cache: Entity<C>) -> Self {
        Self {
            id: id.into(),
            source,
            cache,
            radius: px(0.),
        }
    }

    pub(super) fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    fn style(&self) -> Style {
        Style {
            size: size(
                Length::Definite(relative(1.)),
                Length::Definite(relative(1.)),
            ),
            ..Default::default()
        }
    }
}

impl<C: ImageCache> Element for AnimatedImage<C> {
    type RequestLayoutState = LayoutState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let resource = Resource::Uri(self.source.clone());
        let loaded = self
            .cache
            .update(cx, |cache, cx| cache.load(&resource, window, cx));
        let image = loaded.and_then(Result::ok);

        let layout_id = window.request_layout(self.style(), std::iter::empty(), cx);
        let source = hash(&self.source);
        let view = window.current_view();

        let frame = window.with_optional_element_state(global_id, |slot, window| {
            let tracked = slot.is_some();
            let mut state: AnimationState = slot.flatten().unwrap_or_default();
            if state.source != source {
                state = AnimationState {
                    source,
                    ..Default::default()
                };
            }

            if let Some(image) = &image {
                let count = image.frame_count();
                if count > 1 && window.is_window_active() && !cx.reduce_motion() {
                    let ticks = state.counter.load(Ordering::Relaxed);
                    if ticks != state.tick {
                        state.tick = ticks;
                        state.frame = (state.frame + 1) % count;
                        state.timer = None;
                    }
                    if state.timer.is_none() {
                        let delay = frame_delay(image, state.frame);
                        let counter = state.counter.clone();
                        let executor = cx.background_executor().clone();
                        state.timer = Some(cx.spawn(async move |cx| {
                            executor.timer(delay).await;
                            counter.fetch_add(1, Ordering::Relaxed);
                            cx.update(|cx| cx.notify(view));
                        }));
                    }
                } else {
                    state.tick = state.counter.load(Ordering::Relaxed);
                    state.timer = None;
                    if count <= 1 {
                        state.frame = 0;
                    }
                }
            } else {
                state.tick = state.counter.load(Ordering::Relaxed);
                state.frame = 0;
                state.timer = None;
            }

            let frame = state.frame.min(
                image
                    .as_ref()
                    .map_or(0, |image| image.frame_count().saturating_sub(1)),
            );
            (frame, tracked.then_some(state))
        });

        (layout_id, LayoutState { image, frame })
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let Some(image) = &request_layout.image else {
            return;
        };
        let count = image.frame_count();
        if count == 0 {
            return;
        }
        let frame = request_layout.frame.min(count - 1);
        let image_bounds = ObjectFit::Contain.get_bounds(bounds, image.size(frame));
        let _ = window.paint_image(
            bounds,
            image_bounds,
            Corners::all(self.radius),
            image.clone(),
            frame,
            false,
        );
    }
}

fn frame_delay(image: &RenderImage, frame: usize) -> Duration {
    Duration::from(image.delay(frame)).max(MIN_FRAME_DELAY)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::{AnimatedImage, MIN_FRAME_DELAY, frame_delay};
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        App, Bounds, Entity, ImageCache, ImageCacheError, RenderImage, Resource, SharedUri,
        TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use image::{Delay, Frame, Rgba, RgbaImage};

    fn frame(delay: Delay) -> Frame {
        Frame::from_parts(RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 0])), 0, 0, delay)
    }

    struct MockCache {
        image: Arc<RenderImage>,
    }

    impl ImageCache for MockCache {
        fn load(
            &mut self,
            _resource: &Resource,
            _window: &mut Window,
            _cx: &mut App,
        ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
            Some(Ok(self.image.clone()))
        }
    }

    struct Harness {
        cache: Entity<MockCache>,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size(px(16.)).child(AnimatedImage::new(
                "animated-image",
                SharedUri::from("test://image"),
                self.cache.clone(),
            ))
        }
    }

    #[test]
    fn clamps_short_and_missing_delays() {
        let image = RenderImage::new([frame(Delay::from_numer_denom_ms(0, 1))]);
        assert_eq!(frame_delay(&image, 0), MIN_FRAME_DELAY);

        let image = RenderImage::new([frame(Delay::from_numer_denom_ms(10, 1))]);
        assert_eq!(frame_delay(&image, 0), MIN_FRAME_DELAY);
    }

    #[test]
    fn keeps_longer_delays() {
        let image = RenderImage::new([frame(Delay::from_numer_denom_ms(100, 1))]);
        assert_eq!(frame_delay(&image, 0), Duration::from_millis(100));
    }

    #[gpui_kit::test]
    fn advances_frames_on_their_own_delay(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let image = Arc::new(RenderImage::new(vec![
            frame(Delay::from_numer_denom_ms(100, 1)),
            frame(Delay::from_numer_denom_ms(100, 1)),
        ]));
        let cache = cx.update(|cx| {
            cx.new(|_| MockCache {
                image: image.clone(),
            })
        });
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(200.), px(200.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| Harness { cache }),
            )
            .expect("open animated image window")
        });

        cx.update_window(window, |_, window, _| window.activate_window())
            .expect("activate window");
        cx.run_until_parked();

        let painted = |window: &Window| window.has_image_atlas_entry(&image);
        assert!(
            !cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                painted(window)
            })
            .expect("first frame"),
            "only the first frame should be painted before its delay"
        );

        cx.background_executor
            .advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert!(
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                painted(window)
            })
            .expect("second frame"),
            "the next frame should be painted once its delay elapses"
        );
    }
}
