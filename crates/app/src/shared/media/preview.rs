use std::{sync::Arc, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::*;
use image::RgbaImage;
use rigili_bilibili::ApiError;
use rigili_core::client;
use rigili_core::runtime::Tokio;
use rigili_playback::resolve_preview;
use rigili_player::{Player, VideoFrame, VideoSize};

const HOVER_DELAY: Duration = Duration::from_millis(400);
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone)]
pub(crate) struct PreviewSource {
    bvid: SharedString,
    cid: Option<u64>,
}

impl PreviewSource {
    pub(crate) fn new(bvid: SharedString, cid: Option<u64>) -> Self {
        Self { bvid, cid }
    }
}

pub(crate) struct PreviewSlot {
    source: PreviewSource,
    hovered: bool,
    generation: u64,
    task: Option<Task<()>>,
    surface: Option<Entity<PreviewSurface>>,
}

impl PreviewSlot {
    fn new(source: PreviewSource) -> Self {
        Self {
            source,
            hovered: false,
            generation: 0,
            task: None,
            surface: None,
        }
    }

    pub(crate) fn set_hovered(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.hovered == hovered {
            return;
        }

        if hovered {
            self.hovered = true;
            self.generation += 1;
            self.task = None;
            self.schedule(window, cx);
        } else {
            self.stop(Some(window), cx);
        }
    }

    pub(crate) fn stop(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.hovered = false;
        self.generation += 1;
        self.task = None;
        if let Some(surface) = self.surface.take() {
            surface.update(cx, |surface, cx| surface.release(window, cx));
            cx.notify();
        }
    }

    fn schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.generation;
        self.task = Some(cx.spawn_in(window, async move |slot, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = slot.update_in(cx, |this, window, cx| {
                if this.generation == generation && this.hovered {
                    this.start(window, cx);
                }
            });
        }));
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        let Ok(client) = client::get(cx) else {
            return;
        };

        let generation = self.generation;
        let bvid = self.source.bvid.to_string();
        let cid = self.source.cid;
        let request = Tokio::spawn(cx, async move { resolve_preview(client, bvid, cid).await });

        self.task = Some(cx.spawn_in(window, async move |slot, cx| {
            let result = request.await;
            let _ = slot.update_in(cx, |this, window, cx| {
                if this.generation == generation && this.hovered {
                    this.finish(result, window, cx);
                }
            });
        }));
    }

    fn finish(
        &mut self,
        result: Result<Result<String, ApiError>, tokio::task::JoinError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        let video = match result {
            Ok(Ok(video)) => video,
            Ok(Err(_)) | Err(_) => return,
        };

        let surface = cx.new(|cx| PreviewSurface::new(video, window, cx));
        let slot = cx.entity().downgrade();
        if let Some(state) = cx.try_global::<PreviewState>() {
            let arbiter = state.arbiter.clone();
            arbiter.update(cx, |arbiter, cx| arbiter.claim(slot, window, cx));
        }
        self.surface = Some(surface);
        cx.notify();
    }
}

impl Render for PreviewSlot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .when_some(self.surface.clone(), |this, surface| this.child(surface))
    }
}

struct PreviewSurface {
    player: Player,
    frame: Option<Arc<RenderImage>>,
    _poll: Task<()>,
}

impl PreviewSurface {
    fn new(video: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let player = Player::new_software();
        player.set_volume(0.0);
        player.load(&video, None);

        let poll = cx.spawn_in(window, async move |surface, cx| {
            loop {
                cx.background_executor().timer(FRAME_INTERVAL).await;
                let alive = surface
                    .update_in(cx, |surface, window, cx| surface.pump(window, cx))
                    .is_ok();
                if !alive {
                    break;
                }
            }
        });

        Self {
            player,
            frame: None,
            _poll: poll,
        }
    }

    fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(frame) = self.player.take_frame() else {
            return;
        };
        let Some(image) = render_image(frame) else {
            return;
        };

        if let Some(previous) = self.frame.replace(image) {
            let _ = window.drop_image(previous);
        }
        cx.notify();
    }

    fn release(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        if let Some(frame) = self.frame.take() {
            cx.drop_image(frame, window);
        }
    }
}

impl Render for PreviewSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let player = self.player.clone();

        div()
            .size_full()
            .children(
                self.frame
                    .clone()
                    .map(|frame| img(frame).size_full().object_fit(ObjectFit::Cover)),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        let scale = window.scale_factor();
                        player.set_viewport(VideoSize::new(
                            (f32::from(bounds.size.width) * scale).round().max(0.) as u32,
                            (f32::from(bounds.size.height) * scale).round().max(0.) as u32,
                        ));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

#[derive(Default)]
struct PreviewArbiter {
    active: Option<WeakEntity<PreviewSlot>>,
}

impl PreviewArbiter {
    fn claim(
        &mut self,
        slot: WeakEntity<PreviewSlot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.entity_id() == slot.entity_id())
        {
            return;
        }

        if let Some(previous) = self.active.replace(slot) {
            let _ = previous.update(cx, |slot, cx| slot.stop(Some(window), cx));
        }
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.active.take() {
            let _ = active.update(cx, |slot, cx| slot.stop(None, cx));
        }
    }
}

struct PreviewState {
    arbiter: Entity<PreviewArbiter>,
}

impl Global for PreviewState {}

pub(crate) fn init(cx: &mut App) {
    if cx.has_global::<PreviewState>() {
        return;
    }

    let arbiter = cx.new(|_| PreviewArbiter::default());
    cx.set_global(PreviewState { arbiter });
}

pub(crate) fn slot(
    window: &mut Window,
    cx: &mut App,
    id: ElementId,
    source: PreviewSource,
) -> Entity<PreviewSlot> {
    window.use_keyed_state(id, cx, |_, _| PreviewSlot::new(source))
}

pub(crate) fn stop_all(cx: &mut App) {
    let Some(state) = cx.try_global::<PreviewState>() else {
        return;
    };
    let arbiter = state.arbiter.clone();
    arbiter.update(cx, |arbiter, cx| arbiter.stop(cx));
}

fn render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(frame.width, frame.height, frame.data)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}
