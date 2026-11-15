rigili_macro::mod_prv!(egl_context);

use egl_context::{EglContext, export_texture, load_proc_address, open_render_node};

use std::{
    ffi::{c_int, c_void},
    fs::File,
    os::fd::AsRawFd,
    ptr::{NonNull, null_mut},
    sync::Arc,
};

use glow::HasContext;
use khronos_egl as egl;

use crate::native::{Dmabuf, NATIVE_SLOTS};

use super::{
    error::{MpvError, check},
    ffi::{
        MPV_RENDER_FRAME_INFO_PRESENT, MPV_RENDER_FRAME_INFO_REDRAW, MPV_RENDER_PARAM_API_TYPE,
        MPV_RENDER_PARAM_DRM_DISPLAY_V2, MPV_RENDER_PARAM_INVALID,
        MPV_RENDER_PARAM_NEXT_FRAME_INFO, MPV_RENDER_PARAM_OPENGL_FBO,
        MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, MpvOpenGlDrmParamsV2, MpvOpenGlFbo,
        MpvOpenGlInitParams, MpvRenderContext, MpvRenderFrameInfo, MpvRenderParam,
        mpv_render_context_create, mpv_render_context_free, mpv_render_context_get_info,
        mpv_render_context_render, mpv_render_context_report_swap,
        mpv_render_context_set_update_callback, mpv_render_context_update,
    },
    handle::Mpv,
    render::{FrameInfo, RenderWake},
};

pub(crate) struct GlRender {
    egl: EglContext,
    render: NonNull<MpvRenderContext>,
    _render_fd: Option<File>,
    slots: [GlSlot; NATIVE_SLOTS],
    size: (u32, u32),
    generation: u64,
}

struct GlSlot {
    framebuffer: glow::Framebuffer,
    texture: glow::Texture,
    dmabuf: Option<Dmabuf>,
    egl_image: Option<egl::Image>,
}

impl GlRender {
    pub(crate) fn new(mpv: &Mpv, wake: Arc<RenderWake>) -> Result<Self, MpvError> {
        let egl = EglContext::new()?;

        let slots = (0..NATIVE_SLOTS)
            .map(|_| create_slot(&egl))
            .collect::<Result<Vec<_>, _>>()?;
        let slots: [GlSlot; NATIVE_SLOTS] = slots
            .try_into()
            .map_err(|_| MpvError::invalid_argument("OpenGL slot count mismatch"))?;

        let mut init_params = MpvOpenGlInitParams {
            get_proc_address: Some(load_proc_address),
            get_proc_address_ctx: (&*egl.egl as *const egl::DynamicInstance<egl::EGL1_5>)
                as *mut c_void,
        };
        let render_fd = open_render_node();
        let mut drm_params = MpvOpenGlDrmParamsV2 {
            fd: -1,
            crtc_id: -1,
            connector_id: -1,
            atomic_request_ptr: null_mut(),
            render_fd: render_fd.as_ref().map_or(-1, |file| file.as_raw_fd()),
        };
        let mut params = vec![
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_API_TYPE,
                data: c"opengl".as_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: &mut init_params as *mut MpvOpenGlInitParams as *mut c_void,
            },
        ];
        if render_fd.is_some() {
            params.push(MpvRenderParam {
                kind: MPV_RENDER_PARAM_DRM_DISPLAY_V2,
                data: &mut drm_params as *mut MpvOpenGlDrmParamsV2 as *mut c_void,
            });
        }
        params.push(MpvRenderParam {
            kind: MPV_RENDER_PARAM_INVALID,
            data: null_mut(),
        });
        let mut render = null_mut();
        check(unsafe {
            mpv_render_context_create(&mut render, mpv.raw_handle(), params.as_mut_ptr())
        })?;
        let render = NonNull::new(render)
            .ok_or_else(|| MpvError::invalid_argument("mpv_render_context_create returned null"))?;

