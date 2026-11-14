use std::{ffi::c_void, os::fd::AsFd};

use rigili_player::{NATIVE_SLOTS, NativeFrame};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{Backend, ObjectId},
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_registry, wl_surface},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
};

pub(super) struct WaylandOutput {
    connection: Connection,
    queue: EventQueue<OutputState>,
    state: OutputState,
    surface: wl_surface::WlSurface,
    dmabuf: ZwpLinuxDmabufV1,
    buffers: [Option<CachedBuffer>; NATIVE_SLOTS],
}

struct CachedBuffer {
    generation: u64,
    width: u32,
    height: u32,
    buffer: wl_buffer::WlBuffer,
    params: ZwpLinuxBufferParamsV1,
}

#[derive(Default)]
struct OutputState {
    released: Vec<u8>,
    failed: bool,
}

impl WaylandOutput {
    pub(super) fn new(display: *mut c_void, surface: *mut c_void) -> Result<Self, String> {
        // SAFETY: the display belongs to the live GPUI Wayland connection and
        // the backend never disconnects a foreign display.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        // SAFETY: the surface was created on the same connection by GPUI.
        let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface.cast()) }
            .map_err(|error| format!("invalid wl_surface: {error}"))?;
        let surface = wl_surface::WlSurface::from_id(&connection, id)
            .map_err(|error| format!("invalid wl_surface id: {error}"))?;
        let (globals, queue) = registry_queue_init::<OutputState>(&connection)
            .map_err(|error| format!("wayland registry: {error}"))?;
        let qh = queue.handle();
        let dmabuf = globals
            .bind::<ZwpLinuxDmabufV1, OutputState, ()>(&qh, 2..=4, ())
            .map_err(|error| format!("linux-dmabuf unavailable: {error}"))?;

        Ok(Self {
            connection,
            queue,
            state: OutputState::default(),
            surface,
            dmabuf,
            buffers: std::array::from_fn(|_| None),
        })
    }

    /// Dispatches compositor events and returns the slots it released.
    pub(super) fn poll(&mut self) -> Result<Vec<u8>, String> {
        let _ = self.connection.flush();
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| format!("wayland dispatch: {error}"))?;
        if self.state.failed {
            return Err("the compositor rejected the dmabuf".into());
        }
        Ok(std::mem::take(&mut self.state.released))
    }

    pub(super) fn present(&mut self, frame: &NativeFrame) -> Result<(), String> {
        let slot = frame.slot as usize;
        if slot >= NATIVE_SLOTS {
            return Err(format!("invalid native frame slot {slot}"));
        }
        let stale = self.buffers[slot].as_ref().is_none_or(|buffer| {
            buffer.generation != frame.dmabuf.generation
                || buffer.width != frame.width
                || buffer.height != frame.height
        });
        if stale {
            self.create_buffer(slot, frame)?;
        }

        let buffer = &self.buffers[slot]
            .as_ref()
            .ok_or("missing wl_buffer")?
            .buffer;
        self.surface.attach(Some(buffer), 0, 0);
        if self.surface.version() >= 4 {
            self.surface
                .damage_buffer(0, 0, frame.width as i32, frame.height as i32);
        } else {
            self.surface
                .damage(0, 0, frame.width as i32, frame.height as i32);
        }
        self.surface.commit();
        let _ = self.connection.flush();
        Ok(())
    }

    /// Applies pending geometry set by the compositor or the window.
    pub(super) fn commit(&self) {
        self.surface.commit();
        let _ = self.connection.flush();
    }

    fn create_buffer(&mut self, slot: usize, frame: &NativeFrame) -> Result<(), String> {
        if let Some(buffer) = self.buffers[slot].take() {
            buffer.buffer.destroy();
            buffer.params.destroy();
        }

        let params: ZwpLinuxBufferParamsV1 = self.dmabuf.create_params(&self.queue.handle(), ());
        for (index, plane) in frame.dmabuf.planes.iter().enumerate() {
            params.add(
                plane.fd.as_fd(),
                index as u32,
                plane.offset,
                plane.stride,
                (frame.dmabuf.modifier >> 32) as u32,
                (frame.dmabuf.modifier & 0xffff_ffff) as u32,
            );
        }
        let buffer = params.create_immed(
            frame.width as i32,
            frame.height as i32,
            frame.dmabuf.fourcc,
            zwp_linux_buffer_params_v1::Flags::empty(),
            &self.queue.handle(),
            slot as u8,
        );

        self.buffers[slot] = Some(CachedBuffer {
            generation: frame.dmabuf.generation,
            width: frame.width,
            height: frame.height,
            buffer,
            params,
        });
        Ok(())
    }
}

impl Drop for WaylandOutput {
    fn drop(&mut self) {
        for buffer in self.buffers.iter_mut().filter_map(Option::take) {
            buffer.buffer.destroy();
            buffer.params.destroy();
        }
        self.dmabuf.destroy();
        let _ = self.connection.flush();
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for OutputState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for OutputState {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxDmabufV1,
        _: zwp_linux_dmabuf_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for OutputState {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_linux_buffer_params_v1::Event::Failed) {
            state.failed = true;
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, u8> for OutputState {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        slot: &u8,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.released.push(*slot);
        }
    }
}
