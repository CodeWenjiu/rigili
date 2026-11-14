use std::{
    ffi::{CStr, c_char, c_int, c_void},
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    ptr::{NonNull, null, null_mut},
    sync::Arc,
};

use glow::HasContext;
use khronos_egl as egl;

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

const EGL_PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31dd;

pub(crate) struct GlRender {
    egl: EglContext,
    render: NonNull<MpvRenderContext>,
    _render_fd: Option<File>,
    framebuffer: glow::Framebuffer,
    texture: glow::Texture,
    size: (u32, u32),
}

impl GlRender {
    pub(crate) fn new(mpv: &Mpv, wake: Arc<RenderWake>) -> Result<Self, MpvError> {
        let egl = EglContext::new()?;

        let (framebuffer, texture) = unsafe {
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
            (framebuffer, texture)
        };

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
            framebuffer,
            texture,
            size: (0, 0),
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
        if width == 0 || height == 0 {
            return Err(MpvError::invalid_argument("render size is empty"));
        }
        if stride != width * 4 {
            return Err(MpvError::invalid_argument(
                "OpenGL output expects a tight stride",
            ));
        }
        self.resize(width, height)?;

        let mut fbo = MpvOpenGlFbo {
            fbo: self.framebuffer.0.get() as c_int,
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

        // mpv renders in GL orientation, so glReadPixels returns rows bottom-up;
        // the resulting buffer is already top-down and FLIP_Y must stay off.
        let length = width as usize * height as usize * 4;
        // SAFETY: the caller guarantees `pointer` has room for `stride * height`
        // bytes, and `stride` is the tight width * 4 checked above.
        let pixels = unsafe { std::slice::from_raw_parts_mut(pointer, length) };
        unsafe {
            self.egl
                .gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
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

    fn resize(&mut self, width: u32, height: u32) -> Result<(), MpvError> {
        if self.size == (width, height) {
            return Ok(());
        }

        unsafe {
            self.egl
                .gl
                .bind_texture(glow::TEXTURE_2D, Some(self.texture));
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
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            self.egl.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.texture),
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

        self.size = (width, height);
        Ok(())
    }
}

struct EglContext {
    egl: Box<egl::DynamicInstance<egl::EGL1_5>>,
    display: egl::Display,
    context: egl::Context,
    gl: glow::Context,
}

impl EglContext {
    fn new() -> Result<Self, MpvError> {
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|error| MpvError::invalid_argument(format!("无法加载 EGL：{error}")))?;
        let egl = Box::new(egl);
        let display = unsafe {
            egl.get_platform_display(
                EGL_PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }
        .map_err(|error| MpvError::invalid_argument(format!("无法创建 EGL 显示器：{error}")))?;
        egl.initialize(display)
            .map_err(|error| MpvError::invalid_argument(format!("无法初始化 EGL：{error}")))?;
        egl.bind_api(egl::OPENGL_API)
            .map_err(|error| MpvError::invalid_argument(format!("无法绑定 OpenGL：{error}")))?;

        let config_attributes = [
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &config_attributes)
            .map_err(|error| MpvError::invalid_argument(format!("无法选择 EGL 配置：{error}")))?
            .ok_or_else(|| MpvError::invalid_argument("没有可用的 EGL 配置"))?;
        let context_attributes = [
            egl::CONTEXT_MAJOR_VERSION,
            3,
            egl::CONTEXT_MINOR_VERSION,
            3,
            egl::CONTEXT_OPENGL_PROFILE_MASK,
            egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            egl::NONE,
        ];
        let context = match egl.create_context(display, config, None, &context_attributes) {
            Ok(context) => context,
            Err(error) => {
                let _ = egl.terminate(display);
                return Err(MpvError::invalid_argument(format!(
                    "无法创建 OpenGL 上下文：{error}"
                )));
            }
        };
        if let Err(error) = egl.make_current(display, None, None, Some(context)) {
            let _ = egl.destroy_context(display, context);
            let _ = egl.terminate(display);
            return Err(MpvError::invalid_argument(format!(
                "无法激活 OpenGL 上下文：{error}"
            )));
        }

        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                egl.get_proc_address(name)
                    .map_or(null(), |proc| proc as *const () as *const c_void)
            })
        };

        Ok(Self {
            egl,
            display,
            context,
            gl,
        })
    }
}

impl Drop for EglContext {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
    }
}

// VAAPI GL interop needs a render node even though the EGL context is
// surfaceless, so hand mpv the first usable node.
fn open_render_node() -> Option<File> {
    (128..=135).find_map(|index| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/dri/renderD{index}"))
            .ok()
    })
}

extern "C" fn load_proc_address(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: `context` points to the boxed EGL instance owned by EglContext and
    // `name` is a NUL-terminated string provided by libmpv.
    let egl = unsafe { &*(context as *const egl::DynamicInstance<egl::EGL1_5>) };
    let name = unsafe { CStr::from_ptr(name) };
    match name
        .to_str()
        .ok()
        .and_then(|name| egl.get_proc_address(name))
    {
        Some(proc) => proc as *const () as *mut c_void,
        None => null_mut(),
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
        unsafe {
            mpv_render_context_set_update_callback(self.render.as_ptr(), None, null_mut());
            mpv_render_context_free(self.render.as_ptr());
            self.egl.gl.delete_framebuffer(self.framebuffer);
            self.egl.gl.delete_texture(self.texture);
        }
    }
}
