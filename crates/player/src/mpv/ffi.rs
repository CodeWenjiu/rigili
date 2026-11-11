use std::ffi::{c_char, c_int, c_void};

pub(super) const MPV_FORMAT_STRING: c_int = 1;
pub(super) const MPV_FORMAT_FLAG: c_int = 3;
pub(super) const MPV_FORMAT_INT64: c_int = 4;
pub(super) const MPV_FORMAT_DOUBLE: c_int = 5;

pub(super) const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
pub(super) const MPV_RENDER_PARAM_NEXT_FRAME_INFO: c_int = 11;
pub(super) const MPV_RENDER_PARAM_SW_SIZE: c_int = 17;
pub(super) const MPV_RENDER_PARAM_SW_FORMAT: c_int = 18;
pub(super) const MPV_RENDER_PARAM_SW_STRIDE: c_int = 19;
pub(super) const MPV_RENDER_PARAM_SW_POINTER: c_int = 20;
pub(super) const MPV_RENDER_PARAM_INVALID: c_int = 0;

pub(super) const MPV_RENDER_FRAME_INFO_PRESENT: u64 = 1 << 0;
pub(super) const MPV_RENDER_FRAME_INFO_REDRAW: u64 = 1 << 1;

#[repr(C)]
pub(super) struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct MpvRenderContext {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct MpvRenderParam {
    pub(super) kind: c_int,
    pub(super) data: *mut c_void,
}

#[repr(C)]
pub(super) struct MpvRenderFrameInfo {
    pub(super) flags: u64,
    pub(super) target_time: i64,
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
    pub(super) fn mpv_create() -> *mut MpvHandle;
    pub(super) fn mpv_initialize(ctx: *mut MpvHandle) -> c_int;
    pub(super) fn mpv_terminate_destroy(ctx: *mut MpvHandle);
    pub(super) fn mpv_set_option_string(
        ctx: *mut MpvHandle,
        name: *const c_char,
        data: *const c_char,
    ) -> c_int;
    pub(super) fn mpv_command(ctx: *mut MpvHandle, args: *const *const c_char) -> c_int;
    pub(super) fn mpv_set_property(
        ctx: *mut MpvHandle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    pub(super) fn mpv_get_property(
        ctx: *mut MpvHandle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    pub(super) fn mpv_wait_event(ctx: *mut MpvHandle, timeout: f64) -> *mut MpvEvent;
    pub(super) fn mpv_error_string(error: c_int) -> *const c_char;
    pub(super) fn mpv_free(data: *mut c_void);
    pub(super) fn mpv_render_context_create(
        result: *mut *mut MpvRenderContext,
        mpv: *mut MpvHandle,
        params: *mut MpvRenderParam,
    ) -> c_int;
    pub(super) fn mpv_render_context_update(ctx: *mut MpvRenderContext) -> u64;
    pub(super) fn mpv_render_context_get_info(
        ctx: *mut MpvRenderContext,
        param: MpvRenderParam,
    ) -> c_int;
    pub(super) fn mpv_render_context_render(
        ctx: *mut MpvRenderContext,
        params: *mut MpvRenderParam,
    ) -> c_int;
    pub(super) fn mpv_render_context_report_swap(ctx: *mut MpvRenderContext);
    pub(super) fn mpv_render_context_free(ctx: *mut MpvRenderContext);
}
