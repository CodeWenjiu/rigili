use std::{
    ffi::{c_int, c_void},
    ptr::{NonNull, null_mut},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use super::{
    error::{MpvError, check},
    ffi::{
        MPV_RENDER_FRAME_INFO_PRESENT, MPV_RENDER_FRAME_INFO_REDRAW, MPV_RENDER_PARAM_API_TYPE,
        MPV_RENDER_PARAM_INVALID, MPV_RENDER_PARAM_NEXT_FRAME_INFO, MPV_RENDER_PARAM_SW_FORMAT,
        MPV_RENDER_PARAM_SW_POINTER, MPV_RENDER_PARAM_SW_SIZE, MPV_RENDER_PARAM_SW_STRIDE,
        MpvRenderContext, MpvRenderFrameInfo, MpvRenderParam, mpv_render_context_create,
        mpv_render_context_free, mpv_render_context_get_info, mpv_render_context_render,
        mpv_render_context_report_swap, mpv_render_context_set_update_callback,
        mpv_render_context_update,
    },
    gl::GlRender,
    handle::Mpv,
};

pub(crate) enum Renderer {
    Sw(RenderContext),
    Gl(Box<GlRender>),
}

impl Renderer {
    pub(crate) fn new(mpv: &Mpv, wake: Arc<RenderWake>) -> Result<Self, MpvError> {
        // GPU rendering is much cheaper, but playback must keep working when
        // EGL or VAAPI interop is unavailable.
        match GlRender::new(mpv, wake.clone()) {
            Ok(gl) => Ok(Self::Gl(Box::new(gl))),
            Err(_) => Self::new_software(mpv, wake),
        }
    }

    pub(crate) fn new_software(mpv: &Mpv, wake: Arc<RenderWake>) -> Result<Self, MpvError> {
        RenderContext::new(mpv, wake).map(Self::Sw)
    }

    pub(crate) fn update(&self) -> u64 {
        match self {
            Self::Sw(render) => render.update(),
            Self::Gl(render) => render.update(),
        }
    }

    pub(crate) fn next_frame_info(&self) -> FrameInfo {
        match self {
            Self::Sw(render) => render.next_frame_info(),
            Self::Gl(render) => render.next_frame_info(),
        }
    }

    pub(crate) fn render(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        pointer: *mut u8,
    ) -> Result<(), MpvError> {
        match self {
            Self::Sw(render) => render.render(width, height, stride, pointer),
            Self::Gl(render) => render.render(width, height, stride, pointer),
        }
    }
}

pub(crate) struct RenderContext {
    handle: NonNull<MpvRenderContext>,
    wake: Arc<RenderWake>,
}

/// Wakes the worker when mpv has a new frame or a command arrives.
#[derive(Default)]
pub(crate) struct RenderWake {
    signaled: Mutex<bool>,
    condvar: Condvar,
}

impl RenderWake {
    pub(crate) fn signal(&self) {
        let mut signaled = self
            .signaled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *signaled = true;
        self.condvar.notify_one();
    }

    pub(crate) fn wait(&self, timeout: Duration) {
        let mut signaled = self
            .signaled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*signaled {
            let (guard, _) = self
                .condvar
                .wait_timeout(signaled, timeout)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            signaled = guard;
        }
        *signaled = false;
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FrameInfo {
    pub(crate) present: bool,
    pub(crate) redraw: bool,
}

impl RenderContext {
    pub(crate) fn new(mpv: &Mpv, wake: Arc<RenderWake>) -> Result<Self, MpvError> {
        let mut handle: *mut MpvRenderContext = null_mut();
        let mut params = [
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_API_TYPE,
                data: c"sw".as_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_INVALID,
                data: null_mut(),
            },
        ];
        check(unsafe {
            mpv_render_context_create(&mut handle, mpv.raw_handle(), params.as_mut_ptr())
        })?;
        let handle = NonNull::new(handle)
            .ok_or_else(|| MpvError::invalid_argument("mpv_render_context_create returned null"))?;
        let context = Self { handle, wake };
        unsafe {
            mpv_render_context_set_update_callback(
                context.handle.as_ptr(),
                Some(render_update),
                Arc::as_ptr(&context.wake) as *mut c_void,
            );
        }
        Ok(context)
    }

    pub(crate) fn update(&self) -> u64 {
        unsafe { mpv_render_context_update(self.handle.as_ptr()) }
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
        let code = unsafe { mpv_render_context_get_info(self.handle.as_ptr(), param) };
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

    /// Render into `pointer`, which must have room for `stride * height` bytes.
    pub(crate) fn render(
        &self,
        width: u32,
        height: u32,
        stride: u32,
        pointer: *mut u8,
    ) -> Result<(), MpvError> {
        if width == 0 || height == 0 {
            return Err(MpvError::invalid_argument("render size is empty"));
        }

        let mut size = [width as c_int, height as c_int];
        let mut stride = stride as usize;
        let mut params = [
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_SW_SIZE,
                data: size.as_mut_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_SW_FORMAT,
                data: c"bgr0".as_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_SW_STRIDE,
                data: &mut stride as *mut usize as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_SW_POINTER,
                data: pointer as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_INVALID,
                data: null_mut(),
            },
        ];

        check(unsafe { mpv_render_context_render(self.handle.as_ptr(), params.as_mut_ptr()) })?;
        unsafe { mpv_render_context_report_swap(self.handle.as_ptr()) };

        // SAFETY: mpv writes the colour bytes of every pixel, so only the padding
        // byte of each pixel needs to be filled before the buffer is exposed.
        unsafe {
            for offset in (3..stride * height as usize).step_by(4) {
                pointer.add(offset).write(255);
            }
        }
        Ok(())
    }
}

extern "C" fn render_update(data: *mut c_void) {
    // SAFETY: the pointer is the Arc payload owned by the RenderContext and the
    // callback is cleared before that Arc is dropped.
    if let Some(wake) = unsafe { (data as *const RenderWake).as_ref() } {
        wake.signal();
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            mpv_render_context_set_update_callback(self.handle.as_ptr(), None, null_mut());
            mpv_render_context_free(self.handle.as_ptr());
        }
    }
}
