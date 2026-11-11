use std::{
    ffi::{c_int, c_void},
    ptr::{NonNull, null_mut},
};

use super::{
    error::{MpvError, check},
    ffi::{
        MPV_RENDER_FRAME_INFO_PRESENT, MPV_RENDER_FRAME_INFO_REDRAW, MPV_RENDER_PARAM_API_TYPE,
        MPV_RENDER_PARAM_INVALID, MPV_RENDER_PARAM_NEXT_FRAME_INFO, MPV_RENDER_PARAM_SW_FORMAT,
        MPV_RENDER_PARAM_SW_POINTER, MPV_RENDER_PARAM_SW_SIZE, MPV_RENDER_PARAM_SW_STRIDE,
        MpvRenderContext, MpvRenderFrameInfo, MpvRenderParam, mpv_render_context_create,
        mpv_render_context_free, mpv_render_context_get_info, mpv_render_context_render,
        mpv_render_context_report_swap, mpv_render_context_update,
    },
    handle::Mpv,
};

pub(crate) struct RenderContext {
    handle: NonNull<MpvRenderContext>,
}

#[derive(Clone, Copy)]
pub(crate) struct FrameInfo {
    pub(crate) present: bool,
    pub(crate) redraw: bool,
}

impl RenderContext {
    pub(crate) fn new(mpv: &Mpv) -> Result<Self, MpvError> {
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
        NonNull::new(handle)
            .map(|handle| Self { handle })
            .ok_or_else(|| MpvError::invalid_argument("mpv_render_context_create returned null"))
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

    pub(crate) fn render(
        &self,
        width: u32,
        height: u32,
        stride: u32,
        buffer: &mut [u8],
    ) -> Result<(), MpvError> {
        if width == 0 || height == 0 {
            return Err(MpvError::invalid_argument("render size is empty"));
        }
        if buffer.len() < stride as usize * height as usize {
            return Err(MpvError::invalid_argument("render buffer is too small"));
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
                data: buffer.as_mut_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: MPV_RENDER_PARAM_INVALID,
                data: null_mut(),
            },
        ];

        check(unsafe { mpv_render_context_render(self.handle.as_ptr(), params.as_mut_ptr()) })?;
        unsafe { mpv_render_context_report_swap(self.handle.as_ptr()) };
        Ok(())
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe { mpv_render_context_free(self.handle.as_ptr()) };
    }
}
