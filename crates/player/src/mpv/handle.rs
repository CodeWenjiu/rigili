use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr::{NonNull, null_mut},
};

use crate::{VideoSize, playback::BufferedRange};

use super::{
    error::{MpvError, c_string, check},
    ffi::{
        MPV_FORMAT_DOUBLE, MPV_FORMAT_FLAG, MPV_FORMAT_INT64, MPV_FORMAT_NODE,
        MPV_FORMAT_NODE_ARRAY, MPV_FORMAT_NODE_MAP, MPV_FORMAT_STRING, MpvEvent, MpvHandle,
        MpvNode, MpvNodeValue, mpv_command, mpv_create, mpv_free, mpv_free_node_contents,
        mpv_get_property, mpv_initialize, mpv_set_option_string, mpv_set_property,
        mpv_terminate_destroy, mpv_wait_event,
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

    pub(crate) fn buffered_ranges(&self) -> Vec<BufferedRange> {
        let Ok(name) = c_string("demuxer-cache-state") else {
            return Vec::new();
        };
        let mut node = MpvNode {
            value: MpvNodeValue { int64: 0 },
            format: 0,
        };
        let code = unsafe {
            mpv_get_property(
                self.handle.as_ptr(),
                name.as_ptr(),
                MPV_FORMAT_NODE,
                &mut node as *mut MpvNode as *mut c_void,
            )
        };
        if code < 0 {
            return Vec::new();
        }

        let ranges = seekable_ranges(&node);
        unsafe { mpv_free_node_contents(&mut node) };
        ranges
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

fn seekable_ranges(node: &MpvNode) -> Vec<BufferedRange> {
    let Some(ranges) = map_get(node, b"seekable-ranges") else {
        return Vec::new();
    };
    node_values(ranges).iter().filter_map(range_entry).collect()
}

fn range_entry(node: &MpvNode) -> Option<BufferedRange> {
    let start = node_f64(map_get(node, b"start")?)?;
    let end = node_f64(map_get(node, b"end")?)?;
    (end > start).then_some(BufferedRange { start, end })
}

fn map_get<'a>(node: &'a MpvNode, name: &[u8]) -> Option<&'a MpvNode> {
    if node.format != MPV_FORMAT_NODE_MAP {
        return None;
    }
    let list = unsafe { node.value.list.as_ref() }?;
    if list.num <= 0 || list.keys.is_null() || list.values.is_null() {
        return None;
    }

    let count = list.num as usize;
    let keys = unsafe { std::slice::from_raw_parts(list.keys, count) };
    let values = unsafe { std::slice::from_raw_parts(list.values, count) };
    keys.iter().zip(values).find_map(|(key, value)| {
        let key = unsafe { key.as_ref() }?;
        (unsafe { CStr::from_ptr(key) }.to_bytes() == name).then_some(value)
    })
}

fn node_values(node: &MpvNode) -> &[MpvNode] {
    if node.format != MPV_FORMAT_NODE_ARRAY {
        return &[];
    }
    let Some(list) = (unsafe { node.value.list.as_ref() }) else {
        return &[];
    };
    if list.num <= 0 || list.values.is_null() {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(list.values, list.num as usize) }
}

fn node_f64(node: &MpvNode) -> Option<f64> {
    match node.format {
        MPV_FORMAT_DOUBLE => Some(unsafe { node.value.double }),
        MPV_FORMAT_INT64 => Some(unsafe { node.value.int64 } as f64),
        _ => None,
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe { mpv_terminate_destroy(self.handle.as_ptr()) };
    }
}
