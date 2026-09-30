//! Bounded, derived effect and isolated-object pixels on one render thread.
//! Keys own the exact cached path and paint inputs, never a document or its masks.
use super::*;
use std::{cell::RefCell, sync::Arc};

// Cloud.oma needs more than 64 MiB of native filtered objects. Retain that
// working set, including prepared content used during fades, within a fixed
// 192 MiB cap; cold scenes still compete through frequency admission.
const MAX_BYTES: usize = 192 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;

#[derive(PartialEq)]
struct RasterKey {
    screen: bool,
    prepared: bool,
    transform: [u32; 6],
    size: [u32; 2],
    stroke_reveal: Option<f32>,
    fill_reveal: Option<f32>,
}

impl RasterKey {
    fn new(shape: &Shape, mut pose: Pose, bounds: crate::geom::Bounds) -> Self {
        pose.opacity = Some(1.0);
        // Match draw_shape_inner's actual floating-point arithmetic, including
        // cancellation of animated translation against the local image origin.
        // Only reuse when the resulting matrix bits really are identical.
        let local = Transform::from_translate(-bounds.min.x, -bounds.min.y);
        let transform = local.pre_concat(pose.to_skia(shape.world_bbox().center()));
        Self::from_transform(
            pose,
            transform,
            [
                bounds.width().ceil().max(1.0) as u32,
                bounds.height().ceil().max(1.0) as u32,
            ],
            false,
        )
    }

    fn screen(shape: &Shape, mut pose: Pose, local: Transform, size: [u32; 2]) -> Self {
        pose.opacity = Some(1.0);
        let transform = if pose.is_identity() {
            local
        } else {
            local.pre_concat(pose.to_skia(shape.world_bbox().center()))
        };
        Self::from_transform(pose, transform, size, true)
    }

    fn from_transform(pose: Pose, transform: Transform, size: [u32; 2], screen: bool) -> Self {
        Self {
            screen,
            prepared: false,
            transform: [
                transform.sx.to_bits(),
                transform.kx.to_bits(),
                transform.ky.to_bits(),
                transform.sy.to_bits(),
                transform.tx.to_bits(),
                transform.ty.to_bits(),
            ],
            size,
            stroke_reveal: pose.stroke_reveal,
            fill_reveal: pose.fill_reveal,
        }
    }
}

