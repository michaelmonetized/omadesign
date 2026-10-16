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
    style.px.get_or_insert(run.px);
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
            let em_scale = 1. / font.units_per_em().unwrap_or(1000.);
            Some(flatten_outline(
                &map_curves(&outline.curves, 0., 0., em_scale),
                em_scale / 64.,
            ))
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
    #[test]
    fn svg_type_native_affine_styled_type_survives_edit_transform_and_project_reopen() {
        use crate::{color::Rgba,document::{Document,Shape,Style},geom::Pt};
        let mut r=run("Type type");
        r.set_character_style(5,9,|s| {s.px=Some(70.);s.font=Some(r#"/usr/share/fonts/liberation/LiberationSans-Bold.ttf"#.into());s.fill=Some(crate::document::Fill::Solid(Rgba::rgb(220,30,50)));});
        assert!(width(&r)>width(&run("Type type")));
        r.transform=Some([-0.9,0.25,0.35,1.2,220.,20.]);
        let local=Pt::new(13.,-24.);
        assert!((r.unmap_point(r.map_point(local))-local).length()<0.001);
        r.contours=super::shape(&r);
        let mut geom=Geom::Text(r);
        let before=geom.contours(32);
        geom.translate(Pt::new(17.,13.));
        crate::text::fill_contours(&mut geom);
        assert!((geom.contours(32)[0][0]-before[0][0]-Pt::new(17.,13.)).length()<0.001);
        let Geom::Text(r)=&mut geom else {unreachable!()};
        r.replace_text(0,0,"é",None);
        assert_eq!(r.character_style(6).px,Some(70.));
        let mut doc=Document::new("Affine type",300.,180.,96.);
        doc.layers.clear();let mut layer=crate::document::Layer::vector("Type");layer.kind.shapes_mut().unwrap().push(Shape::new(geom,Style::default()));doc.layers.push(layer);
        let encoded=crate::project::encode(&doc).unwrap();
        assert!(encoded.contains("\"version\":14"));
        let decoded=crate::project::decode(&encoded).unwrap();
        assert_eq!(serde_json::to_value(&doc).unwrap(),serde_json::to_value(&decoded).unwrap());
        let image=crate::compositor::render_export(&decoded,1).unwrap();
        assert!(image.pixels().iter().any(|p|u16::from(p.red())>u16::from(p.blue())+20 && u16::from(p.red())>u16::from(p.green())+20));
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
    fn nonuniform_resize_preserves_spacing_after_recompose_and_save_reopen() {
        use crate::document::{Document, Shape, Style};
        let mut original = run("AVATAR AV");
        original.origin = Pt::new(37., 83.);
        original.tracking = 2.;
        original.set_character_style(0, 3, |s| s.tracking = Some(100.));
        original.set_character_style(4, 8, |s| s.tracking = Some(-40.));
        original.set_character_style(2, 5, |s| s.hscale = Some(125.));
        original.manual_kern.insert(1, -80.);
        original.manual_kern.insert(7, 120.);
        original.contours = shape(&original);
        for (sx, sy) in [(2., 1.), (0.6, 1.5), (1.25, 1.25)] {
            let mut geometry = Geom::Text(original.clone());
            let source = geometry.bbox();
            let target = Bounds::from_min_size(Pt::new(91., 117.), Pt::new(source.width() * sx, source.height() * sy));
            geometry.map_into(source, target);
            let Geom::Text(transformed) = &geometry else { unreachable!() };
            let compare = |actual: &[Vec<Pt>], label: &str| {
                assert_eq!(transformed.contours.len(), actual.len());
                let mut max_error = 0_f32;
                for (mapped, fresh) in transformed.contours.iter().zip(actual) {
                    assert_eq!(mapped.len(), fresh.len());
                    for (a, b) in mapped.iter().zip(fresh) {
                        max_error = max_error.max((*a - *b).length());
                    }
                }
                assert!(max_error < 0.001, "{label}: sx={sx} sy={sy}, contour drift={max_error}px");
            };
            compare(&shape(transformed), "recompose");
            let mut doc = Document::new("Spacing transform", 800., 600., 96.);
            let art = Shape::new(geometry.clone(), Style::default());
            let id = art.id;
            doc.layers[1].kind.shapes_mut().unwrap().push(art);
            let restored = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
            let Geom::Text(reopened) = &restored.find_shape(1, id).unwrap().geom else { unreachable!() };
            compare(&reopened.contours, "save/reopen");
            assert_eq!(reopened.spans, transformed.spans);
            assert_eq!(reopened.manual_kern, transformed.manual_kern);
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

#[cfg(test)]
mod svg_type_native_acceptance {
    use crate::{color::Rgba,document::{Document,Fill,Layer,Shape,Style},geom::{Geom,Pt,TypeRun}};
    fn sample(content:&str)->TypeRun{TypeRun{content:content.into(),px:32.,origin:Pt::new(25.,65.),font:concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(),..Default::default()}}
    fn document(mut run:TypeRun)->Document{run.contours=crate::text::shape(&run);let mut doc=Document::new("Native type",320.,180.,96.);doc.width=320.;doc.height=180.;doc.transparent=true;doc.layers.clear();let mut layer=Layer::vector("Type");layer.kind.shapes_mut().unwrap().push(Shape::new(Geom::Text(run),Style::default()));doc.layers.push(layer);doc}
    fn compare(source:&str,doc:&Document)->f32{let mut options=usvg::Options::default();options.fontdb_mut().load_system_fonts();let tree=usvg::Tree::from_str(source,&options).unwrap();let mut expected=tiny_skia::Pixmap::new(320,180).unwrap();resvg::render(&tree,tiny_skia::Transform::identity(),&mut expected.as_mut());let actual=crate::compositor::render_export(doc,1).unwrap();actual.data().iter().zip(expected.data()).map(|(a,b)|a.abs_diff(*b)as f32).sum::<f32>()/actual.data().len()as f32}
    #[test]
    fn native_type_size_boundaries_leading_resize_and_affine_editing(){
        let mut run=sample("AAAA\nBBBB");run.set_character_style(2,4,|s|s.px=Some(64.));run.manual_kern.insert(2,125.);
        let line=crate::text::compose(&run);let before=line[0].glyphs.iter().find(|g|g.cluster==2).unwrap().x;
        run.manual_kern.clear();let line=crate::text::compose(&run);assert!((before-line[0].glyphs.iter().find(|g|g.cluster==2).unwrap().x-8.).abs()<0.01);assert!(line[0].height>=76.);
        run.contours=crate::text::shape(&run);let mut geom=Geom::Text(run);let src=geom.bbox();geom.map_into(src,crate::geom::Bounds::from_min_size(src.min,src.size()*2.));let Geom::Text(mut run)=geom else{unreachable!()};assert_eq!(run.character_style(2).px,Some(128.));
        run.transform=Some([0.,1.,-1.,0.,150.,0.]);run.contours=crate::text::shape(&run);let(top,bottom)=crate::text::caret_segment(&run,2);assert!((top.x-bottom.x).abs()>100. && (top.y-bottom.y).abs()<0.01);let quads=crate::text_geometry::selection_quads(&run,2,3).unwrap();assert!((quads[0][0]-run.map_point(crate::text::selection_rects(&run,2,3)[0].0)).length()<0.01);
    }
    #[test]
    fn native_type_paint_ligatures_rotation_gradient_masks_and_svg_matrix(){
        let mut run=sample("fi fi");run.set_character_style(1,2,|s|s.fill=Some(Fill::Solid(Rgba::rgb(220,40,60))));assert_eq!(crate::text::glyph_count(&run),4);assert_eq!(crate::text::paint_groups(&run,&Fill::Solid(Rgba::rgb(0,0,220))).len(),3);
        run.transform=Some([1.,0.15,0.25,1.,30.,5.]);let mut doc=document(run);let shape=&mut doc.layers[0].kind.shapes_mut().unwrap()[0];shape.rotation=0.3;shape.style.fill=Fill::Linear{from:[0.,0.],to:[1.,0.],c0:Rgba::rgb(0,0,220),c1:Rgba::rgb(0,200,80)};shape.opacity=0.7;
        let svg=crate::svg::export(&doc).unwrap();assert!(compare(&svg,&doc)<0.1,"SVG and native painted affine type differ: {}",compare(&svg,&doc));
        doc.layers[0].kind.shapes_mut().unwrap()[0].visible=false;let svg=crate::svg::export(&doc).unwrap();assert!(compare(&svg,&doc)<0.01);
        let mut plain=sample("Matrix");plain.transform=Some([0.9,0.3,-0.2,1.1,20.,4.]);let doc=document(plain);let svg=crate::svg::export(&doc).unwrap();assert!(compare(&svg,&doc)<0.1);
    }
    #[test]
    fn native_type_threaded_frame_uses_range_faces_and_affine_after_layout(){
        let mut story=sample("MMMM WWWW");story.frame=Some(crate::text_geometry::TextFrame{size:Pt::new(240.,100.),..Default::default()});story.set_character_style(5,9,|s|s.font=Some("/usr/share/fonts/liberation/LiberationSans-Bold.ttf".into()));let plain=crate::text_geometry::layout_frame(&story,&story,0,&[],false);assert!(!plain.contours.is_empty());story.transform=Some([1.,0.2,0.3,1.,20.,10.]);let mapped=crate::text_geometry::layout_frame(&story,&story,0,&[],false);assert!((mapped.contours[0][0]-story.map_point(plain.contours[0][0])).length()<0.001);assert!((mapped.selections[0].2[0]-story.map_point(plain.selections[0].2[0])).length()<0.001);
        let mut geom=Geom::Text(story);let bounds=geom.bbox();geom.map_into_with_text_scale(bounds,crate::geom::Bounds::from_min_size(bounds.min,bounds.size()*2.));let Geom::Text(run)=geom else{unreachable!()};assert_eq!(run.px,32.);assert!((run.transform.unwrap()[0]-2.).abs()<0.001);
    }
    #[test]
    fn native_type_frame_path_paints_sizes_obstacles_and_wrapped_ink(){
        use crate::text_geometry::{TextFrame,TextOnPath,ArcPath,Obstacle,TextWrap,WrapMode};
        use crate::geom::Bounds;
        let red=Fill::Solid(Rgba::rgb(220,20,40));
        let mut story=sample("MMMM WWWW");story.px=20.;story.origin=Pt::new(20.,80.);story.wrap_width=Some(230.);story.set_character_style(0,1,|s|{s.px=Some(96.);s.fill=Some(red.clone());});story.contours=crate::text::shape(&story);
        let bounds=Geom::Text(story.clone()).bbox();assert!(story.contours.iter().flatten().all(|p|p.y>=bounds.min.y-0.01));
        let doc=document(story.clone());let image=crate::compositor::render_export(&doc,1).unwrap();assert!(image.pixels().iter().enumerate().any(|(i,p)|i/320<50 && p.red().saturating_sub(p.blue())>20));
        story.frame=Some(TextFrame{size:Pt::new(250.,150.),..Default::default()});
        let obstacle=Obstacle{id:7,order:(0,0),bounds:Bounds::from_min_size(Pt::new(20.,60.),Pt::new(90.,100.)),contours:vec![],wrap:TextWrap{mode:WrapMode::BoundingBox,..Default::default()},locked:false};
        let plain=crate::text_geometry::layout_frame(&story,&story,0,&[obstacle.clone()],false);assert!(!plain.contours.is_empty());assert!(plain.caret_heights.iter().any(|(i,h)|*i==0&&*h>95.));
        let mut shifted=obstacle;shifted.bounds.min.x+=100.;shifted.bounds.max.x+=100.;story.transform=Some([1.,0.,0.,1.,100.,0.]);let mapped=crate::text_geometry::layout_frame(&story,&story,0,&[shifted],false);assert_eq!(plain.contours.len(),mapped.contours.len());assert!((mapped.contours[0][0]-story.map_point(plain.contours[0][0])).length()<0.01);
        story.transform=None;story.layout=Some(crate::text_geometry::layout_frame(&story,&story,0,&[],false));assert!(crate::text::paint_groups(&story,&Fill::None).iter().any(|(f,c)|*f==red&&!c.is_empty()));let doc=document(story.clone());assert!(compare(&crate::svg::export(&doc).unwrap(),&doc)<0.1);
        story.frame=None;story.layout=None;story.on_path=Some(TextOnPath{cache:Some(ArcPath::new(vec![[Pt::new(10.,100.),Pt::new(110.,100.),Pt::new(210.,100.),Pt::new(310.,100.)]],false)),..Default::default()});let path=crate::text_geometry::layout_on_path(&story);assert!(path.caret_heights.iter().any(|(i,h)|*i==0&&*h>95.));assert!(crate::text::paint_groups(&story,&Fill::None).iter().any(|(f,c)|*f==red&&!c.is_empty()));let doc=document(story);assert!(compare(&crate::svg::export(&doc).unwrap(),&doc)<0.1);
    }

    #[test]
    fn native_type_clipped_paint_ranges_and_affine_path_release(){
        let mut run=sample("MMMMM");run.frame=Some(crate::text_geometry::TextFrame{size:Pt::new(20.,100.),..Default::default()});run.set_character_style(0,1,|s|s.fill=Some(Fill::Solid(Rgba::rgb(220,20,40))));let layout=crate::text_geometry::layout_frame(&run,&run,0,&[],false);assert!(layout.paint_ranges.iter().all(|(_,r)|r.end<=layout.contours.len()));let _=crate::text::paint_groups(&run,&Fill::None);let doc=document(run);crate::compositor::render_export(&doc,1).unwrap();
        let mut run=sample("Release");run.on_path=Some(crate::text_geometry::TextOnPath{cache:Some(crate::text_geometry::ArcPath::new(vec![[Pt::new(20.,70.),Pt::new(110.,70.),Pt::new(210.,70.),Pt::new(300.,70.)]],false)),..Default::default()});run.transform=Some([1.,0.,0.,1.,100.,0.]);let before=crate::text::caret_pt(&run,0);crate::text_geometry::release(&mut run);assert!((crate::text::caret_pt(&run,0)-before).length()<0.01);
    }

    #[test]
    fn native_type_visible_overflow_paints_and_affine_autoheight_stays_local(){
        use crate::text_geometry::{TextFrame,Overflow};
        let mut run=sample("MMMMM");run.frame=Some(TextFrame{size:Pt::new(20.,100.),overflow:Overflow::Visible,..Default::default()});run.set_character_style(0,1,|s|s.fill=Some(Fill::Solid(Rgba::rgb(220,20,40))));let doc=document(run);let image=crate::compositor::render_export(&doc,1).unwrap();assert!(image.pixels().iter().enumerate().any(|(i,p)|i%320>60&&p.alpha()>0));assert!(compare(&crate::svg::export(&doc).unwrap(),&doc)<0.1);
        let mut run=sample("Auto height words wrap over multiple lines");run.frame=Some(TextFrame{size:Pt::new(120.,100.),auto_height:true,..Default::default()});let mut plain=document(run.clone());run.transform=Some([1.,0.,0.,1.,0.,100.]);let mut shifted=document(run);crate::text_geometry::reflow_areas(&mut plain);crate::text_geometry::reflow_areas(&mut shifted);let height=|d:&Document|{let Geom::Text(t)=&d.layers[0].kind.shapes().unwrap()[0].geom else{unreachable!()};t.frame.as_ref().unwrap().size.y};assert!((height(&plain)-height(&shifted)).abs()<0.01);
    }

}
