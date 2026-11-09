use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::{NonNull, null_mut},
};

use thiserror::Error;

const MPV_FORMAT_STRING: c_int = 1;
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_INT64: c_int = 4;
const MPV_FORMAT_DOUBLE: c_int = 5;

const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
const MPV_RENDER_PARAM_SW_SIZE: c_int = 17;
const MPV_RENDER_PARAM_SW_FORMAT: c_int = 18;
const MPV_RENDER_PARAM_SW_STRIDE: c_int = 19;
const MPV_RENDER_PARAM_SW_POINTER: c_int = 20;
const MPV_RENDER_PARAM_INVALID: c_int = 0;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const OPTIONS: &[(&str, &str)] = &[
    ("vo", "libmpv"),
    ("hwdec", "auto-safe"),
    ("idle", "yes"),
    ("keep-open", "yes"),
    ("terminal", "no"),
    ("msg-level", "all=warn"),
    ("audio-display", "no"),
    ("osc", "no"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    ("user-agent", USER_AGENT),
    ("referrer", "https://www.bilibili.com/"),
];

#[repr(C)]
struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvRenderContext {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvRenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct MpvEvent {
    pub(crate) event_id: c_int,
    pub(crate) error: c_int,
    pub(crate) reply_userdata: u64,
    pub(crate) data: *mut c_void,
}

#[link(name = "mpv")]
unsafe extern "C" {
    fn mpv_create() -> *mut MpvHandle;
    fn mpv_initialize(ctx: *mut MpvHandle) -> c_int;
    fn mpv_terminate_destroy(ctx: *mut MpvHandle);
    fn mpv_set_option_string(ctx: *mut MpvHandle, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_command(ctx: *mut MpvHandle, args: *const *const c_char) -> c_int;
    fn mpv_set_property(
        ctx: *mut MpvHandle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    fn mpv_get_property(
        ctx: *mut MpvHandle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    fn mpv_wait_event(ctx: *mut MpvHandle, timeout: f64) -> *mut MpvEvent;
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_free(data: *mut c_void);
    fn mpv_render_context_create(
        result: *mut *mut MpvRenderContext,
        mpv: *mut MpvHandle,
        params: *mut MpvRenderParam,
    ) -> c_int;
    fn mpv_render_context_update(ctx: *mut MpvRenderContext) -> u64;
    fn mpv_render_context_render(
        ctx: *mut MpvRenderContext,
        params: *mut MpvRenderParam,
    ) -> c_int;
    fn mpv_render_context_report_swap(ctx: *mut MpvRenderContext);
    fn mpv_render_context_free(ctx: *mut MpvRenderContext);
}

#[derive(Debug, Error)]
#[error("libmpv error {code}: {message}")]
pub(crate) struct MpvError {
    code: c_int,
    message: String,
}

impl MpvError {
    pub(crate) fn from_code(code: c_int) -> Self {
        Self {
            code,
            message: error_string(code),
        }
    }

    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: -1,
            message: message.into(),
        }
    }
}

fn error_string(code: c_int) -> String {
    let message = unsafe { mpv_error_string(code) };
    if message.is_null() {
        format!("unknown error {code}")
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}

fn check(code: c_int) -> Result<(), MpvError> {
    if code < 0 {
        Err(MpvError::from_code(code))
    } else {
        Ok(())
    }
}

fn c_string(value: &str) -> Result<CString, MpvError> {
    CString::new(value).map_err(|_| MpvError::invalid_argument("argument contains NUL"))
}

pub(crate) struct Mpv {
    handle: NonNull<MpvHandle>,
}

impl Mpv {
    pub(crate) fn new() -> Result<Self, MpvError> {
        let handle = NonNull::new(unsafe { mpv_create() })
            .ok_or_else(|| MpvError::invalid_argument("mpv_create returned null"))?;
        let mpv = Self { handle };

        for (name, value) in OPTIONS {
            mpv.set_option_string(name, value)?;
        }

        check(unsafe { mpv_initialize(mpv.handle.as_ptr()) })?;
        Ok(mpv)
    }

    fn set_option_string(&self, name: &str, value: &str) -> Result<(), MpvError> {
        let name = c_string(name)?;
        let value = c_string(value)?;
        check(unsafe { mpv_set_option_string(self.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })
    }

    pub(crate) fn command(&self, args: &[&str]) -> Result<(), MpvError> {
        let args = args
            .iter()
            .map(|arg| c_string(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers = args.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        pointers.push(std::ptr::null());
        check(unsafe { mpv_command(self.handle.as_ptr(), pointers.as_ptr()) })
    }

    pub(crate) fn set_property_double(&self, name: &str, value: f64) -> Result<(), MpvError> {
        let name = c_string(name)?;
        let mut value = value;
        check(unsafe {
            mpv_set_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_DOUBLE,
                &mut value as *mut f64 as *mut c_void,
            )
        })
    }

    pub(crate) fn set_property_flag(&self, name: &str, value: bool) -> Result<(), MpvError> {
        let name = c_string(name)?;
        let mut value = c_int::from(value);
        check(unsafe {
            mpv_set_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_FLAG,
                &mut value as *mut c_int as *mut c_void,
            )
        })
    }

    pub(crate) fn get_property_double(&self, name: &str) -> Option<f64> {
        let name = c_string(name).ok()?;
        let mut value = 0.0f64;
        let code = unsafe {
            mpv_get_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_DOUBLE,
                &mut value as *mut f64 as *mut c_void,
            )
        };
        (code >= 0).then_some(value)
    }

    pub(crate) fn get_property_i64(&self, name: &str) -> Option<i64> {
        let name = c_string(name).ok()?;
        let mut value = 0i64;
        let code = unsafe {
            mpv_get_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_INT64,
                &mut value as *mut i64 as *mut c_void,
            )
        };
        (code >= 0).then_some(value)
    }

    pub(crate) fn get_property_flag(&self, name: &str) -> Option<bool> {
        let name = c_string(name).ok()?;
        let mut value = 0 as c_int;
        let code = unsafe {
            mpv_get_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_FLAG,
                &mut value as *mut c_int as *mut c_void,
            )
        };
        (code >= 0).then_some(value != 0)
    }

    pub(crate) fn get_property_string(&self, name: &str) -> Option<String> {
        let name = c_string(name).ok()?;
        let mut value: *mut c_char = null_mut();
        let code = unsafe {
            mpv_get_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_STRING,
                &mut value as *mut *mut c_char as *mut c_void,
            )
        };
        if code < 0 || value.is_null() {
            return None;
        }

        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { mpv_free(value as *mut c_void) };
        Some(result)
    }

    pub(crate) fn video_size(&self) -> Option<(u32, u32)> {
        let width = self.get_property_i64("video-params/w")?;
        let height = self.get_property_i64("video-params/h")?;
        (width > 0 && height > 0).then_some((width as u32, height as u32))
    }

    pub(crate) fn wait_event(&self, timeout: f64) -> MpvEvent {
        unsafe { *mpv_wait_event(self.handle.as_ptr(), timeout) }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe { mpv_terminate_destroy(self.handle.as_ptr()) };
    }
}

pub(crate) struct RenderContext {
    handle: NonNull<MpvRenderContext>,
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
            mpv_render_context_create(&mut handle, mpv.handle.as_ptr(), params.as_mut_ptr())
        })?;
        NonNull::new(handle)
            .map(|handle| Self { handle })
            .ok_or_else(|| MpvError::invalid_argument("mpv_render_context_create returned null"))
    }

    pub(crate) fn update(&self) -> u64 {
        unsafe { mpv_render_context_update(self.handle.as_ptr()) }
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