struct Entry {
    path: Arc<tiny_skia::Path>,
    style: crate::document::Style,
    image: Option<crate::layout_images::ImageFill>,
    filters: crate::filter::FilterStack,
    raster: RasterKey,
    pixels: Arc<Pixmap>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<usize, Entry>,
    bytes: usize,
    clock: u64,
    admission: super::cache_admission::Admission,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn reset_admission() {
    CACHE.with_borrow_mut(|cache| cache.admission.reset());
}

pub(super) fn render(
    shape: &Shape,
    pose: Pose,
    bounds: crate::geom::Bounds,
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<Arc<Pixmap>> {
    // Opacity and placement on the destination do not invalidate native pixels.
    // Paint changes are already materialized in shape.style by the caller.
    let raster = RasterKey::new(shape, pose, bounds);
    let Some(path) = shape.get_cached_path(96) else {
        return draw().map(Arc::new);
    };
    render_keyed(shape, raster, path, draw)
}

/// Cache shape rasterization and pixel filters separately from appearance
/// compositing, so changing opacity does not recompute a large blur every frame.
pub(super) fn prepared(
    shape: &Shape,
    pose: Pose,
    bounds: crate::geom::Bounds,
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<Arc<Pixmap>> {
    let mut raster = RasterKey::new(shape, pose, bounds);
    raster.prepared = true;
    let Some(path) = shape.get_cached_path(96) else {
        return draw().map(Arc::new);
    };
    render_keyed(shape, raster, path, draw)
}

/// Reuse only the expensive paths that already require an isolated screen-space
/// image. The caller retains the original final opacity, blend and mask blit.
pub(super) enum ScreenPixels {
    Cached(Arc<Pixmap>),
    Uncached(Pixmap),
}

impl AsRef<Pixmap> for ScreenPixels {
    fn as_ref(&self) -> &Pixmap {
        match self {
            Self::Cached(pixels) => pixels,
            Self::Uncached(pixels) => pixels,
        }
    }
}

pub(super) fn render_screen(
    shape: &Shape,
    pose: Pose,
    local: Transform,
    size: [u32; 2],
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<ScreenPixels> {
    let Some(path) = shape
        .get_cached_path(96)
        .filter(|path| path.points().len() > 256)
    else {
        return draw().map(ScreenPixels::Uncached);
    };
    render_keyed(
        shape,
        RasterKey::screen(shape, pose, local, size),
        path,
        draw,
    )
    .map(ScreenPixels::Cached)
}

fn render_keyed(
    shape: &Shape,
    raster: RasterKey,
    path: Arc<tiny_skia::Path>,
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<Arc<Pixmap>> {
    let key = (Arc::as_ptr(&path) as usize) | usize::from(raster.prepared);
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        cache.admission.record(key as u64);
        let entry = cache.entries.get_mut(&key)?;
        if Arc::ptr_eq(&entry.path, &path)
            && entry.style == shape.style
            && entry.image == shape.layout.image
            && entry.filters == shape.filters
            && entry.raster == raster
        {
            entry.touched = cache.clock;
            Some(entry.pixels.clone())
        } else {
            None
        }
    });
    if hit.is_some() {
        return hit;
    }
    // Do not retain a RefCell borrow while drawing (future render paths can nest).
    let pixels = Arc::new(draw()?);
    let gradient_stops = match &shape.style.fill {
        Fill::Gradient(gradient) => gradient.stops.len(),
        _ => 0,
    } + shape
        .style
        .stroke
        .as_ref()
        .and_then(|stroke| stroke.gradient.as_ref())
        .map_or(0, |gradient| gradient.stops.len());
    let bytes = pixels.data().len()
        + path.points().len() * 2 * (std::mem::size_of::<Point>() + 1)
        + gradient_stops * std::mem::size_of::<crate::gradient::GradientStop>()
        + shape.filters.items.len() * std::mem::size_of::<crate::filter::Fx>()
        + std::mem::size_of::<Entry>()
        + shape
            .layout
            .image
            .as_ref()
            .map_or(0, |image| image.data.len());
    CACHE.with_borrow_mut(|cache| {
        if let Some(old) = cache.entries.remove(&key) {
            cache.bytes -= old.bytes;
        }
        if bytes > MAX_BYTES {
            return;
        }
        let mut victims = Vec::new();
        if cache.bytes + bytes > MAX_BYTES || cache.entries.len() >= MAX_ENTRIES {
            let frequency = cache.admission.frequency(key as u64);
            let mut candidates: Vec<_> = cache.entries.iter().collect();
            candidates.sort_unstable_by_key(|(_, entry)| entry.touched);
            let mut retained = cache.bytes;
            for (&oldest, entry) in candidates {
                // Preserve a useful working subset when a sequential scene is
                // larger than the budget, instead of missing on every object.
                if frequency <= cache.admission.frequency(oldest as u64) {
                    return;
                }
                victims.push(oldest);
                retained -= entry.bytes;
                if retained + bytes <= MAX_BYTES
                    && cache.entries.len() - victims.len() < MAX_ENTRIES
                {
                    break;
                }
            }
        }
        for oldest in victims {
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes -= old.bytes;
            }
        }
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                path,
                style: shape.style.clone(),
                image: shape.layout.image.clone(),
                filters: shape.filters.clone(),
                raster,
                pixels: pixels.clone(),
                bytes,
                touched: cache.clock,
            },
        );
    });
    Some(pixels)
}

