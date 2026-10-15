//! Fallible native storage with allocation lifetime accounting.
use crate::resources::{self, Allocation, CancellationCheck, Lease};
use std::{io::Write, sync::Arc};

pub fn check() -> Result<(), String> {
    Ok(())
}

pub fn count(w: u32, h: u32, channels: usize) -> Result<usize, String> {
    if w == 0 || h == 0 {
        return Err("Pixel dimensions must be positive".into());
    }
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(channels))
        .filter(|n| *n <= isize::MAX as usize / std::mem::size_of::<f32>())
        .ok_or_else(|| "Pixel dimensions overflow addressable memory".into())
}

pub fn adopt<T: Copy>(values: Vec<T>) -> Result<Arc<Allocation<T>>, String> {
    let bytes = values
        .capacity()
        .checked_mul(std::mem::size_of::<T>())
        .ok_or("Pixel capacity overflows")?;
    let lease = resources::global().reserve_now(bytes as u64, &check)?;
    resources::global().adopt(values, lease)
}

pub fn collect<T: Copy>(values: impl IntoIterator<Item = T>) -> Result<Arc<Allocation<T>>, String> {
    let values = values.into_iter();
    let mut builder = Builder::with_capacity(values.size_hint().0)?;
    for value in values {
        builder.extend(&[value])?;
    }
    builder.finish()
}

pub struct Builder<T: Copy> {
    values: Vec<T>,
    lease: Arc<Lease>,
}
impl<T: Copy> Builder<T> {
    pub fn new() -> Result<Self, String> {
        Self::with_capacity(0)
    }
    pub fn with_capacity(capacity: usize) -> Result<Self, String> {
        let bytes = capacity.checked_mul(std::mem::size_of::<T>()).ok_or("Pixel capacity overflows")?;
        let lease = resources::global().reserve_now(bytes as u64, &check)?;
        let mut values = Vec::new();
        values.try_reserve_exact(capacity).map_err(|_| "Not enough memory to read pixels")?;
        lease.resize_now((values.capacity() * std::mem::size_of::<T>()) as u64, &check)?;
        Ok(Self {values, lease})
    }
    pub fn extend(&mut self, values: &[T]) -> Result<(), String> {
        let len = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or("Pixel length overflows")?;
        if len > self.values.capacity() {
            let capacity = len.max(self.values.capacity().saturating_mul(2)).max(1024);
            let bytes = capacity
                .checked_mul(std::mem::size_of::<T>())
                .ok_or("Pixel capacity overflows")?;
            self.lease.resize_now(bytes as u64, &check)?;
            self.values
                .try_reserve_exact(capacity - self.values.len())
                .map_err(|_| "Not enough memory to read pixels")?;
            self.lease.resize_now(
                (self.values.capacity() * std::mem::size_of::<T>()) as u64,
                &check,
            )?;
        }
        self.values.extend_from_slice(values);
        Ok(())
    }
    pub fn len(&self) -> usize {self.values.len()}
    pub fn finish(self) -> Result<Arc<Allocation<T>>, String> {
        resources::global().adopt(self.values, self.lease)
    }
}

/// Encode a seekable format while admitting every capacity increase before allocation.
/// Args: writes and seeks from an image encoder. Returns admitted encoded storage on finish.
pub struct Encoded {
    bytes: std::io::Cursor<Vec<u8>>,
    memory: Arc<Lease>,
}
impl Encoded {
    pub fn new() -> Result<Self, String> {
        Ok(Self {bytes: std::io::Cursor::new(Vec::new()), memory: resources::global().reserve_now(0, &check)?})
    }
    pub fn finish(self) -> Result<Arc<Allocation<u8>>, String> {
        resources::global().adopt(self.bytes.into_inner(), self.memory)
    }
}
impl Write for Encoded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = usize::try_from(self.bytes.position()).ok().and_then(|p| p.checked_add(bytes.len())).ok_or_else(|| std::io::Error::other("Encoded image size overflows"))?;
        let buffer = self.bytes.get_mut();
        if end > buffer.capacity() {
            let capacity = end.max(buffer.capacity().saturating_mul(2));
            self.memory.resize_now(capacity as u64, &check).map_err(std::io::Error::other)?;
            buffer.try_reserve_exact(capacity - buffer.len()).map_err(std::io::Error::other)?;
            self.memory.resize_now(buffer.capacity() as u64, &check).map_err(std::io::Error::other)?;
        }
        self.bytes.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {Ok(())}
}
impl std::io::Seek for Encoded {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {std::io::Seek::seek(&mut self.bytes, position)}
}
impl Write for Builder<u8> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend(bytes).map_err(std::io::Error::other)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn make_mut<'a, T: Copy, C: CancellationCheck + ?Sized>(
    buffer: &'a mut Arc<Allocation<T>>,
    cancel: &C,
) -> Result<&'a mut [T], String> {
    if Arc::get_mut(buffer).is_none() {
        *buffer = Arc::new(buffer.try_clone_now(cancel)?);
    }
    Ok(Arc::get_mut(buffer).unwrap().as_mut_slice())
}

pub mod bytes {
    use super::*;
    use serde::{
        Deserializer, Serializer,
        de::{SeqAccess, Visitor},
    };
    pub fn serialize<S: Serializer>(
        data: &Arc<Allocation<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(data.as_slice(), serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Allocation<u8>>, D::Error> {
        struct Bytes;
        impl<'de> Visitor<'de> for Bytes {
            type Value = Arc<Allocation<u8>>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut builder = Builder::new().map_err(serde::de::Error::custom)?;
                while let Some(value) = sequence.next_element::<u8>()? {
                    builder.extend(&[value]).map_err(serde::de::Error::custom)?;
                }
                builder.finish().map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_seq(Bytes)
    }
}

/// Allocate a paint coverage surface before entering the byte renderer.
/// Args: positive dimensions. Returns the surface and a lease retained for its complete lifetime.
pub fn pixmap(w: u32, h: u32) -> Result<(tiny_skia::Pixmap, Arc<Lease>), String> {
    let data = resources::global().allocate_now::<u8, _>(count(w, h, 4)?, &check)?;
    let (bytes, memory) = data.into_parts();
    let size = tiny_skia::IntSize::from_wh(w, h).ok_or("Invalid paint surface dimensions")?;
    let pixels = tiny_skia::Pixmap::from_vec(bytes, size).ok_or("Invalid paint surface storage")?;
    Ok((pixels, memory))
}
