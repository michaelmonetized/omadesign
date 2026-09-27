//! Document-aware editable text geometry. Path samples are adaptive, in world space.
use crate::document::{Cmd, Document, Shape};
use crate::geom::{Geom, Pt, TextAlign, TypeRun};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathAlign { #[default] Baseline, Center, Top, Bottom }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOnPath {
    pub path_id: u64,
    pub start: f32,
    pub end: f32,
    pub flip: bool,
    pub baseline_shift: f32,
    pub align: PathAlign,
    pub spacing: f32,
    #[serde(skip)]
    pub cache: Option<ArcPath>,
}
impl Default for TextOnPath {
    fn default() -> Self { Self { path_id: 0, start: 0., end: 1., flip: false, baseline_shift: 0., align: PathAlign::Baseline, spacing: 0., cache: None } }
}

/// Adaptive cubic arc-length parameterization; retain exact cubic tangents.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcPath {
    pub cubics: Vec<[Pt; 4]>,
    pub closed: bool,
    pub length: f32,
    samples: Vec<(f32, usize, f32)>,
}
impl ArcPath {
    pub fn new(cubics: Vec<[Pt; 4]>, closed: bool) -> Self {
        let mut result = Self { cubics, closed, length: 0., samples: vec![] };
        for i in 0..result.cubics.len() {
            let c = result.cubics[i];
            if result.samples.is_empty() { result.samples.push((0., i, 0.)); }
            result.subdivide(c, i, 0., 1., 0);
        }
        result
    }
    fn subdivide(&mut self, c: [Pt; 4], segment: usize, t0: f32, t1: f32, depth: u8) {
        let chord = (c[3] - c[0]).length();
        let polygon = (c[1]-c[0]).length() + (c[2]-c[1]).length() + (c[3]-c[2]).length();
        if depth >= 18 || (polygon - chord <= 0.002 && chord <= 8.) {
            self.length += (chord + polygon) * 0.5;
            self.samples.push((self.length, segment, t1));
        } else {
            let (a,b) = crate::geom::split_cubic(c[0], c[1], c[2], c[3], 0.5);
            let mid = (t0+t1)*0.5;
            self.subdivide(a, segment, t0, mid, depth+1);
            self.subdivide(b, segment, mid, t1, depth+1);
        }
    }
    pub fn point_and_tangent_at(&self, distance: f32) -> (Pt, Pt) {
        if self.cubics.is_empty() || self.length < 1e-6 { return (Pt::ZERO, Pt::new(1.,0.)); }
        let d = if self.closed { distance.rem_euclid(self.length) } else { distance.clamp(0.,self.length) };
        let upper = self.samples.partition_point(|s| s.0 < d).min(self.samples.len()-1).max(1);
        let a = self.samples[upper-1];
        let b = self.samples[upper];
        let start_t = if a.1 == b.1 { a.2 } else { 0. };
        let fraction = if b.0-a.0 > 1e-6 { (d-a.0)/(b.0-a.0) } else { 0. };
        let t = start_t + (b.2-start_t)*fraction;
        let c = self.cubics[b.1];
        let point = crate::geom::eval_cubic(c[0],c[1],c[2],c[3],t);
        let mut tangent = ((c[1]-c[0])*(3.*(1.-t).powi(2)) + (c[2]-c[1])*(6.*(1.-t)*t) + (c[3]-c[2])*(3.*t*t)).normalized();
        if tangent.length_sq()<0.1 { tangent = (c[3]-c[0]).normalized(); }
        (point,tangent)
    }
    pub fn nearest_distance(&self, point: Pt) -> f32 {
        let mut best = (f32::INFINITY, 0.);
        for pair in self.samples.windows(2) {
            let a = self.point_and_tangent_at(pair[0].0).0;
            let b = self.point_and_tangent_at(pair[1].0 - 0.00001).0;
            let edge = b-a;
            let t = ((point-a).dot(edge)/edge.length_sq().max(1e-9)).clamp(0.,1.);
            let distance = (point-a-edge*t).length_sq();
            if distance < best.0 { best = (distance, pair[0].0+(pair[1].0-pair[0].0)*t); }
        }
        best.1
    }
}

