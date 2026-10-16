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
    values: Allocation<T>,
}
impl<T: Copy> Builder<T> {
    pub fn new() -> Result<Self, String> {
        Self::with_capacity(0)
    }
    pub fn with_capacity(capacity: usize) -> Result<Self, String> {
        let mut values = resources::global().copy_now(&[], &check)?;
        values.reserve_now(capacity, &check)?;
        Ok(Self {values})
    }
    pub fn extend(&mut self, values: &[T]) -> Result<(), String> {
        let len = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or("Pixel length overflows")?;
        let bytes = len.checked_mul(std::mem::size_of::<T>()).ok_or("Pixel length overflows")?;
        if bytes as u64 > self.values.bytes() {
            let capacity = usize::try_from(self.values.bytes() / std::mem::size_of::<T>() as u64)
                .map_err(|_| "Pixel capacity overflows")?;
            let growth = capacity.checked_add(capacity / 4).ok_or("Pixel capacity overflows")?;
            self.values.reserve_now(len.max(growth).max(1024), &check)?;
        }
        self.values.extend_from_slice_now(values, &check)
    }
    pub fn len(&self) -> usize {self.values.len()}
    pub fn finish(self) -> Result<Arc<Allocation<T>>, String> {
        Ok(Arc::new(self.values))
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn initialized_builder_retains_spare_and_last_alias_without_default_samples() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Sample(u32);
        let pool = resources::global();
        let before = (pool.usage().0, pool.pending_bytes());
        let mut builder = Builder::with_capacity(32).unwrap();
        builder.extend(&[Sample(3); 8]).unwrap();
        assert_eq!(builder.len(), 8);
        assert_eq!((pool.usage().0 - before.0, pool.pending_bytes() - before.1), (128, 96));
        let output = builder.finish().unwrap();
        let memory = Arc::downgrade(output.lease());
        let alias = output.clone();
        assert_eq!(output.as_slice(), &[Sample(3); 8]);
        drop(output);
        assert!(memory.upgrade().is_some());
        assert_eq!((pool.usage().0 - before.0, pool.pending_bytes() - before.1), (128, 96));
        drop(alias);
        assert!(memory.upgrade().is_none());
        assert_eq!((pool.usage().0, pool.pending_bytes()), before);
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

pub fn empty_bytes() -> Arc<Allocation<u8>> {
    adopt(Vec::new()).expect("Empty storage needs no memory")
}
pub fn empty(bytes: &Arc<Allocation<u8>>) -> bool {
    bytes.is_empty()
}
pub mod optional_floats {
    use super::*;
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{SeqAccess, Visitor},
    };
    pub fn serialize<S: Serializer>(
        value: &Option<Arc<Allocation<f32>>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(|v| v.as_slice()).serialize(s)
    }
    struct Floats(Arc<Allocation<f32>>);
    impl<'de> Deserialize<'de> for Floats {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Values;
            impl<'de> Visitor<'de> for Values {
                type Value = Floats;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("finite color samples")
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Floats, A::Error> {
                    let mut out = Builder::new().map_err(serde::de::Error::custom)?;
                    while let Some(value) = seq.next_element::<f32>()? {
                        if !value.is_finite() {
                            return Err(serde::de::Error::custom("Nonfinite ink sample"));
                        }
                        out.extend(&[value]).map_err(serde::de::Error::custom)?;
                    }
                    Ok(Floats(out.finish().map_err(serde::de::Error::custom)?))
                }
            }
            d.deserialize_seq(Values)
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Arc<Allocation<f32>>>, D::Error> {
        Ok(Option::<Floats>::deserialize(d)?.map(|v| v.0))
    }
}