        let mut this = Self {
            egl,
            render,
            _render_fd: render_fd,
            slots,
            size: (0, 0),
            generation: 0,
        };
        unsafe {
            mpv_render_context_set_update_callback(
                this.render.as_ptr(),
                Some(render_update),
                Arc::as_ptr(&wake) as *mut c_void,
            );
        }
        this.resize(2, 2)?;
        Ok(this)
    }

    pub(crate) fn update(&self) -> u64 {
        unsafe { mpv_render_context_update(self.render.as_ptr()) }
    }

    pub(crate) fn next_frame_info(&self) -> FrameInfo {
        let mut info = MpvRenderFrameInfo {
            flags: 0,
            target_time: 0,
        };
        let param = MpvRenderParam {
            kind: MPV_RENDER_PARAM_NEXT_FRAME_INFO,
            data: &mut info as *mut MpvRenderFrameInfo as *mut c_void,
        };
        let code = unsafe { mpv_render_context_get_info(self.render.as_ptr(), param) };
        if code < 0 {
            return FrameInfo {
                present: true,
                redraw: false,
            };
        }

        FrameInfo {
            present: info.flags & MPV_RENDER_FRAME_INFO_PRESENT != 0,
            redraw: info.flags & MPV_RENDER_FRAME_INFO_REDRAW != 0,
        }
    }

    pub(crate) fn render(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        pointer: *mut u8,
    ) -> Result<(), MpvError> {
        if stride != width * 4 {
            return Err(MpvError::invalid_argument(
                "OpenGL output expects a tight stride",
            ));
        }
        self.render_frame(0, width, height)?;

        // mpv renders in GL orientation, so glReadPixels returns rows bottom-up;
        // the resulting buffer is already top-down and FLIP_Y must stay off.
        let length = width as usize * height as usize * 4;
        // SAFETY: the caller guarantees `pointer` has room for `stride * height`
        // bytes, and `stride` is the tight width * 4 checked above.
        let pixels = unsafe { std::slice::from_raw_parts_mut(pointer, length) };
        unsafe {
            self.egl
                .gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.slots[0].framebuffer));
            self.egl.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            self.egl.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::BGRA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(pixels)),
            );
            self.egl.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Ok(())
    }

    fn render_frame(&mut self, slot: usize, width: u32, height: u32) -> Result<(), MpvError> {
        if width == 0 || height == 0 {
            return Err(MpvError::invalid_argument("render size is empty"));
        }
        self.resize(width, height)?;

        let mut fbo = MpvOpenGlFbo {
            fbo: self.slots[slot].framebuffer.0.get() as c_int,
            w: width as c_int,
            h: height as c_int,
            internal_format: 0,
        };
        let mut params = [
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_OPENGL_FBO,
                data: &mut fbo as *mut MpvOpenGlFbo as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_INVALID,
                data: null_mut(),
            },
        ];
        check(unsafe { mpv_render_context_render(self.render.as_ptr(), params.as_mut_ptr()) })?;
        unsafe { mpv_render_context_report_swap(self.render.as_ptr()) };
        Ok(())
    }

    pub(crate) fn render_native(
        &mut self,
        slot: usize,
        width: u32,
        height: u32,
    ) -> Result<(), MpvError> {
        self.render_frame(slot, width, height)?;
        unsafe { self.egl.gl.finish() };
        self.ensure_dmabuf(slot)
    }

    pub(crate) fn dmabuf(&mut self, slot: usize) -> Result<&Dmabuf, MpvError> {
        self.ensure_dmabuf(slot)?;
        Ok(self.slots[slot]
            .dmabuf
            .as_ref()
            .expect("dmabuf was just created"))
    }

    fn ensure_dmabuf(&mut self, slot: usize) -> Result<(), MpvError> {
        if self.slots[slot].dmabuf.is_some() {
            return Ok(());
        }
        self.generation = self.generation.wrapping_add(1);
        let (image, dmabuf) = export_texture(&self.egl, self.slots[slot].texture, self.generation)?;
        self.slots[slot].egl_image = Some(image);
        self.slots[slot].dmabuf = Some(dmabuf);
        Ok(())
    }

    fn clear_dmabufs(&mut self) {
        for slot in &mut self.slots {
            if let Some(image) = slot.egl_image.take() {
                let _ = self.egl.egl.destroy_image(self.egl.display, image);
            }
            slot.dmabuf = None;
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), MpvError> {
        if self.size == (width, height) {
            return Ok(());
        }
        self.clear_dmabufs();

        for slot in &self.slots {
            unsafe {
                self.egl
                    .gl
                    .bind_texture(glow::TEXTURE_2D, Some(slot.texture));
                self.egl.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.egl
                    .gl
                    .bind_framebuffer(glow::FRAMEBUFFER, Some(slot.framebuffer));
                self.egl.gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(slot.texture),
                    0,
                );
                let status = self.egl.gl.check_framebuffer_status(glow::FRAMEBUFFER);
                self.egl.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                self.egl.gl.bind_texture(glow::TEXTURE_2D, None);
                if status != glow::FRAMEBUFFER_COMPLETE {
                    return Err(MpvError::invalid_argument(format!(
                        "OpenGL framebuffer is incomplete: {status:#x}"
                    )));
                }
            }
        }

        self.size = (width, height);
        Ok(())
    }
}

extern "C" fn render_update(data: *mut c_void) {
    // SAFETY: the pointer is the Arc payload owned by the render context and the
    // callback is cleared before that Arc is dropped.
    if let Some(wake) = unsafe { (data as *const RenderWake).as_ref() } {
        wake.signal();
    }
}

impl Drop for GlRender {
    fn drop(&mut self) {
        self.clear_dmabufs();
        unsafe {
            mpv_render_context_set_update_callback(self.render.as_ptr(), None, null_mut());
            mpv_render_context_free(self.render.as_ptr());
            for slot in &self.slots {
                self.egl.gl.delete_framebuffer(slot.framebuffer);
                self.egl.gl.delete_texture(slot.texture);
            }
        }
    }
}

fn create_slot(egl: &EglContext) -> Result<GlSlot, MpvError> {
    unsafe {
        let texture = egl.gl.create_texture().map_err(|error| {
            MpvError::invalid_argument(format!("无法创建 OpenGL 纹理：{error}"))
        })?;
        egl.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        egl.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        egl.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        egl.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        egl.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        egl.gl.bind_texture(glow::TEXTURE_2D, None);

        let framebuffer = egl.gl.create_framebuffer().map_err(|error| {
            MpvError::invalid_argument(format!("无法创建 OpenGL 帧缓冲：{error}"))
        })?;
        egl.gl
            .bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        egl.gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        egl.gl.bind_framebuffer(glow::FRAMEBUFFER, None);

        Ok(GlSlot {
            framebuffer,
            texture,
            dmabuf: None,
            egl_image: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires EGL and a GPU"]
    fn exports_the_texture_as_dmabuf() {
        let context = EglContext::new().expect("egl context");
        let gl = &context.gl;
        let (width, height) = (320u32, 180u32);

        unsafe {
            let texture = gl.create_texture().expect("texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.finish();

            let (image, dmabuf) = export_texture(&context, texture, 1).expect("dmabuf");
            println!(
                "dmabuf fourcc={:#x} modifier={:#x} planes={}",
                dmabuf.fourcc,
                dmabuf.modifier,
                dmabuf.planes.len()
            );
            for plane in &dmabuf.planes {
                println!("stride={} offset={}", plane.stride, plane.offset);
            }

            let _ = context.egl.destroy_image(context.display, image);
            gl.delete_texture(texture);
        }
    }
}
