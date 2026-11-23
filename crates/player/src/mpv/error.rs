use std::ffi::{CStr, CString, c_int};

use thiserror::Error;

use super::ffi::mpv_error_string;

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

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: -1,
            message: message.into(),
        }
    }

    pub(crate) fn is_unknown_option(&self) -> bool {
        self.code == super::ffi::MPV_ERROR_OPTION_NOT_FOUND
            || self.code == super::ffi::MPV_ERROR_PROPERTY_NOT_FOUND
    }

    pub(crate) fn with_context(mut self, context: impl std::fmt::Display) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
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

pub(super) fn check(code: c_int) -> Result<(), MpvError> {
    if code < 0 {
        Err(MpvError::from_code(code))
    } else {
        Ok(())
    }
}

pub(super) fn c_string(value: &str) -> Result<CString, MpvError> {
    CString::new(value).map_err(|_| MpvError::invalid_argument("argument contains NUL"))
}
