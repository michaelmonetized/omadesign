//! Exact source-derived appearance planes. Destination blends, masks, placement,
//! Fill opacity and final group interpolation are intentionally not cached.
use super::{Fx, Pixmap, effect_pixels};
use crate::compositor::cache_admission::Admission;
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

// Includes source snapshots and all effect planes. The measured independent
// shadow needs about 6.5 MiB; do not duplicate the full native effect budget.
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
type Planes = Vec<(Fx, Pixmap)>;

struct Entry {
    source: Vec<u8>,
    width: u32,
    height: u32,
    planes: Arc<Planes>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    bytes: usize,
    clock: u64,
    admission: Admission,
}

impl Cache {
    fn make_room(&mut self, key: u64, bytes: usize) -> bool {
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        if bytes > MAX_BYTES {
            return false;
        }
        let mut victims = Vec::new();
        if self.bytes + bytes > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let frequency = self.admission.frequency(key);
            let mut candidates: Vec<_> = self.entries.iter().collect();
            candidates.sort_unstable_by_key(|(_, entry)| entry.touched);
            let mut retained = self.bytes;
            for (&oldest, entry) in candidates {
                if frequency <= self.admission.frequency(oldest) {
                    return false;
                }
                victims.push(oldest);
                retained -= entry.bytes;
                if retained + bytes <= MAX_BYTES && self.entries.len() - victims.len() < MAX_ENTRIES
                {
                    break;
                }
            }
        }
        for oldest in victims {
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        true
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn reset_admission() {
    CACHE.with_borrow_mut(|cache| cache.admission = Admission::default());
}

fn key(source: &Pixmap, effects: &[Fx]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.width().hash(&mut hash);
    source.height().hash(&mut hash);
    source.data().hash(&mut hash);
    for fx in effects {
        std::mem::discriminant(fx).hash(&mut hash);
        let (blend, opacity, color, numbers, extra) = match *fx {
            Fx::Shadow {
                blend,
                opacity,
                color,
                dx,
                dy,
                blur,
                spread,
                knockout,
            } => (
                blend,
                opacity,
                color,
                [dx, dy, blur, spread],
                knockout as u8,
            ),
            Fx::InnerShadow {
                blend,
                opacity,
                color,
                dx,
                dy,
                blur,
                choke,
            } => (blend, opacity, color, [dx, dy, blur, choke], 0),
            Fx::OuterGlow {
                blend,
                opacity,
                color,
                blur,
                spread,
            } => (blend, opacity, color, [0., 0., blur, spread], 0),
            Fx::InnerGlow {
                blend,
                opacity,
                color,
                blur,
                choke,
                source,
            } => (blend, opacity, color, [0., 0., blur, choke], source as u8),
            Fx::ColorOverlay {
                blend,
                opacity,
                color,
            } => (blend, opacity, color, [0.; 4], 0),
            _ => unreachable!("only appearance effects have cached planes"),
        };
        (blend as u8).hash(&mut hash);
        opacity.to_bits().hash(&mut hash);
        color.hash(&mut hash);
        numbers.map(f32::to_bits).hash(&mut hash);
        extra.hash(&mut hash);
    }
    hash.finish()
}

pub(super) fn render(source: &Pixmap, stack: &super::FilterStack) -> Arc<Planes> {
    let effects: Vec<_> = stack
        .items
        .iter()
        .copied()
        .filter(|fx| stack.active() && fx.appearance().is_some())
        .collect();
    if effects.is_empty() {
        return Arc::new(Vec::new());
    }
    let bytes = source.data().len() * (effects.len() + 1)
        + effects.len() * std::mem::size_of::<(Fx, Pixmap)>()
        + std::mem::size_of::<Entry>()
        + std::mem::size_of::<Planes>();
    if bytes > MAX_BYTES {
        return build(source, &effects);
    }
    let key = key(source, &effects);
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        cache.admission.record(key);
        let entry = cache.entries.get_mut(&key)?;
        // Hashing only narrows lookup. Every source byte and Fx value still
        // participates in validation, including after a hash/key collision.
        if entry.width == source.width()
            && entry.height == source.height()
            && entry.planes.iter().map(|(fx, _)| fx).eq(effects.iter())
            && entry.source == source.data()
        {
            entry.touched = cache.clock;
            Some(entry.planes.clone())
        } else {
            None
        }
    });
    if let Some(hit) = hit {
        return hit;
    }
    // Rejected scans and large surfaces get no extra source snapshot.
    let keep = CACHE.with_borrow_mut(|cache| cache.make_room(key, bytes));
    let input = keep.then(|| source.data().to_vec());
    let planes = build(source, &effects);
    CACHE.with_borrow_mut(|cache| {
        let Some(input) = input else {
            return;
        };
        if !cache.make_room(key, bytes) {
            return;
        }
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                source: input,
                width: source.width(),
                height: source.height(),
                planes: planes.clone(),
                bytes,
                touched: cache.clock,
            },
        );
    });
    planes
}

