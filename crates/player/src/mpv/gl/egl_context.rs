use std::{
    ffi::{CStr, c_char, c_int, c_void},
    fs::{File, OpenOptions},
    os::fd::{FromRawFd, OwnedFd},
    ptr::{null, null_mut},
};

use khronos_egl as egl;

use crate::native::{Dmabuf, DmabufPlane};

use super::super::error::MpvError;

const EGL_PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31dd;

type ExportQuery =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_int, *mut c_int, *mut u64) -> u32;
type ExportImage =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_int, *mut c_int, *mut c_int) -> u32;

pub(super) struct EglContext {
    pub(super) egl: Box<egl::DynamicInstance<egl::EGL1_5>>,
    pub(super) display: egl::Display,
    pub(super) context: egl::Context,
    pub(super) gl: glow::Context,
}

impl EglContext {
    pub(super) fn new() -> Result<Self, MpvError> {
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
                return Err(MpvError::invalid_argument(format!(
                    "无法创建 OpenGL 上下文：{error}"
                )));
            }
        };
        if let Err(error) = egl.make_current(display, None, None, Some(context)) {
            let _ = egl.destroy_context(display, context);
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

// Every player opens the same surfaceless display, so it is never terminated:
// terminating it takes the display away from the players still rendering.
impl Drop for EglContext {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

pub(super) fn export_texture(
    egl: &EglContext,
    texture: glow::Texture,
    generation: u64,
) -> Result<(egl::Image, Dmabuf), MpvError> {
    let buffer = unsafe { egl::ClientBuffer::from_ptr(texture.0.get() as usize as *mut c_void) };
    let image = egl
        .egl
        .create_image(
            egl.display,
            egl.context,
            egl::GL_TEXTURE_2D as egl::Enum,
            buffer,
            &[egl::IMAGE_PRESERVED as egl::Attrib, 1, egl::ATTRIB_NONE],
        )
        .map_err(|error| MpvError::invalid_argument(format!("无法创建 EGLImage：{error}")))?;

    let query: ExportQuery = match egl.egl.get_proc_address("eglExportDMABUFImageQueryMESA") {
        // SAFETY: the MESA export extension entry point uses this signature.
        Some(proc) => unsafe { std::mem::transmute::<extern "system" fn(), ExportQuery>(proc) },
        None => {
            let _ = egl.egl.destroy_image(egl.display, image);
            return Err(MpvError::invalid_argument(
                "eglExportDMABUFImageQueryMESA is not supported",
            ));
        }
    };
    let export: ExportImage = match egl.egl.get_proc_address("eglExportDMABUFImageMESA") {
        // SAFETY: the MESA export extension entry point uses this signature.
        Some(proc) => unsafe { std::mem::transmute::<extern "system" fn(), ExportImage>(proc) },
        None => {
            let _ = egl.egl.destroy_image(egl.display, image);
            return Err(MpvError::invalid_argument(
                "eglExportDMABUFImageMESA is not supported",
            ));
        }
    };

    let mut fourcc = 0;
    let mut planes = 0;
    let mut modifier = 0u64;
    if unsafe {
        query(
            egl.display.as_ptr(),
            image.as_ptr(),
            &mut fourcc,
            &mut planes,
            &mut modifier,
        )
    } == 0
    {
        let _ = egl.egl.destroy_image(egl.display, image);
        return Err(MpvError::invalid_argument(
            "eglExportDMABUFImageQueryMESA failed",
        ));
    }
    if !(1..=4).contains(&planes) {
        let _ = egl.egl.destroy_image(egl.display, image);
        return Err(MpvError::invalid_argument(format!(
            "unsupported dmabuf plane count: {planes}"
        )));
    }

    let mut fds = [-1; 4];
    let mut strides = [0; 4];
    let mut offsets = [0; 4];
    if unsafe {
        export(
            egl.display.as_ptr(),
            image.as_ptr(),
            fds.as_mut_ptr(),
            strides.as_mut_ptr(),
            offsets.as_mut_ptr(),
        )
    } == 0
    {
        let _ = egl.egl.destroy_image(egl.display, image);
        close_fds(&fds);
        return Err(MpvError::invalid_argument(
            "eglExportDMABUFImageMESA failed",
        ));
    }

    let mut result = Vec::with_capacity(planes as usize);
    for index in 0..planes as usize {
        if fds[index] < 0 {
            let _ = egl.egl.destroy_image(egl.display, image);
            close_fds(&fds);
            return Err(MpvError::invalid_argument("dmabuf export returned no fd"));
        }
        // SAFETY: the exported fds are owned by this process.
        result.push(DmabufPlane {
            fd: unsafe { OwnedFd::from_raw_fd(fds[index]) },
            offset: offsets[index] as u32,
            stride: strides[index] as u32,
        });
    }

    Ok((
        image,
        Dmabuf {
            generation,
            fourcc: fourcc as u32,
            modifier,
            planes: result,
        },
    ))
}

fn close_fds(fds: &[c_int]) {
    for &fd in fds {
        if fd >= 0 {
            // SAFETY: the fd was returned by the EGL export and is not used elsewhere.
            drop(unsafe { OwnedFd::from_raw_fd(fd) });
        }
    }
}

// VAAPI GL interop needs a render node even though the EGL context is
// surfaceless, so hand mpv the first usable node.
pub(super) fn open_render_node() -> Option<File> {
    (128..=135).find_map(|index| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/dri/renderD{index}"))
            .ok()
    })
}

pub(super) extern "C" fn load_proc_address(
    context: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use glow::HasContext as _;

    use super::*;

    fn texture(egl: &EglContext) -> glow::Texture {
        unsafe {
            let texture = egl.gl.create_texture().expect("texture");
            egl.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            egl.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                4,
                4,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            egl.gl.bind_texture(glow::TEXTURE_2D, None);
            egl.gl.finish();
            texture
        }
    }

    #[test]
    #[ignore = "requires EGL and a GPU"]
    fn dropping_one_player_leaves_the_shared_display_usable() {
        let first = EglContext::new().expect("first context");
        let (ready, ready_rx) = mpsc::channel();
        let (go, go_rx) = mpsc::channel();
        let other = std::thread::spawn(move || {
            let context = EglContext::new().expect("second context");
            let texture = texture(&context);
            ready.send(()).expect("ready");
            go_rx.recv().expect("go");
            let result = export_texture(&context, texture, 1);
            let outcome = result.as_ref().map(drop).map_err(ToString::to_string);
            if let Ok((image, _)) = result {
                let _ = context.egl.destroy_image(context.display, image);
            }
            unsafe {
                context.gl.delete_texture(texture);
            }
            outcome
        });

        ready_rx.recv().expect("second context is up");
        drop(first);
        go.send(()).expect("go");

        let exported = other.join().expect("second thread");
        assert!(exported.is_ok(), "{:?}", exported.err());
    }
}
