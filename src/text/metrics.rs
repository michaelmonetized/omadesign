use super::*;
use crate::geom::{CharSpan, KernMode, Leading};

pub fn character_metrics(run: &TypeRun, index: usize) -> CharSpan {
    let mut style = run.character_style(index);
    style
        .tracking
        .get_or_insert(run.tracking / run.px.max(1.) * 1000.);
    style.kerning.get_or_insert(if run.kern {
        KernMode::Metrics
    } else {
        KernMode::None
    });
    style.leading.get_or_insert(if run.leading > 0.5 {
        Leading::Fixed(run.leading)
    } else {
        Leading::Auto(120.)
    });
    style.baseline_shift.get_or_insert(0.);
    style.hscale.get_or_insert(100.);
    style.vscale.get_or_insert(100.);
    style
}

/// Side-bearing profiles in em units, sampled at common horizontal bands.
/// Reused across font sizes and repeated pairs during interactive composition.
fn profile(run: &TypeRun, id: u16) -> Arc<Vec<Option<(f32, f32)>>> {
    type Cache = HashMap<(String, u16), Arc<Vec<Option<(f32, f32)>>>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let key = (run.font.clone(), id);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(value) = cache.lock().unwrap().get(&key).cloned() {
        return value;
    }
    let contours = resolve_path(run)
        .and_then(|p| font_bytes(&p))
        .and_then(|bytes| {
            let font = ab_glyph::FontRef::try_from_slice(&bytes).ok()?;
            let outline = font.outline(GlyphId(id))?;
            Some(flatten_outline(&map_curves(
                &outline.curves,
                0.,
                0.,
                1. / font.units_per_em().unwrap_or(1000.),
            )))
        })
        .unwrap_or_default();
    let bands: Vec<Option<(f32, f32)>> = (0..16)
        .map(|band| {
            let y = -0.95 + band as f32 * 0.075;
            let mut xs = Vec::new();
            for contour in &contours {
                for (a, b) in contour
                    .iter()
                    .zip(contour.iter().cycle().skip(1))
                    .take(contour.len())
                {
                    if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                        xs.push(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y));
                    }
                }
            }
            let min = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            min.is_finite().then_some((min, max))
        })
        .collect();
    let value = Arc::new(bands);
    let mut cache = cache.lock().unwrap();
    if cache.len() > 4096 {
        cache.clear();
    }
    cache.insert(key, value.clone());
    value
}
pub(super) fn optical_adjustment(
    run: &TypeRun,
    left: &LayoutGlyph,
    left_untracked_advance: f32,
    right: u16,
    right_scale: f32,
) -> f32 {
    type PairKey = (String, u16, u16, u32, u32, u32);
    static PAIRS: OnceLock<Mutex<HashMap<PairKey, f32>>> = OnceLock::new();
    let px = run.px.max(1.);
    let key = (
        run.font.clone(),
        left.id,
        right,
        (left_untracked_advance / px).to_bits(),
        left.hscale.to_bits(),
        right_scale.to_bits(),
    );
    let pairs = PAIRS.get_or_init(Default::default);
    if let Some(value) = pairs.lock().unwrap().get(&key).copied() {
        return value * px;
    }
    let a = profile(run, left.id);
    let b = profile(run, right);
    let mut gaps: Vec<_> = a
        .iter()
        .zip(b.iter())
        .filter_map(|(a, b)| {
            Some(left_untracked_advance / px + b.as_ref()?.0 * right_scale - a.as_ref()?.1 * left.hscale)
        })
        .collect();
    if gaps.is_empty() {
        return 0.;
    }
    gaps.sort_by(f32::total_cmp);
    let adjustment = (0.075 - gaps[gaps.len() / 4]).clamp(-0.15, 0.15);
    let mut pairs = pairs.lock().unwrap();
    if pairs.len() >= 8192 {
        pairs.clear();
    }
    pairs.insert(key, adjustment);
    adjustment * px
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::Studio,
        geom::{Bounds, Geom},
    };
    use eframe::egui::{self, Event, Key, Modifiers};
    fn run(content: &str) -> TypeRun {
        TypeRun {
            content: content.into(),
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            px: 40.,
            ..Default::default()
        }
    }
    fn width(run: &TypeRun) -> f32 {
        compose(run)[0].width
    }
    fn near(a: f32, b: f32) {
        assert!((a - b).abs() < 0.02, "{a} != {b}");
    }
    #[test]
    fn legacy_defaults_and_em_tracking_keep_physical_spacing() {
        let mut r = run("AAA");
        r.tracking = 2.;
        r.leading = 51.;
        near(character_metrics(&r, 1).tracking.unwrap(), 50.);
        near(compose(&r)[0].height, 51.);
        let legacy = width(&r);
        r.set_character_style(0, 3, |s| s.tracking = Some(50.));
        near(width(&r), legacy);
        r.px = 80.;
        r.tracking = 4.;
        r.leading = 102.;
        near(width(&r), legacy * 2.);
        r.set_character_style(0, 3, |s| s.tracking = Some(100.));
        near(width(&r), legacy * 2. + 12.);
    }
    #[test]
    fn line_uses_largest_leading_and_cache_includes_ranges() {
        let mut r = run("AAA\nBBB\nCCC");
        let before = compose(&r);
        near(before[1].baseline, 48.);
        r.set_character_style(5, 6, |s| s.leading = Some(Leading::Fixed(76.)));
        r.set_character_style(8, 11, |s| s.leading = Some(Leading::Auto(150.)));
        let after = compose(&r);
        near(after[1].baseline, 76.);
        near(after[2].baseline, 136.);
        near(after[2].height, 60.);
        assert!(!Arc::ptr_eq(&before, &after));
    }
    #[test]
    fn manual_pair_moves_caret_splits_ligature_and_survives_edits() {
        let mut r = run("AV fi");
        let plain = caret_pt(&r, 1);
        r.manual_kern.insert(1, 100.);
        near(caret_pt(&r, 1).x, plain.x + 4.);
        let fi = run("fi");
        let mut separated = fi.clone();
        separated.manual_kern.insert(1, 100.);
        assert_eq!(glyph_count(&separated), 2);
        assert_eq!(glyph_count(&fi), 1);
        let mut discretionary = run("ct ct");
        discretionary.features.push((*b"dlig", 1));
        assert_eq!(glyph_count(&discretionary), 3);
        discretionary.manual_kern.insert(1, 100.);
        assert_eq!(glyph_count(&discretionary), 4, "The manually kerned pair splits its discretionary ligature");
        assert_eq!(compose(&discretionary)[0].glyphs.last().unwrap().cluster, 3, "The adjacent discretionary ligature remains intact");
        r.replace_text(0, 0, "é", None);
        assert_eq!(r.manual_kern.get(&2), Some(&100.));
        r.replace_text(0, 1, "", None);
        assert_eq!(r.manual_kern.get(&1), Some(&100.));
        r.replace_text(1, 2, "", None);
        assert!(r.manual_kern.is_empty());
        let mut wrapped = run("A A");
        wrapped.wrap_width = Some(35.);
        wrapped.manual_kern.insert(2, 100.);
        assert_eq!(compose(&wrapped)[1].start, 2);
        near(compose(&wrapped)[1].glyphs[0].x, 0.);
    }
    #[test]
    fn metrics_none_and_optical_are_distinct_and_repeatable() {
        let r = run("AV Wa To");
        let metrics = width(&r);
        let mut none = r.clone();
        none.set_character_style(0, 8, |s| s.kerning = Some(KernMode::None));
        assert!((width(&none) - metrics).abs() > 0.1);
        let mut optical = none.clone();
        optical.set_character_style(0, 8, |s| s.kerning = Some(KernMode::Optical));
        assert!((width(&optical) - width(&none)).abs() > 0.1);
        let w = width(&optical);
        near(width(&optical), w);
        optical.px *= 2.;
        near(width(&optical), w * 2.);
    }
    #[test]
    fn metric_ranges_inside_ligatures_split_only_the_affected_cluster() {
        let original = run("fi fi");
        assert_eq!(glyph_count(&original), 3, "fixture must contain two fi ligatures");
        for metric in ["tracking", "baseline", "horizontal", "vertical", "kerning"] {
            let mut styled = original.clone();
            styled.set_character_style(1, 2, |s| match metric {
                "tracking" => s.tracking = Some(100.),
                "baseline" => s.baseline_shift = Some(7.),
                "horizontal" => s.hscale = Some(150.),
                "vertical" => s.vscale = Some(160.),
                _ => s.kerning = Some(KernMode::None),
            });
            let lines = compose(&styled);
            let glyphs = &lines[0].glyphs;
            assert_eq!(glyphs.iter().map(|g| g.cluster).collect::<Vec<_>>(), vec![0, 1, 2, 3], "{metric}");
            assert_eq!(glyphs[3].id, compose(&original)[0].glyphs[2].id, "neighboring ligature remains intact: {metric}");
            let isolated = run("i");
            let plain_i = compose(&isolated)[0].glyphs[0].clone();
            match metric {
                "tracking" => near(glyphs[1].advance, plain_i.advance + 4.),
                "baseline" => near(glyphs[1].y, -7.),
                "horizontal" => near(glyphs[1].advance, plain_i.advance * 1.5),
                "vertical" => near(glyphs[1].vscale, 1.6),
                _ => {},
            }
        }
        let mut identical = original.clone();
        identical.set_character_style(1, 2, |s| { s.tracking=Some(0.); s.hscale=Some(100.); });
        assert_eq!(glyph_count(&identical), 3, "equivalent defaults must preserve ligatures");
    }
    #[test]
    fn optical_kerning_keeps_tracking_additive() {
        let mut optical = run("AVWa");
        optical.set_character_style(0, 4, |s| s.kerning=Some(KernMode::Optical));
        let original=compose(&optical);
        for tracking in [-40., 40., 100.] {
            let mut tracked=optical.clone();
            tracked.set_character_style(0, 4, |s| s.tracking=Some(tracking));
            let changed=compose(&tracked);
            for (i,(a,b)) in original[0].glyphs.iter().zip(&changed[0].glyphs).enumerate() {
                near(b.x-a.x, i as f32 * tracking * optical.px / 1000.);
            }
            near(width(&tracked)-width(&optical), 4. * tracking * optical.px / 1000.);
        }
    }
    #[test]
    fn shifted_scaled_glyphs_caret_selection_and_hits_agree() {
        let mut r = run("ABC");
        let unscaled = compose(&r)[0].glyphs[1].advance;
        r.set_character_style(1, 2, |s| {
            s.hscale = Some(150.);
            s.vscale = Some(180.);
            s.baseline_shift = Some(18.);
        });
        let glyph = compose(&r)[0].glyphs[1].clone();
        near(glyph.advance, unscaled * 1.5);
        near(glyph.y, -18.);
        let p = caret_pt(&r, 1);
        near(p.y, -18.);
        near(caret_height(&r, 1), 72.);
        assert_eq!(hit_char(&r, p), 1);
        let selection = selection_rects(&r, 1, 2);
        near(selection[0].0.x, p.x);
        near(selection[0].0.y, -18. - 72. * 0.9);
        assert_eq!(selection_rects(&r, 0, 3).len(), 3);
        let src = Bounds::from_min_size(Pt::ZERO, Pt::new(100., 100.));
        let dst = Bounds::from_min_size(Pt::ZERO, Pt::new(200., 200.));
        r.set_character_style(0, 3, |s| s.leading = Some(Leading::Fixed(60.)));
        let mut geom = Geom::Text(r);
        geom.map_into(src, dst);
        let Geom::Text(t) = &geom else { panic!() };
        near(t.character_style(1).baseline_shift.unwrap(), 36.);
        assert_eq!(t.character_style(1).leading, Some(Leading::Fixed(120.)));
        near(t.character_style(1).hscale.unwrap(), 150.);
        geom.flip(false);
        let Geom::Text(t) = &geom else { panic!() };
        near(t.character_style(1).baseline_shift.unwrap(), -36.);
    }
    fn key(studio: &mut Studio, key: Key, modifiers: Modifiers) {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| studio.handle_shortcuts(ui.ctx()),
        );
        output.textures_delta.clear();
    }
    #[test]
    fn keyboard_increments_reset_defaults_history_and_roundtrip() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.active_layer = Some(1);
        studio.text_font = run("").font;
        studio.patch_character(|s| s.vscale = Some(125.));
        studio.place_text(Pt::ZERO);
        studio.type_insert("AV typography");
        assert_eq!(
            studio.selected_type().unwrap().character_style(0).vscale,
            Some(125.)
        );
        studio.startup_preferences.tracking_step = 30;
        studio.startup_preferences.leading_step = 3;
        studio.startup_preferences.baseline_step = 4;
        {
            let e = studio.type_edit.as_mut().unwrap();
            e.anchor = 0;
            e.caret = 2;
        }
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        key(&mut studio, Key::ArrowRight, alt);
        near(
            studio
                .selected_type()
                .unwrap()
                .character_style(0)
                .tracking
                .unwrap(),
            30.,
        );
        key(
            &mut studio,
            Key::ArrowRight,
            Modifiers {
                ctrl: true,
                command: true,
                ..alt
            },
        );
        near(
            studio
                .selected_type()
                .unwrap()
                .character_style(0)
                .tracking
                .unwrap(),
            180.,
        );
        key(&mut studio, Key::ArrowDown, alt);
        let expected = studio.selected_type().unwrap().line_height() + 3.;
        assert_eq!(
            studio.selected_type().unwrap().character_style(0).leading,
            Some(Leading::Fixed(expected))
        );
        key(&mut studio, Key::ArrowUp, Modifiers { shift: true, ..alt });
        near(
            studio
                .selected_type()
                .unwrap()
                .character_style(0)
                .baseline_shift
                .unwrap(),
            4.,
        );
        {
            let e = studio.type_edit.as_mut().unwrap();
            e.anchor = 1;
            e.caret = 1;
        }
        key(&mut studio, Key::ArrowLeft, alt);
        assert_eq!(
            studio.selected_type().unwrap().manual_kern.get(&1),
            Some(&-30.)
        );
        {
            let e = studio.type_edit.as_mut().unwrap();
            e.anchor = 0;
            e.caret = 2;
        }
        key(
            &mut studio,
            Key::Q,
            Modifiers {
                ctrl: true,
                command: true,
                ..alt
            },
        );
        let r = studio.selected_type().unwrap();
        assert!(r.manual_kern.is_empty());
        assert_eq!(r.character_style(0).tracking, Some(0.));
        studio.commit_type_edit();
        studio.undo();
        studio.redo();
        assert_eq!(studio.selected_type().unwrap().spans, r.spans);
        let encoded = crate::project::encode(&studio.doc).unwrap();
        assert!(encoded.contains("\"version\":12"));
        let restored = crate::project::decode(&encoded).unwrap();
        let t = restored.layers[1]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .find_map(|s| {
                if let Geom::Text(t) = &s.geom {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(t.spans, r.spans);
        let html = paragraph_html(t);
        assert!(html.contains("top:-4px"));
        assert!(html.contains("letter-spacing:calc(0em + var(--oma-paragraph-letter-spacing, 0em))"));
    }
    #[test]
    fn clipboard_keeps_spacing_styles_and_internal_pair_positions() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.active_layer = Some(1);
        studio.text_font = run("").font;
        studio.place_text(Pt::ZERO);
        studio.type_insert("AV xx");
        {
            let e = studio.type_edit.as_mut().unwrap();
            e.anchor = 0;
            e.caret = 2;
        }
        studio.patch_character(|s| {
            s.tracking = Some(80.);
            s.baseline_shift = Some(5.);
        });
        studio.patch_type(|r| {
            r.manual_kern.insert(1, -50.);
        });
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Copy],
                ..Default::default()
            },
            |ui| studio.handle_shortcuts(ui.ctx()),
        );
        output.textures_delta.clear();
        {
            let e = studio.type_edit.as_mut().unwrap();
            e.anchor = 5;
            e.caret = 5;
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Paste("AV".into())],
                ..Default::default()
            },
            |ui| studio.handle_shortcuts(ui.ctx()),
        );
        output.textures_delta.clear();
        let run = studio.selected_type().unwrap();
        assert_eq!(run.manual_kern.get(&6), Some(&-50.));
        assert_eq!(run.character_style(5).tracking, Some(80.));
        assert_eq!(run.character_style(6).baseline_shift, Some(5.));
        assert_eq!(run.character_style(3).tracking, None);
    }
}