fn build(source: &Pixmap, effects: &[Fx]) -> Arc<Planes> {
    Arc::new(
        effects
            .iter()
            .filter_map(|fx| Some((*fx, effect_pixels(source, fx)?)))
            .collect(),
    )
}

#[cfg(test)]
pub(super) fn clear() {
    CACHE.with_borrow_mut(|cache| *cache = Cache::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::{Blend, Rgba},
        filter::{FilterStack, GlowSource},
    };

    fn source(w: u32, h: u32, color: u8) -> Pixmap {
        let mut pixels = Pixmap::new(w, h).unwrap();
        pixels.fill(Rgba::new(color, 71, 169, 211).to_skia());
        pixels.data_mut()[0..4].fill(0);
        pixels
    }

    fn stack() -> FilterStack {
        FilterStack {
            enabled: true,
            legacy_composite: false,
            items: vec![
                Fx::Shadow {
                    blend: Blend::Overlay,
                    opacity: 0.73,
                    color: Rgba::new(10, 30, 70, 189),
                    dx: 2.5,
                    dy: -1.7,
                    blur: 1.4,
                    spread: 1.,
                    knockout: true,
                },
                Fx::OuterGlow {
                    blend: Blend::Screen,
                    opacity: 0.42,
                    color: Rgba::new(99, 171, 203, 138),
                    blur: 1.6,
                    spread: 1.,
                },
                Fx::InnerShadow {
                    blend: Blend::Multiply,
                    opacity: 0.91,
                    color: Rgba::new(21, 13, 19, 174),
                    dx: -2.,
                    dy: 1.,
                    blur: 1.3,
                    choke: 1.,
                },
                Fx::InnerGlow {
                    blend: Blend::Screen,
                    opacity: 0.82,
                    color: Rgba::new(212, 117, 83, 162),
                    blur: 1.7,
                    choke: 1.,
                    source: GlowSource::Center,
                },
                Fx::InnerGlow {
                    blend: Blend::Overlay,
                    opacity: 0.65,
                    color: Rgba::new(52, 107, 183, 192),
                    blur: 1.5,
                    choke: 0.,
                    source: GlowSource::Edge,
                },
                Fx::ColorOverlay {
                    blend: Blend::Color,
                    opacity: 0.39,
                    color: Rgba::new(121, 207, 31, 149),
                },
            ],
        }
    }

    fn assert_fresh(source: &Pixmap, stack: &FilterStack) -> Arc<Planes> {
        let planes = render(source, stack);
        assert_eq!(planes.len(), stack.items.len());
        for ((actual_fx, actual), fx) in planes.iter().zip(&stack.items) {
            assert_eq!(actual_fx, fx);
            assert_eq!(actual.data(), effect_pixels(source, fx).unwrap().data());
        }
        planes
    }

    #[test]
    fn every_appearance_plane_matches_fresh_pixels_and_reuses_immutable_results() {
        clear();
        let source = source(31, 25, 132);
        let stack = stack();
        let first = assert_fresh(&source, &stack);
        let second = render(&source, &stack);
        assert!(Arc::ptr_eq(&first, &second));
        let mut changed = source.clone();
        changed.data_mut()[4] ^= 1;
        let edited = assert_fresh(&changed, &stack);
        assert!(!Arc::ptr_eq(&first, &edited));
        let mut restyled = stack.clone();
        restyled.items.reverse();
        assert_fresh(&source, &restyled);
        clear();
    }

    #[test]
    fn hash_collision_still_checks_exact_source_dimensions_and_effects() {
        for change in 0..3 {
            clear();
            let before = source(24, 32, 119);
            let mut after = before.clone();
            let original = stack();
            let mut changed = original.clone();
            match change {
                0 => after.data_mut()[4] ^= 1,
                1 => {
                    after = Pixmap::new(32, 24).unwrap();
                    after.data_mut().copy_from_slice(before.data());
                }
                _ => changed.items.reverse(),
            }
            let first = render(&before, &original);
            let before_key = key(&before, &original.items);
            let after_key = key(&after, &changed.items);
            CACHE.with_borrow_mut(|cache| {
                let entry = cache.entries.remove(&before_key).unwrap();
                cache.entries.insert(after_key, entry);
            });
            let actual = assert_fresh(&after, &changed);
            assert!(!Arc::ptr_eq(&first, &actual));
        }
        clear();
    }

    #[test]
    fn retained_planes_are_bounded_and_sequential_over_budget_scenes_keep_hits() {
        clear();
        let stack = FilterStack {
            items: vec![Fx::ColorOverlay {
                blend: Blend::Overlay,
                opacity: 0.7,
                color: Rgba::new(71, 133, 211, 149),
            }],
            ..Default::default()
        };
        let mut previous = Vec::new();
        for index in 0..40 {
            previous.push(Arc::downgrade(&render(
                &source(256, 256, index * 5),
                &stack,
            )));
        }
        let mut hits = 0;
        for (index, old) in previous.iter().enumerate() {
            let old = old.upgrade();
            let current = render(&source(256, 256, index as u8 * 5), &stack);
            hits += usize::from(old.is_some_and(|old| Arc::ptr_eq(&old, &current)));
        }
        assert!(
            hits >= 20,
            "above-budget scan should retain a useful subset, got {hits} hits"
        );
        CACHE.with_borrow(|cache| {
            assert!(cache.bytes <= MAX_BYTES);
            assert!(cache.entries.len() <= MAX_ENTRIES);
            assert_eq!(
                cache.bytes,
                cache
                    .entries
                    .values()
                    .map(|entry| entry.bytes)
                    .sum::<usize>()
            );
        });
        let oversized = source(2048, 1024, 141);
        let result = render(&oversized, &stack);
        assert_eq!(
            result[0].1.data(),
            effect_pixels(&oversized, &stack.items[0]).unwrap().data()
        );
        let large_key = key(&oversized, &stack.items);
        CACHE.with_borrow(|cache| assert!(!cache.entries.contains_key(&large_key)));
        clear();
    }

    #[test]
    fn document_edits_reset_admission_without_dropping_appearance_planes() {
        let mut studio = crate::app::Studio::new();
        clear();
        let source = source(8, 8, 151);
        let stack = stack();
        let pixels = render(&source, &stack);
        for _ in 0..256 {
            render(&source, &stack);
        }
        let key = key(&source, &stack.items);
        CACHE.with_borrow(|cache| assert_eq!(cache.admission.frequency(key), 255));
        studio.commit(crate::document::Cmd::AddLayer {
            index: studio.doc.layers.len(),
            layer: crate::document::Layer::vector("new artwork"),
        });
        CACHE.with_borrow(|cache| assert_eq!(cache.admission.frequency(key), 0));
        assert!(Arc::ptr_eq(&pixels, &render(&source, &stack)));
        clear();
    }

    #[test]
    fn cached_planes_preserve_current_backdrop_masks_order_and_group_opacity() {
        use tiny_skia::{Mask, PixmapPaint, Transform};
        clear();
        let source = source(31, 25, 132);
        let stack = stack();
        // Independent, uncached recreation of the established appearance draws.
        fn draw(
            dst: &mut Pixmap,
            src: &Pixmap,
            blend_mode: tiny_skia::BlendMode,
            opacity: f32,
            transform: Transform,
            mask: Option<&Mask>,
        ) {
            dst.draw_pixmap(
                0,
                0,
                src.as_ref(),
                &PixmapPaint {
                    blend_mode,
                    opacity,
                    quality: tiny_skia::FilterQuality::Bilinear,
                },
                transform,
                mask,
            );
        }
        for variant in 0..4 {
            let opacity = if variant < 2 { 0.43 } else { 1. };
            let fill = if variant % 2 == 0 { 0.31 } else { 0.78 };
            let interior = variant % 2 == 1;
            let transform = Transform::from_row(0.91, 0.03, -0.07, 1.13, 9.25, 5.75);
            let mut mask = Mask::new(64, 48).unwrap();
            for (i, byte) in mask.data_mut().iter_mut().enumerate() {
                *byte = (i % 251) as u8;
            }
            let mask = (variant != 0).then_some(&mask);
            let mut expected = source_backdrop(variant);
            let mut actual = expected.clone();
            let original = expected.data().to_vec();
            let effects: Vec<_> = stack
                .items
                .iter()
                .map(|fx| (*fx, effect_pixels(&source, fx).unwrap()))
                .collect();
            for (fx, pixels) in effects.iter().filter(|(fx, _)| fx.outer()) {
                let (mode, alpha) = fx.appearance().unwrap();
                draw(
                    &mut expected,
                    pixels,
                    mode.to_skia(),
                    alpha,
                    transform,
                    mask,
                );
            }
            let mut content = source.clone();
            if interior {
                for byte in content.data_mut() {
                    *byte = (*byte as f32 * fill).round() as u8;
                }
                for (fx, pixels) in effects.iter().filter(|(fx, _)| !fx.outer()) {
                    let (mode, alpha) = fx.appearance().unwrap();
                    draw(
                        &mut content,
                        pixels,
                        mode.to_skia(),
                        alpha,
                        Transform::identity(),
                        None,
                    );
                }
                draw(
                    &mut expected,
                    &content,
                    Blend::Multiply.to_skia(),
                    1.,
                    transform,
                    mask,
                );
            } else {
                draw(
                    &mut expected,
                    &content,
                    Blend::Multiply.to_skia(),
                    fill,
                    transform,
                    mask,
                );
                for (fx, pixels) in effects.iter().filter(|(fx, _)| !fx.outer()) {
                    let (mode, alpha) = fx.appearance().unwrap();
                    draw(
                        &mut expected,
                        pixels,
                        mode.to_skia(),
                        alpha,
                        transform,
                        mask,
                    );
                }
            }
            if opacity < 1. {
                for (out, before) in expected.data_mut().iter_mut().zip(original) {
                    *out = (before as f32 + (*out as f32 - before as f32) * opacity).round() as u8;
                }
            }
            super::super::composite(
                &mut actual,
                source.clone(),
                &stack,
                transform,
                Blend::Multiply.to_skia(),
                opacity,
                fill,
                interior,
                mask,
            );
            assert_eq!(
                actual.data(),
                expected.data(),
                "appearance composition variant {variant}"
            );
        }
        clear();
    }

    fn source_backdrop(variant: u8) -> Pixmap {
        let mut dst = Pixmap::new(64, 48).unwrap();
        dst.fill(Rgba::new(41 + variant * 39, 179, 97, 187).to_skia());
        dst
    }
}