pub fn guide_path(shape: &Shape) -> Option<ArcPath> {
    if matches!(shape.geom, Geom::Text(_)) { return None; }
    let converted = shape.geom.to_path();
    let (anchors,closed) = match &converted {
        Geom::Path {anchors,closed} => (anchors,*closed),
        Geom::Paths {paths,..} => { let p=paths.first()?; (&p.anchors,p.closed) },
        _ => return None,
    };
    let center = shape.geom.bbox().center();
    let cubics = crate::geom::path_cubics(anchors,closed).into_iter().map(|c| c.map(|p| p.rotate_about(center,shape.rotation))).collect();
    Some(ArcPath::new(cubics,closed))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextGeometryLayout {
    pub contours: Vec<Vec<Pt>>,
    pub caret_heights: Vec<(usize, f32)>,
    pub carets: Vec<(usize, Pt, Pt)>, // story index, baseline position, tangent
    pub selections: Vec<(usize,usize,[Pt;4])>,
    pub overflow: bool,
    pub source: String,
    pub visible_start: usize,
    pub visible_end: usize,
    pub visible_text: String,
}

pub fn path_interval(on: &TextOnPath) -> Option<(f32,f32)> {
    let path=on.cache.as_ref()?;
    let start=on.start*path.length;
    let span=if path.closed { let fraction=on.end-on.start; if fraction.abs()>=0.99999 {1.} else {fraction.rem_euclid(1.)} } else {(on.end.clamp(0.,1.)-on.start.clamp(0.,1.)).max(0.)};
    Some((start,span*path.length))
}

pub fn path_position(run: &TypeRun, distance: f32) -> Option<(Pt,Pt)> {
    let on=run.on_path.as_ref()?;
    let path=on.cache.as_ref()?;
    let (start,span)=path_interval(on)?;
    let direction=if on.flip {-1.} else {1.};
    let d=if on.flip {start+span-distance} else {start+distance};
    let (point,tangent)=path.point_and_tangent_at(d);
    let tangent=tangent*direction;
    let align=match on.align {PathAlign::Baseline=>0.,PathAlign::Center=>run.px*0.35,PathAlign::Top=>run.px*0.8,PathAlign::Bottom=>-run.px*0.2};
    Some((point+tangent.perp()*(align-on.baseline_shift),tangent))
}

pub fn layout_on_path(run: &TypeRun) -> TextGeometryLayout {
    let Some(on)=run.on_path.as_ref() else {return TextGeometryLayout::default()};
    let Some((_,span))=path_interval(on) else {return TextGeometryLayout::default()};
    let mut plain=run.clone();
    plain.origin=Pt::ZERO; plain.on_path=None; plain.layout=None; plain.wrap_width=None;
    plain.content=plain.content.replace('\n'," ");
    plain.tracking+=on.spacing;
    let lines=crate::text::compose(&plain);
    let mut result=TextGeometryLayout::default();
    let width=lines.iter().map(|line|line.width).fold(0f32,f32::max);
    let offset=match run.align {TextAlign::Start=>0.,TextAlign::Center=>(span-width).max(0.)*0.5,TextAlign::End=>(span-width).max(0.), _=>0.};
    for line in lines.iter() {
        for glyph in &line.glyphs {
            let center=offset+glyph.x+glyph.advance*0.5;
            if center<0. || center+glyph.advance*0.5>span+0.001 {result.overflow=true;continue;}
            let Some((point,tangent))=path_position(run,center) else {continue};
            for contour in crate::text::glyph_contours(&plain,glyph) {
                result.contours.push(contour.into_iter().map(|p|point+tangent*(p.x-glyph.x-glyph.advance*0.5)+tangent.perp()*(p.y-line.baseline)).collect());
            }
            result.visible_end=result.visible_end.max(glyph.cluster+1);
        }
        for &(index, x) in &line.carets {
            let distance = (offset + x).clamp(0., span);
            if let Some((point, tangent)) = path_position(run, distance) {
                let metrics=crate::text::character_metrics(run,index.min(run.content.chars().count().saturating_sub(1)));
                result.carets.push((index, point-tangent.perp()*metrics.baseline_shift.unwrap(), tangent));
                result.caret_heights.push((index,run.px*metrics.vscale.unwrap()/100.));
            }
        }
        for pair in line.carets.windows(2) {
            let (a, x) = pair[0];
            let (b, y) = pair[1];
            if offset + x > span {
                continue;
            }
            if let (Some((p, t)), Some((q, u))) = (
                path_position(run, (offset + x).min(span)),
                path_position(run, (offset + y).min(span)),
            ) {
                let metrics=crate::text::character_metrics(run,a);let height=run.px*metrics.vscale.unwrap()/100.;
                let p=p-t.perp()*metrics.baseline_shift.unwrap();let q=q-u.perp()*metrics.baseline_shift.unwrap();
                result.selections.push((
                    a,
                    b,
                    [
                        p - t.perp() * height * 0.9,
                        q - u.perp() * height * 0.9,
                        q + u.perp() * height * 0.2,
                        p + t.perp() * height * 0.2,
                    ],
                ));
            }
        }
    }
    result.overflow|=width>span+0.001;
    result.visible_text=run.content.clone();
    result
}
pub fn shape_on_path(run: &TypeRun) -> Vec<Vec<Pt>> {layout_on_path(run).contours}

pub fn caret_frame(run: &TypeRun, index: usize) -> Option<(Pt,Pt)> {
    let computed;
    let layout=if let Some(layout)=&run.layout {layout} else if run.on_path.is_some() {computed=layout_on_path(run);&computed} else {return None};
    layout.carets.iter().min_by_key(|c|c.0.abs_diff(index)).map(|c|(c.1,c.2))
}
pub fn hit_char(run: &TypeRun, point: Pt) -> Option<usize> {
    let computed;
    let layout=if let Some(layout)=&run.layout {layout} else if run.on_path.is_some() {computed=layout_on_path(run);&computed} else {return None};
    layout.carets.iter().min_by(|a,b|(a.1-point).length_sq().total_cmp(&(b.1-point).length_sq())).map(|c|c.0)
}
pub fn selection_quads(run:&TypeRun,a:usize,b:usize)->Option<Vec<[Pt;4]>> {
    let computed;
    let layout=if let Some(layout)=&run.layout {layout} else if run.on_path.is_some() {computed=layout_on_path(run);&computed} else {return None};
    let (lo,hi)=(a.min(b),a.max(b));
    Some(layout.selections.iter().filter(|s|s.0<hi&&s.1>lo).map(|s|s.2).collect())
}

/// Resolve guides only after every layer exists. This pass also runs during live drags.
pub fn reflow(doc: &mut Document) {
    let guides:HashMap<_,_>=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().filter_map(|s|guide_path(s).map(|p|(s.id,p))).collect();
    for layer in &mut doc.layers {if let Some(shapes)=layer.kind.shapes_mut() {for shape in shapes {
        if let Geom::Text(run)=&mut shape.geom && let Some(on)=&mut run.on_path {
            let next=guides.get(&on.path_id).cloned();
            let changed=on.cache!=next;
            on.cache=next;
            let source=serde_json::to_string(run).unwrap_or_default();
            if changed || run.layout.as_ref().is_none_or(|l|l.source!=source) {
                let mut layout=layout_on_path(run);layout.source=source;
                run.contours=layout.contours.clone();run.layout=Some(layout);
            }
        }
    }}}
}

pub fn release(run: &mut TypeRun) {
    if let Some((point,_))=caret_frame(run,0) {run.origin=point;}
    run.on_path=None;run.layout=None;
    run.contours=crate::text::shape(run);
}

/// Record automatic detachments in the same history transaction as guide deletion.
pub fn reconcile_links(doc:&Document)->Vec<Cmd> {
    let ids:std::collections::HashSet<_>=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().map(|s|s.id).collect();
    let mut commands=vec![];
    for (layer,l) in doc.layers.iter().enumerate() {for shape in l.kind.shapes().unwrap_or(&[]) {
        if let Geom::Text(run)=&shape.geom && run.on_path.as_ref().is_some_and(|on|!ids.contains(&on.path_id)) {
            let mut after=run.clone();release(&mut after);
            commands.push(Cmd::SetGeom {layer,id:shape.id,before:shape.geom.clone(),after:Geom::Text(after),rot_before:shape.rotation,rot_after:shape.rotation});
        }
    }}
    commands
}

pub fn remap_copy(shape:&mut Shape,ids:&HashMap<u64,u64>) {
    if let Geom::Text(run)=&mut shape.geom && let Some(on)=&mut run.on_path {
        if let Some(id)=ids.get(&on.path_id) {on.path_id=*id;on.cache=None;run.layout=None;} else {release(run);}
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn straight_arc_positions_and_clamps() {
        let a=Pt::new(10.,20.);let b=Pt::new(110.,20.);
        let path=ArcPath::new(vec![[a,a.lerp(b,1./3.),a.lerp(b,2./3.),b]],false);
        assert!((path.length-100.).abs()<0.01);
        assert!((path.point_and_tangent_at(25.).0-Pt::new(35.,20.)).length()<0.01);
        assert_eq!(path.point_and_tangent_at(200.).0,b);
    }
    #[test] fn circle_arc_length_wrap_and_flip() {
        let guide=Shape::new(Geom::Ellipse{center:Pt::ZERO,radii:Pt::splat(100.)},Default::default());
        let path=guide_path(&guide).unwrap();assert!((path.length-std::f32::consts::TAU*100.).abs()<0.2);
        let a=path.point_and_tangent_at(25.);let b=path.point_and_tangent_at(path.length+25.);
        assert!((a.0-b.0).length()<0.01);
        let mut run=TypeRun::default();run.on_path=Some(TextOnPath{cache:Some(path),..Default::default()});
        let forward=path_position(&run,50.).unwrap();run.on_path.as_mut().unwrap().flip=true;
        let reverse=path_position(&run,run.on_path.as_ref().unwrap().cache.as_ref().unwrap().length-50.).unwrap();
        assert!((forward.0-reverse.0).length()<0.01);assert!(forward.1.dot(reverse.1) < -0.999);
    }
}

#[cfg(test)] mod lifecycle_tests {
    use super::*;
    use crate::{app::Studio,document::Style};
    fn scene()->(Studio,u64,u64){let mut studio=Studio::new();let guide=Shape::new(Geom::Ellipse{center:Pt::new(300.,300.),radii:Pt::splat(120.)},Style::default());let guide_id=guide.id;studio.doc.layers[1].kind.shapes_mut().unwrap().push(guide);studio.active_layer=Some(1);studio.place_text_on_path(Pt::new(420.,300.),(1,guide_id));studio.type_insert("Curved ligatures ffi");studio.commit_type_edit();let id=studio.selection[0].1;(studio,guide_id,id)}
    #[test] fn linked_paths_reflow_on_transform_release_on_delete_and_restore_on_undo(){let(mut studio,guide,id)=scene();let before=studio.doc.find_shape(1,id).unwrap().geom.clone();studio.doc.find_shape_mut(1,guide).unwrap().geom.translate(Pt::new(50.,20.));studio.mark();let Geom::Text(after)=&studio.doc.find_shape(1,id).unwrap().geom else{panic!()};let Geom::Text(before)=before else{panic!()};assert!((after.contours[0][0]-before.contours[0][0]-Pt::new(50.,20.)).length()<0.01);studio.selection=vec![(1,guide)];studio.delete_selection();let Geom::Text(run)=&studio.doc.find_shape(1,id).unwrap().geom else{panic!()};assert!(run.on_path.is_none());studio.undo();let Geom::Text(run)=&studio.doc.find_shape(1,id).unwrap().geom else{panic!()};assert_eq!(run.on_path.as_ref().unwrap().path_id,guide);}
    #[test] fn linked_save_load_copy_and_outlined_svg_preserve_positions(){let(studio,guide,id)=scene();let encoded=crate::project::encode(&studio.doc).unwrap();let restored=crate::project::decode(&encoded).unwrap();let original=studio.doc.find_shape(1,id).unwrap();let loaded=restored.find_shape(1,id).unwrap();let Geom::Text(a)=&original.geom else{panic!()};let Geom::Text(b)=&loaded.geom else{panic!()};assert_eq!(a.contours,b.contours);let svg=crate::svg::export(&restored).unwrap();assert!(svg.contains(&format!("<path id=\"oma-{id}\"")));let mut copy=original.clone();remap_copy(&mut copy,&HashMap::from([(id,id+1000),(guide,guide+1000)]));let Geom::Text(c)=copy.geom else{panic!()};assert_eq!(c.on_path.unwrap().path_id,guide+1000);let mut copy=original.clone();remap_copy(&mut copy,&HashMap::from([(id,id+1000)]));let Geom::Text(c)=copy.geom else{panic!()};assert!(c.on_path.is_none());}
    #[test] fn every_primitive_and_compound_can_guide_type(){for geom in [Geom::Rect{origin:Pt::ZERO,size:Pt::new(100.,50.),radius:4.},Geom::Ellipse{center:Pt::ZERO,radii:Pt::splat(30.)},Geom::Line{a:Pt::ZERO,b:Pt::new(100.,0.)},Geom::Polygon{center:Pt::ZERO,radii:Pt::splat(40.),sides:5},Geom::Star{center:Pt::ZERO,outer:Pt::splat(40.),inner:0.5,points:5},Geom::Paths{paths:vec![crate::geom::PathContour{anchors:vec![crate::geom::Anchor::corner(Pt::ZERO),crate::geom::Anchor::corner(Pt::new(100.,0.))],closed:false}],winding:true}]{assert!(guide_path(&Shape::new(geom,Style::default())).unwrap().length>10.);}}
}
