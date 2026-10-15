//! Premultiplied paint surfaces retain their shared working-memory admission.
use super::storage;
use crate::resources::{self, Allocation, Lease};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
#[derive(Debug)]
pub struct Surface {
    pixels: tiny_skia::Pixmap,
    memory: Arc<Lease>,
}
impl Surface {
    /// Allocate a paint surface without waiting on the UI thread.
    /// Args: positive dimensions. Returns initialized pixels or an admission error.
    pub fn new(w: u32, h: u32) -> Result<Self, String> {
        let data =
            resources::global().allocate_now::<u8, _>(storage::count(w, h, 4)?, &storage::check)?;
        Self::from_allocation(w, h, data)
    }
    /// Transfer admitted premultiplied bytes into a paint surface.
    /// Args: dimensions and exclusive storage. Returns a surface retaining the same lease.
    pub fn from_allocation(w: u32, h: u32, data: Allocation<u8>) -> Result<Self, String> {
        if data.len() != storage::count(w, h, 4)? {
            return Err("Paint surface dimensions do not match storage".into());
        }
        let (bytes, memory) = data.into_parts();
        let size = tiny_skia::IntSize::from_wh(w, h).ok_or("Invalid paint surface dimensions")?;
        let pixels =
            tiny_skia::Pixmap::from_vec(bytes, size).ok_or("Invalid paint surface storage")?;
        Ok(Self { pixels, memory })
    }
    /// Create a display surface from straight stored bytes.
    /// Args: dimensions and straight RGBA8 display cache. Returns an admitted premultiplied copy.
    pub fn from_rgba(w: u32, h: u32, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != storage::count(w, h, 4)? {
            return Err("Invalid display pixel dimensions".into());
        }
        let mut out = Self::new(w, h)?;
        for (dst, src) in out.pixels_mut().iter_mut().zip(bytes.chunks_exact(4)) {
            *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
        }
        Ok(out)
    }
    /// Copy a borrowed renderer surface after admitting its complete buffer.
    /// Args: premultiplied renderer pixels. Returns an independently owned paint surface.
    pub fn from_pixmap(pixels: &tiny_skia::Pixmap) -> Result<Self, String> {
        Self::from_allocation(
            pixels.width(),
            pixels.height(),
            resources::global().copy_now(pixels.data(), &storage::check)?,
        )
    }
    /// Copy a surface with fallible admission rather than an implicit pixel Clone.
    /// Args: none. Returns an independent surface while preserving this one on failure.
    pub fn try_clone(&self) -> Result<Self, String> {
        Self::from_pixmap(&self.pixels)
    }
    /// Keep accidental surface clones fallible even through Pixmap's Deref API.
    /// Args: none. Returns a separately admitted copy.
    pub fn clone(&self) -> Result<Self, String> {
        self.try_clone()
    }
    pub fn admitted_bytes(&self) -> u64 {
        self.memory.bytes()
    }
}
impl Deref for Surface {
    type Target = tiny_skia::Pixmap;
    fn deref(&self) -> &Self::Target {
        &self.pixels
    }
}
impl DerefMut for Surface {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pixels
    }
}
