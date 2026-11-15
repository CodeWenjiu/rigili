use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle, RawWindowHandle};
use rigili_player::{PlaybackPhase, VideoSize};

use super::PlayerView;
use crate::native::WaylandOutput;

pub(super) struct NativeSurface {
    pub(super) composition: WindowCompositionSurface,
    pub(super) output: WaylandOutput,
    pub(super) visible: bool,
}

impl PlayerView {
    pub(super) fn pump_native(&mut self, window: &Window) -> bool {
        let Some(mut native) = self.native.take() else {
            return false;
        };
        let mut changed = false;
        let mut failure = None;

        if self.native_requested {
            match native.output.poll() {
                Ok(released) => {
                    for slot in released {
                        self.player.native_frame_released(slot);
                    }
                }
                Err(error) => failure = Some(error),
            }

            if failure.is_none() {
                let frames = self.player.take_native_frames();
                if !native.visible && self.status.phase == PlaybackPhase::Loading {
                    for frame in &frames {
                        self.player.native_frame_released(frame.slot);
                    }
                } else if let Some((frame, skipped)) = frames.split_last() {
                    for skipped in skipped {
                        self.player.native_frame_released(skipped.slot);
                    }
                    match native.output.present(frame) {
                        Ok(()) => {
                            if !native.visible {
                                if let Some(bounds) = self.video_bounds.get().map(|area| {
                                    contain_rect(area, VideoSize::new(frame.width, frame.height))
                                        .to_device_pixels(window.scale_factor())
                                }) {
                                    if let Ok(surface) = native.composition.platform_surface() {
                                        let _ = surface.set_bounds(bounds);
                                    }
                                    self.native_bounds = Some(bounds);
                                }
                                let _ = native
                                    .composition
                                    .platform_surface()
                                    .map(|surface| surface.set_visible(true));
                                native.output.commit();
                                native.visible = true;
                                changed = true;
                            }
                        }
                        Err(error) => failure = Some(error),
                    }
                }
            }

            if failure.is_none() && self.player.native_output_failed() {
                failure = Some("the renderer cannot export GPU frames".into());
            }
        }

        if let Some(error) = failure {
            self.native_failed = true;
            self.native_requested = false;
            self.player.set_native_output(false);
            if let Ok(surface) = native.composition.platform_surface() {
                let _ = surface.set_visible(false);
            }
            native.visible = false;
            changed = true;
            eprintln!("direct GPU output unavailable, using readback: {error}");
        }

        self.native = Some(native);
        changed
    }

    pub(super) fn sync_native_bounds(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.native_requested || self.native_failed {
            return;
        }
        let Some(area) = self.video_bounds.get() else {
            return;
        };
        let Some(video_size) = self.status.video_size else {
            return;
        };
        let rect = contain_rect(area, video_size);
        let bounds = rect.to_device_pixels(window.scale_factor());
        if self.native_bounds == Some(bounds) {
            return;
        }
        self.native_bounds = Some(bounds);
        let Some(native) = &self.native else {
            return;
        };
        if let Ok(surface) = native.composition.platform_surface() {
            if surface.set_bounds(bounds).is_err() {
                self.native_failed = true;
                self.native_requested = false;
                self.player.set_native_output(false);
                let _ = surface.set_visible(false);
                return;
            }
            native.output.commit();
        }
        cx.notify();
    }
}

pub(super) fn create_native_surface(window: &mut Window) -> Option<NativeSurface> {
    let surface = window
        .enable_window_composition()
        .ok()
        .and_then(|composition| composition.create_native_surface().ok())?;
    let handle = surface.platform_surface().ok()?.platform_handle().ok()?;
    let handle = handle.downcast::<RawWindowHandle>().ok()?;
    let RawWindowHandle::Wayland(surface_handle) = *handle else {
        return None;
    };
    let display = window.display_handle().ok()?;
    let RawDisplayHandle::Wayland(display_handle) = display.as_raw() else {
        return None;
    };
    let output = WaylandOutput::new(
        display_handle.display.as_ptr(),
        surface_handle.surface.as_ptr(),
    )
    .map_err(|error| eprintln!("direct GPU output unavailable: {error}"))
    .ok()?;
    Some(NativeSurface {
        composition: surface,
        output,
        visible: false,
    })
}

fn contain_rect(area: Bounds<Pixels>, source: VideoSize) -> Bounds<Pixels> {
    let area_width = f32::from(area.size.width);
    let area_height = f32::from(area.size.height);
    let scale = (area_width / source.width as f32).min(area_height / source.height as f32);
    let width = source.width as f32 * scale;
    let height = source.height as f32 * scale;
    Bounds::new(
        point(
            area.origin.x + px((area_width - width) / 2.0),
            area.origin.y + px((area_height - height) / 2.0),
        ),
        size(px(width), px(height)),
    )
}
