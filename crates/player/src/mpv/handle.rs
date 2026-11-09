use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr::{NonNull, null_mut},
};

use crate::VideoSize;

use super::{
    error::{MpvError, c_string, check},
    ffi::{
        MPV_FORMAT_DOUBLE, MPV_FORMAT_FLAG, MPV_FORMAT_INT64, MPV_FORMAT_STRING, MpvEvent,
        MpvHandle, mpv_command, mpv_create, mpv_free, mpv_get_property, mpv_initialize,
        mpv_set_option_string, mpv_set_property, mpv_terminate_destroy, mpv_wait_event,
    },
};

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

    pub(super) fn raw_handle(&self) -> *mut MpvHandle {
        self.handle.as_ptr()
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

    pub(crate) fn video_size(&self) -> Option<VideoSize> {
        let width = self.get_property_i64("video-params/w")?;
        let height = self.get_property_i64("video-params/h")?;
        (width > 0 && height > 0).then_some(VideoSize::new(width as u32, height as u32))
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
