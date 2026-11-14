use std::{
    io,
    os::fd::{AsFd, OwnedFd},
};

/// Number of textures shared with the compositor at once.
pub const NATIVE_SLOTS: usize = 3;

/// One plane of an exported DMA-BUF.
#[derive(Debug)]
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}

/// A frame's GPU memory exported as DMA-BUFs.
#[derive(Debug)]
pub struct Dmabuf {
    pub generation: u64,
    pub fourcc: u32,
    pub modifier: u64,
    pub planes: Vec<DmabufPlane>,
}

impl Dmabuf {
    pub fn try_clone(&self) -> io::Result<Self> {
        let planes = self
            .planes
            .iter()
            .map(|plane| {
                Ok(DmabufPlane {
                    fd: plane.fd.as_fd().try_clone_to_owned()?,
                    offset: plane.offset,
                    stride: plane.stride,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            generation: self.generation,
            fourcc: self.fourcc,
            modifier: self.modifier,
            planes,
        })
    }
}

/// A rendered frame that a compositor can present without a CPU copy.
#[derive(Debug)]
pub struct NativeFrame {
    pub slot: u8,
    pub width: u32,
    pub height: u32,
    pub dmabuf: Dmabuf,
}
