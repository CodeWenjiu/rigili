use std::ffi::CStr;

use super::ffi::{MpvEvent, MpvEventEndFile, MpvEventLogMessage, MpvEventStartFile};

pub(crate) struct LogMessage {
    pub(crate) prefix: String,
    pub(crate) level: String,
    pub(crate) text: String,
}

pub(crate) fn log_message(event: &MpvEvent) -> Option<LogMessage> {
    let message = unsafe { event.data.cast::<MpvEventLogMessage>().as_ref()? };
    Some(LogMessage {
        prefix: string(message.prefix),
        level: string(message.level),
        text: string(message.text).trim_end().to_owned(),
    })
}

pub(crate) fn end_file_reason(event: &MpvEvent) -> &'static str {
    let Some(end) = (unsafe { event.data.cast::<MpvEventEndFile>().as_ref() }) else {
        return "unknown";
    };
    match end.reason {
        0 => "eof",
        2 => "stop",
        3 => "quit",
        4 => "error",
        5 => "redirect",
        _ => "unknown",
    }
}

pub(crate) fn end_file_error(event: &MpvEvent) -> i32 {
    unsafe { event.data.cast::<MpvEventEndFile>().as_ref() }
        .map(|end| end.error)
        .unwrap_or_default()
}

pub(crate) fn end_file_entry(event: &MpvEvent) -> Option<i64> {
    unsafe { event.data.cast::<MpvEventEndFile>().as_ref() }.map(|end| end.playlist_entry_id)
}

pub(crate) fn start_file_entry(event: &MpvEvent) -> Option<i64> {
    unsafe { event.data.cast::<MpvEventStartFile>().as_ref() }.map(|start| start.playlist_entry_id)
}

fn string(pointer: *const std::ffi::c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}