#[cfg(test)]
pub(super) fn clear() {
    CACHE.with_borrow_mut(|cache| *cache = Cache::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complex_shape() -> Shape {
        let mut shape = Shape::new(
            Geom::Poly {
                contours: vec![
                    (0..300)
                        .map(|i| {
                            let angle = i as f32 * std::f32::consts::TAU / 300.;
                            Pt::new(35. + angle.cos() * 22., 30. + angle.sin() * 19.)
                        })
                        .collect(),
                ],
                winding: true,
            },
            crate::document::Style {
                fill: Fill::Solid(Rgba::new(130, 45, 210, 205)),
                stroke: Some(crate::document::Stroke {
                    width: 2.3,
                    color: Rgba::new(60, 190, 85, 235),
                    ..Default::default()
                }),
            },
        );
        shape.opacity = 0.395;
        shape.blend = crate::color::Blend::Overlay;
        shape
    }

    #[test]
    fn prepared_blur_survives_opacity_changes_and_coexists_with_finished_effects() {
        clear();
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(32., 24.),
                radius: 2.,
            },
            Default::default(),
        );
        let bounds = shape.world_bbox();
        let first = prepared(
            &shape,
            Pose {
                opacity: Some(0.2),
                ..Pose::identity()
            },
            bounds,
            || Pixmap::new(32, 24),
        )
        .unwrap();
        render(&shape, Pose::identity(), bounds, || Pixmap::new(32, 24)).unwrap();
        let second = prepared(
            &shape,
            Pose {
                opacity: Some(0.7),
                ..Pose::identity()
            },
            bounds,
            || panic!("fading must reuse prepared blur"),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let mut changed = shape.clone();
        changed.style.fill = Fill::Solid(Rgba::new(12, 34, 56, 255));
        let third = prepared(&changed, Pose::identity(), bounds, || Pixmap::new(32, 24)).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
        clear();
    }

    #[test]
    fn screen_cache_reuses_opacity_only_and_skips_simple_paths() {
        clear();
        let shape = complex_shape();
        let local = Transform::from_scale(0.75, 0.75);
        let first = render_screen(&shape, Pose::identity(), local, [64, 60], || {
            Pixmap::new(64, 60)
        })
        .unwrap();
        let second = render_screen(
            &shape,
            Pose {
                opacity: Some(0.8),
                ..Pose::identity()
            },
            local,
            [64, 60],
            || panic!("opacity belongs to final blit"),
        )
        .unwrap();
        assert!(std::ptr::eq(first.as_ref(), second.as_ref()));
        let simple = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(12.),
                radius: 0.,
            },
            crate::document::Style::default(),
        );
        for _ in 0..3 {
            let pixels = render_screen(&simple, Pose::identity(), local, [12, 12], || {
                Pixmap::new(12, 12)
            })
            .unwrap();
            assert!(matches!(pixels, ScreenPixels::Uncached(_)));
        }
        CACHE.with_borrow(|cache| assert_eq!(cache.entries.len(), 1));
        clear();
    }

    // Temporarily discard every derived entry for the reference draw, then put
    // the previous working set back to exercise invalidation on the next draw.
    fn fresh_draw(draw: impl FnOnce() -> Pixmap) -> Pixmap {
        let previous = CACHE.with_borrow_mut(std::mem::take);
        let pixels = draw();
        CACHE.with_borrow_mut(|cache| *cache = previous);
        pixels
    }

    #[test]
    fn isolated_screen_cache_matches_fresh_pixels_through_motion_and_edits() {
        clear();
        let mut shape = complex_shape();
        let mut image = Pixmap::new(3, 2).unwrap();
        image.fill(Rgba::new(180, 80, 20, 220).to_skia());
        let embedded = crate::layout_images::from_bytes(&image.encode_png().unwrap()).unwrap();
        for step in 0..20 {
            match step {
                4 => shape.style.fill = Fill::Solid(Rgba::new(10, 225, 90, 190)),
                5 => shape.style.stroke.as_mut().unwrap().width = 4.1,
                6 => shape.rotation = 0.17,
                7 => {
                    if let Geom::Poly { contours, .. } = &mut shape.geom {
                        contours[0][0].x += 5.3;
                    }
                }
                8 => shape.layout.image = Some(embedded.clone()),
                9 => shape.layout.image.as_mut().unwrap().focal = Pt::new(0.1, 0.85),
                10 => {
                    shape.style.stroke.as_mut().unwrap().alignment =
                        crate::document::StrokeAlignment::Outside
                }
                11 => shape.blend = crate::color::Blend::Multiply,
                12 => shape.fill_opacity = 0.63,
                13 => shape.layout.image = None,
                16 => {
                    shape.style.fill = Fill::Gradient(crate::gradient::Gradient::new(
                        crate::gradient::GradientKind::Linear,
                        Rgba::new(20, 220, 90, 180),
                        Rgba::new(245, 70, 40, 230),
                    ))
                }
                18 => {
                    if let Fill::Gradient(gradient) = &mut shape.style.fill {
                        gradient.kind = crate::gradient::GradientKind::Conic;
                        gradient.stops[0].offset = 0.17;
                    }
                }
                19 => {
                    if let Geom::Poly { winding, .. } = &mut shape.geom {
                        *winding = false;
                    }
                }
                _ => {}
            }
            let pose = Pose {
                opacity: Some(0.2 + (step % 6) as f32 * 0.1),
                dx: if step < 2 { 0. } else { 3.25 },
                dy: if step < 3 { 0. } else { -2.6 },
                scale: if step < 5 { 1. } else { 0.87 },
                rotation: if step < 6 { 0. } else { -0.13 },
                fill_reveal: (step >= 7).then_some(0.3 + (step % 3) as f32 * 0.2),
                stroke_reveal: (step >= 8).then_some(0.4 + (step % 4) as f32 * 0.1),
                fill_color: (step == 14).then_some(Rgba::rgb(240, 160, 30)),
                stroke_width: (step == 15).then_some(1.4),
                gradient_angle: (step >= 17).then_some(0.21),
                ..Pose::identity()
            };
            let transform = Transform::from_translate(if step < 2 { 3.3 } else { -7.15 }, 2.75)
                .pre_concat(Transform::from_scale(if step < 3 { 1. } else { 1.2 }, 0.9));
            let width = if step < 4 { 83 } else { 63 };
            let mut mask = tiny_skia::Mask::new(width, 73).unwrap();
            for (i, alpha) in mask.data_mut().iter_mut().enumerate() {
                *alpha = (75 + (i + step) % 180) as u8;
            }
            let draw = || {
                let mut pixels = Pixmap::new(width, 73).unwrap();
                for (i, rgba) in pixels.data_mut().chunks_exact_mut(4).enumerate() {
                    rgba.copy_from_slice(&[(i % 135) as u8, (i % 90) as u8, (i % 170) as u8, 190]);
                }
                draw_shape_masked(
                    &mut pixels,
                    &shape,
                    transform,
                    0.72,
                    tiny_skia::BlendMode::SourceOver,
                    pose,
                    Some(&mask),
                );
                pixels
            };
            let reference = fresh_draw(draw);
            let first = draw();
            let hit = draw();
            assert_eq!(
                first.data(),
                reference.data(),
                "invalidation at step {step}"
            );
            assert_eq!(hit.data(), reference.data(), "cached draw at step {step}");
        }
        CACHE.with_borrow(|cache| assert!(cache.bytes <= MAX_BYTES));
        clear();
    }

    #[test]
    fn new_tab_admits_its_working_set_after_a_saturated_large_effect() {
        assert_fresh_working_set(false);
    }

    #[test]
    fn committed_replacement_admits_its_working_set_after_a_saturated_large_effect() {
        assert_fresh_working_set(true);
    }

    fn assert_fresh_working_set(replace: bool) {
        let mut studio = crate::app::Studio::new();
        studio.show_welcome = false;
        studio.doc.layers = vec![Layer::vector("effects")];
        clear();
        let old_shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(4096., (MAX_BYTES / (4096 * 4) - 64) as f32),
                radius: 0.,
            },
            crate::document::Style::default(),
        );
        let bounds = old_shape.world_bbox();
        let first = render(&old_shape, Pose::identity(), bounds, || {
            Pixmap::new(4096, (MAX_BYTES / (4096 * 4) - 64) as u32)
        })
        .unwrap();
        let previous = Arc::downgrade(&first);
        for _ in 0..256 {
            let current = render(&old_shape, Pose::identity(), bounds, || {
                panic!("old effect must stay hot")
            })
            .unwrap();
            assert!(Arc::ptr_eq(&first, &current));
        }
        drop(first);
        CACHE.with_borrow(|cache| {
            let key = *cache.entries.keys().next().unwrap() as u64;
            assert_eq!(cache.admission.frequency(key), 255);
        });
        let shapes: Vec<_> = (0..50)
            .map(|index| {
                Shape::new(
                    Geom::Rect {
                        origin: Pt::new(index as f32, 0.),
                        size: Pt::new(512., 512.),
                        radius: 0.,
                    },
                    crate::document::Style::default(),
                )
            })
            .collect();
        if replace {
            studio.doc.layers[0]
                .kind
                .shapes_mut()
                .unwrap()
                .push(old_shape);
            studio.commit(crate::document::Cmd::SetVectorShapes {
                layer: 0,
                before: studio.doc.layers[0].kind.shapes().unwrap().to_vec(),
                after: shapes,
            });
            assert!(studio.dirty);
        } else {
            studio.new_tab();
            assert_eq!(studio.tab_count(), 2);
            studio.doc.layers = vec![Layer::vector("effects")];
            *studio.doc.layers[0].kind.shapes_mut().unwrap() = shapes;
        }
        assert!(
            previous.upgrade().is_some(),
            "activating or editing a document must preserve cached pixels"
        );
        let shapes = studio.doc.layers[0].kind.shapes().unwrap();
        for shape in shapes {
            render(shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(512, 512)
            })
            .unwrap();
        }
        assert!(
            previous.upgrade().is_none(),
            "the old large working set must yield immediately"
        );
        for shape in shapes {
            render(shape, Pose::identity(), shape.world_bbox(), || {
                panic!("new working set must be cached after its first traversal")
            })
            .unwrap();
        }
        CACHE.with_borrow(|cache| assert!(cache.bytes <= MAX_BYTES));
        clear();
    }

    #[test]
    fn playback_ticks_and_gesture_samples_preserve_admission_history() {
        let mut studio = crate::app::Studio::new();
        studio.persona = crate::tools::Persona::Motion;
        studio.playing = true;
        studio.play_clock = 1.;
        studio.playhead = 0.2;
        studio.doc.motion.duration = 10.;
        clear();
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(8., 8.),
                radius: 0.,
            },
            crate::document::Style::default(),
        );
        for _ in 0..20 {
            render(&shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(8, 8)
            })
            .unwrap();
        }
        let frequency = || {
            CACHE.with_borrow(|cache| {
                cache
                    .admission
                    .frequency(*cache.entries.keys().next().unwrap() as u64)
            })
        };
        let before = frequency();
        studio.mark_interaction();
        let ctx = eframe::egui::Context::default();
        ctx.begin_pass(eframe::egui::RawInput {
            time: Some(1.1),
            ..Default::default()
        });
        studio.tick_motion(&ctx);
        ctx.end_pass().textures_delta.clear();
        assert!(studio.playhead > 0.2);
        assert_eq!(
            frequency(),
            before,
            "playback and gesture ticks are not document revisions"
        );
        clear();
    }

    #[test]
    fn exact_local_raster_reuses_fractional_translation_and_opacity() {
        clear();
        let shape = Shape::new(
            Geom::Rect {
                // Keep the local origin nonzero: tiny-skia's identity fast
                // path can preserve -0 while later cancellation produces +0.
                origin: Pt::new(12., 16.),
                size: Pt::new(20., 16.),
                radius: 2.,
            },
            crate::document::Style::default(),
        );
        let mut original = None;
        let mut original_raster = None;
        for step in 0..16 {
            let pose = Pose {
                dx: step as f32 * 0.25,
                dy: -(step as f32) * 0.5,
                opacity: Some(0.1 + step as f32 * 0.05),
                ..Pose::identity()
            };
            let bounds = pose.map_bounds(shape.world_bbox()).inflate(8.);
            let raster = RasterKey::new(&shape, pose, bounds);
            if let Some(previous) = &original_raster {
                assert!(
                    previous == &raster,
                    "fixture must have bit-identical local matrices"
                );
            } else {
                original_raster = Some(raster);
            }
            let current = render(&shape, pose, bounds, || Pixmap::new(36, 32)).unwrap();
            if let Some(previous) = &original {
                assert!(
                    Arc::ptr_eq(previous, &current),
                    "identical native matrix must reuse pixels"
                );
            } else {
                original = Some(current);
            }
        }
        clear();
    }

    #[test]
    fn sequential_effect_scene_retains_its_native_working_set() {
        clear();
        let shapes: Vec<_> = (0..40)
            .map(|index| {
                Shape::new(
                    Geom::Rect {
                        origin: Pt::new(index as f32, 0.),
                        size: Pt::new(20., 16.),
                        radius: 2.,
                    },
                    crate::document::Style::default(),
                )
            })
            .collect();
        let mut previous = Vec::new();
        for shape in &shapes {
            let pixels = render(shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(512, 512)
            })
            .unwrap();
            previous.push(Arc::downgrade(&pixels));
        }
        for (shape, previous) in shapes.iter().zip(previous) {
            let previous = previous
                .upgrade()
                .expect("a 40 MiB scene must remain resident");
            let pixels = render(shape, Pose::identity(), shape.world_bbox(), || {
                panic!("sequential cache thrash")
            })
            .unwrap();
            assert!(Arc::ptr_eq(&previous, &pixels));
        }
        CACHE.with_borrow(|cache| assert!(cache.bytes <= MAX_BYTES));
        clear();
    }

    #[test]
    fn sequential_scene_above_budget_does_not_evict_every_useful_entry() {
        let mut studio = crate::app::Studio::new();
        studio.doc.layers = vec![Layer::vector("edited scene")];
        clear();
        let shapes: Vec<_> = (0..(MAX_BYTES / (512 * 512 * 4) + 16))
            .map(|index| {
                Shape::new(
                    Geom::Rect {
                        origin: Pt::new(index as f32, 0.),
                        size: Pt::new(20., 16.),
                        radius: 2.,
                    },
                    crate::document::Style::default(),
                )
            })
            .collect();
        *studio.doc.layers[0].kind.shapes_mut().unwrap() = shapes;
        for shape in studio.doc.layers[0].kind.shapes().unwrap() {
            render(shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(512, 512)
            })
            .unwrap();
        }
        for revision in 0..4 {
            if revision > 0 {
                let shape = &studio.doc.layers[0].kind.shapes().unwrap()[0];
                studio.commit(crate::document::Cmd::SetOpacity {
                    layer: 0,
                    id: shape.id,
                    before: shape.opacity,
                    after: 0.9 - revision as f32 * 0.1,
                });
            }
            let mut misses = 0;
            for shape in studio.doc.layers[0].kind.shapes().unwrap() {
                render(shape, Pose::identity(), shape.world_bbox(), || {
                    misses += 1;
                    Pixmap::new(512, 512)
                })
                .unwrap();
            }
            assert!(
                misses < 40,
                "revision {revision}: over-budget scene must keep a useful working subset, got {misses} misses"
            );
        }
        CACHE.with_borrow(|cache| {
            assert!(cache.bytes <= MAX_BYTES);
            assert!(cache.entries.len() <= MAX_ENTRIES);
        });
        clear();
    }

    #[test]
    fn cache_evicts_old_paths_instead_of_retaining_closed_documents() {
        clear();
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            crate::document::Style::default(),
        );
        let bounds = shape.geom.bbox();
        let first = render(&shape, Pose::identity(), bounds, || Pixmap::new(8, 8)).unwrap();
        let recent = std::sync::Arc::downgrade(&first);
        drop(first);
        for _ in 0..MAX_ENTRIES * 8 {
            shape.geom.translate(Pt::new(1.0, 0.0));
            render(&shape, Pose::identity(), shape.geom.bbox(), || {
                Pixmap::new(8, 8)
            })
            .unwrap();
        }
        assert!(recent.upgrade().is_none());
        CACHE.with_borrow(|cache| {
            assert_eq!(cache.entries.len(), MAX_ENTRIES);
            assert!(cache.bytes <= MAX_BYTES);
            assert_eq!(
                cache.bytes,
                cache.entries.values().map(|e| e.bytes).sum::<usize>()
            );
        });
        clear();
        CACHE.with_borrow(|cache| assert_eq!(cache.bytes, 0));
    }
}
