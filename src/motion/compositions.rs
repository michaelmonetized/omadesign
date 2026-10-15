//! Portable nested compositions with bounded traversal.

use crate::{document::Document, geom::Pt};
use serde::{Deserialize, Serialize};

pub const MAX_DEPTH: usize = 8;
pub const MAX_COMPOSITIONS: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Composition {
    pub id: u64,
    pub name: String,
    pub document: Box<Document>,
    #[serde(default)]
    pub start: f32,
    pub end: f32,
    #[serde(default)]
    pub source_start: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub time_remap: Vec<super::Key>,
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(default)]
    pub looped: bool,
    #[serde(default)]
    pub position: Pt,
    #[serde(default)]
    pub anchor: Pt,
    #[serde(default = "unit")]
    pub scale: Pt,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
}
impl PartialEq for Composition {
    fn eq(&self, other: &Self) -> bool {
        match (serde_json::to_value(self), serde_json::to_value(other)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}
fn one() -> f32 {
    1.
}
fn yes() -> bool {
    true
}
fn unit() -> Pt {
    Pt::splat(1.)
}

impl Composition {
    /// Embed an editable document without changing its artwork or timeline.
    /// Args: child document. Returns a new composition layer at the origin.
    pub fn new(document: Document) -> Self {
        Self {
            id: crate::document::next_id(),
            name: document.name.clone(),
            end: document.motion.duration,
            document: Box::new(document),
            start: 0.,
            source_start: 0.,
            time_remap: vec![],
            speed: 1.,
            looped: false,
            position: Pt::ZERO,
            anchor: Pt::ZERO,
            scale: unit(),
            rotation: 0.,
            opacity: 1.,
            visible: true,
        }
    }

    /// Convert parent time to source time.
    /// Args: parent sample in seconds. Returns local seconds while the layer is visible.
    pub fn local_time(&self, time: f32) -> Option<f32> {
        if !self.visible || time < self.start || time > self.end {
            return None;
        }
        let time = super::eval_keys(&self.time_remap, time, self.end)
            .unwrap_or_else(|| self.source_start + (time - self.start) * self.speed);
        let duration = self.document.motion.duration.max(0.01);
        Some(if self.looped {
            time.rem_euclid(duration)
        } else {
            time.clamp(0., duration)
        })
    }
}

/// Validate bounded editable motion data recursively.
/// Args: root document. Returns a named error before malformed data reaches rendering.
pub fn validate(doc: &Document) -> Result<(), String> {
    fn visit(doc: &Document, depth: usize, count: &mut usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("CompositionDepth: at most eight nested compositions".into());
        }
        if !doc.width.is_finite() || !doc.height.is_finite() || doc.width <= 0. || doc.height <= 0.
        {
            return Err("CompositionSize: dimensions must be finite and positive".into());
        }
        let motion = &doc.motion;
        if let Some(scene) = &motion.scene3d {
            scene.validate()?;
        }
        if !motion.duration.is_finite()
            || !(0.01..=3600.).contains(&motion.duration)
            || !motion.fps.is_finite()
            || !(1.0..=240.).contains(&motion.fps)
        {
            return Err("MotionRange: invalid duration or frame rate".into());
        }
        if motion.expressions.len() > super::expressions::MAX_EXPRESSIONS
            || motion.expressions.iter().any(|e| {
                e.source.len() > super::expressions::MAX_SOURCE || e.prop == super::Prop::Fill
            })
        {
            return Err("ExpressionLimit: invalid expression data".into());
        }
        if motion.tracks.len() > 10_000
            || motion.tracks.iter().map(|t| t.keys.len()).sum::<usize>() > 100_000
        {
            return Err("KeyframeLimit: composition exceeds keyframe budget".into());
        }
        if motion.tracks.iter().any(|t| {
            t.keys.windows(2).any(|k| k[0].t >= k[1].t)
                || t.keys.iter().any(|k| {
                    !k.t.is_finite() || !k.value.is_finite() || k.t < 0. || !k.ease.valid()
                })
        }) {
            return Err(
                "KeyframeData: keys must have finite ordered times, values and valid curves".into(),
            );
        }
        let mut ids = std::collections::HashSet::new();
        for comp in &motion.compositions {
            *count += 1;
            if *count > MAX_COMPOSITIONS {
                return Err("CompositionLimit: at most 128 compositions".into());
            }
            if !ids.insert(comp.id)
                || comp.id == 0
                || comp.id >= u64::MAX - 200_000
                || doc.layers.iter().any(|l| {
                    l.id == comp.id
                        || l.kind
                            .shapes()
                            .is_some_and(|s| s.iter().any(|s| s.id == comp.id))
                })
            {
                return Err("CompositionId: duplicate or zero target id".into());
            }
            if [
                comp.start,
                comp.end,
                comp.source_start,
                comp.speed,
                comp.position.x,
                comp.position.y,
                comp.anchor.x,
                comp.anchor.y,
                comp.scale.x,
                comp.scale.y,
                comp.rotation,
                comp.opacity,
            ]
            .iter()
            .any(|x| !x.is_finite())
                || comp.end <= comp.start
                || comp.speed == 0.
                || comp.scale.x == 0.
                || comp.scale.y == 0.
                || !(0.0..=1.).contains(&comp.opacity)
            {
                return Err("CompositionTransform: invalid timing, transform or opacity".into());
            }
            if comp.time_remap.windows(2).any(|p| p[0].t >= p[1].t)
                || comp.time_remap.len() > 10_000
                || comp
                    .time_remap
                    .iter()
                    .any(|k| !k.t.is_finite() || !k.value.is_finite() || !k.ease.valid())
            {
                return Err("CompositionTime: invalid time-remap keys".into());
            }
            visit(&comp.document, depth + 1, count)?;
        }
        Ok(())
    }
    visit(doc, 0, &mut 0)
}

/// Write a portable motion project with native nested artwork and curves intact.
/// Args: document. Returns an interchange envelope containing the versioned project.
pub fn export(doc: &Document) -> Result<String, String> {
    validate(doc)?;
    let project: serde_json::Value =
        serde_json::from_str(&crate::project::encode(doc)?).map_err(|e| e.to_string())?;
    serde_json::to_string(
        &serde_json::json!({"format":"omadesign-motion","version":1,"project":project}),
    )
    .map_err(|e| e.to_string())
}

/// Read a portable motion project.
/// Args: bounded UTF-8 interchange text. Returns the full editable document.
pub fn import(source: &str) -> Result<Document, String> {
    if source.len() > 64 * 1024 * 1024 {
        return Err("CompositionLimit: interchange exceeds 64 MiB".into());
    }
    let v: serde_json::Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    if v["format"] != "omadesign-motion" || v["version"] != 1 {
        return Err("CompositionFormat: unsupported motion project".into());
    }
    let doc = crate::project::decode(&v["project"].to_string())?;
    validate(&doc)?;
    Ok(doc)
}

pub(super) fn append_lottie(
    doc: &Document,
    fps: f32,
    assets: &mut Vec<serde_json::Value>,
    layers: &mut Vec<serde_json::Value>,
    mut index: i32,
) -> Result<(), String> {
    use serde_json::json;
    for comp in doc.motion.compositions.iter().filter(|c| c.visible) {
        let mut child: serde_json::Value =
            serde_json::from_str(&super::export_lottie(&comp.document)?)
                .map_err(|e| e.to_string())?;
        let ratio = fps / comp.document.motion.fps;
        fn retime(v: &mut serde_json::Value, ratio: f32) {
            match v {
                serde_json::Value::Object(o) => {
                    for (key, value) in o {
                        if ["t", "ip", "op", "st"].contains(&key.as_str()) && value.is_number() {
                            *value = json!(value.as_f64().unwrap_or(0.) * ratio as f64);
                        } else {
                            retime(value, ratio);
                        }
                    }
                }
                serde_json::Value::Array(a) => {
                    for v in a {
                        retime(v, ratio)
                    }
                }
                _ => {}
            }
        }
        retime(&mut child, ratio);
        let prefix = format!("comp-{}-", comp.id);
        fn prefix_refs(v: &mut serde_json::Value, prefix: &str) {
            match v {
                serde_json::Value::Object(o) => {
                    for (key, v) in o {
                        if ["id", "refId"].contains(&key.as_str()) && v.is_string() {
                            *v = json!(format!("{prefix}{}", v.as_str().unwrap_or_default()));
                        } else {
                            prefix_refs(v, prefix)
                        }
                    }
                }
                serde_json::Value::Array(a) => {
                    for v in a {
                        prefix_refs(v, prefix)
                    }
                }
                _ => {}
            }
        }
        prefix_refs(&mut child, &prefix);
        if let Some(nested) = child["assets"].as_array() {
            assets.extend(nested.iter().cloned());
        }
        let id = format!("{prefix}root");
        assets.push(json!({"id":id,"w":comp.document.width,"h":comp.document.height,"layers":child["layers"]}));
        let mut times = doc.motion.sample_times(comp.id);
        let count = ((comp.end - comp.start) * fps).ceil().clamp(1., 10000.) as usize;
        times.extend(
            (0..=count).map(|i| comp.start + (comp.end - comp.start) * i as f32 / count as f32),
        );
        times.sort_by(f32::total_cmp);
        times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let ks = json!({
            "a":{"a":0,"k":[comp.anchor.x,comp.anchor.y,0]},
            "p":super::lottie_vec2_anim(&times,fps,|t|{let p=doc.motion.pose(comp.id,t);[comp.position.x+p.dx,comp.position.y+p.dy]}),
            "s":super::lottie_vec2_anim(&times,fps,|t|{let p=doc.motion.pose(comp.id,t);[comp.scale.x*p.scale*p.width_scale*100.,comp.scale.y*p.scale*p.height_scale*100.]}),
            "r":super::lottie_scalar_anim(&times,fps,|t|(comp.rotation+doc.motion.pose(comp.id,t).rotation).to_degrees()),
            "o":super::lottie_scalar_anim(&times,fps,|t|comp.opacity*doc.motion.pose(comp.id,t).opacity.unwrap_or(1.)*100.)
        });
        let tm = super::lottie_scalar_anim(&times, fps, |t| comp.local_time(t).unwrap_or(0.));
        layers.insert(0,json!({"ty":0,"ddd":0,"ind":index,"refId":id,"nm":comp.name,"w":comp.document.width,"h":comp.document.height,"ip":comp.start*fps,"op":comp.end*fps,"st":0,"sr":1,"tm":tm,"ks":ks}));
        index += 1;
    }
    Ok(())
}

pub(super) fn lottie_composition(
    layer: &serde_json::Value,
    root: &serde_json::Value,
    fps: f32,
    ip: f32,
    depth: usize,
    active: &mut Vec<String>,
) -> Result<Composition, String> {
    use serde_json::json;
    if depth > MAX_DEPTH {
        return Err("CompositionDepth: Lottie nesting exceeds eight".into());
    }
    let id = layer["refId"]
        .as_str()
        .ok_or("CompositionReference: missing precomposition id")?;
    if active.iter().any(|s| s == id) {
        return Err("CompositionCycle: cyclic Lottie assets".into());
    }
    let asset = root["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["id"] == id))
        .ok_or("CompositionReference: missing Lottie asset")?;
    if !asset["layers"].is_array() {
        return Err("CompositionUnsupported: image assets require raster import".into());
    }
    active.push(id.into());
    let child = json!({"v":"5.7.4","w":asset["w"].as_f64().or(layer["w"].as_f64()).unwrap_or(root["w"].as_f64().unwrap_or(1280.)),"h":asset["h"].as_f64().or(layer["h"].as_f64()).unwrap_or(root["h"].as_f64().unwrap_or(800.)),"fr":fps,"ip":0,"op":root["op"],"layers":asset["layers"],"assets":root["assets"]});
    let imported = super::import_lottie_value(&child, depth, active)?;
    active.pop();
    let mut doc = Document::new(
        layer["nm"].as_str().unwrap_or(id),
        imported.width,
        imported.height,
        96.,
    );
    doc.layers.clear();
    doc.artboards.clear();
    doc.artboardless = true;
    doc.transparent = true;
    let mut vectors = crate::document::Layer::vector("Artwork");
    *vectors.kind.shapes_mut().unwrap() = imported.shapes;
    doc.layers.push(vectors);
    doc.motion = imported.motion;
    let mut comp = Composition::new(doc);
    comp.start = super::frame_to_t(super::js_f32(&layer["ip"]).unwrap_or(ip), fps, ip);
    comp.end = super::frame_to_t(
        super::js_f32(&layer["op"]).unwrap_or(root["op"].as_f64().unwrap_or(60.) as f32),
        fps,
        ip,
    );
    comp.speed = 1. / super::js_f32(&layer["sr"]).unwrap_or(1.);
    let st = super::js_f32(&layer["st"]).unwrap_or(0.);
    comp.source_start = ((comp.start * fps + ip - st) / fps) * comp.speed;
    comp.anchor = super::js_vec2(&layer["ks"]["a"]).unwrap_or(Pt::ZERO);
    comp.position = super::parse_anim_vec2(&layer["ks"]["p"])
        .first()
        .map(|(_, p)| *p)
        .unwrap_or(Pt::ZERO);
    if layer.get("tm").is_some() {
        comp.time_remap = super::parse_anim_scalar(&layer["tm"])
            .into_iter()
            .map(|(t, v)| {
                super::Key::at(
                    super::frame_to_t(t, fps, ip),
                    v,
                    super::lottie_ease(&layer["tm"], t, 0),
                )
            })
            .collect();
    }
    Ok(comp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Layer, Pixels},
        motion::{Ease, Prop},
        project,
    };
    fn child() -> Document {
        let mut doc = Document::new("Child", 12., 12., 96.);
        doc.layers.clear();
        doc.artboards.clear();
        doc.artboardless = true;
        doc.transparent = true;
        let mut layer = Layer::vector("Vector");
        let mut shape = crate::layout::make_frame(Pt::new(1., 1.), Pt::splat(4.));
        shape.style.fill = crate::document::Fill::Solid(crate::color::Rgba::rgb(240, 10, 20));
        let id = shape.id;
        layer.kind.shapes_mut().unwrap().push(shape);
        doc.layers.push(layer);
        doc.motion.set_key(
            id,
            Prop::X,
            0.,
            0.,
            Ease::CubicBezier {
                x1: 0.2,
                y1: 0.,
                x2: 0.4,
                y2: 1.,
            },
        );
        doc.motion.set_key(id, Prop::X, 1., 5., Ease::Hold);
        doc
    }
    #[test]
    fn nested_time_curves_pixels_save_and_interchange_round_trip() {
        let mut nested = child();
        let mut image = Layer::raster("Pixels", 2, 2);
        *image.kind.pixels_mut().unwrap() = Pixels::from_rgba(2, 2, vec![90; 16]).unwrap();
        nested.layers.push(image);
        let mut mid = child();
        let mut a = Composition::new(nested);
        a.position = Pt::new(2., 1.);
        a.start = 0.25;
        a.end = 1.75;
        mid.motion.compositions.push(a);
        let mut root = Document::new("Root", 48., 32., 96.);
        root.layers.clear();
        root.transparent = true;
        root.artboards.clear();
        root.artboardless = true;
        let mut b = Composition::new(mid);
        b.position = Pt::new(10., 4.);
        b.scale = Pt::splat(1.5);
        b.opacity = 0.5;
        b.speed = -1.;
        b.source_start = 2.;
        root.motion.compositions.push(b);
        validate(&root).unwrap();
        let reopened = project::decode(&project::encode(&root).unwrap()).unwrap();
        assert_eq!(reopened.motion, root.motion);
        let bundle = export(&reopened).unwrap();
        assert_eq!(import(&bundle).unwrap().motion, root.motion);
        let before = crate::compositor::render_export_at(&root, 1., 0.5, true).unwrap();
        let after = crate::compositor::render_export_at(&reopened, 1., 0.5, true).unwrap();
        assert_eq!(before.data(), after.data());
        assert!(before.pixels().iter().any(|p| p.alpha() > 0));
        assert!(before.pixels().iter().all(|p| p.alpha() <= 128));
        let decoded = image::load_from_memory(&after.encode_png().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (48, 32));
    }
    #[test]
    fn lottie_precompositions_and_temporal_curves_stay_editable() {
        let mut root = child();
        root.motion.compositions.push(Composition::new(child()));
        let text = crate::motion::export_lottie(&root).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            v["assets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"].as_str().is_some_and(|s| s.starts_with("comp-")))
        );
        assert!(v["layers"].as_array().unwrap().iter().any(|l| l["ty"] == 0));
        let imported = crate::motion::import_lottie(&text).unwrap();
        assert_eq!(imported.motion.compositions.len(), 1);
        assert!(
            imported
                .motion
                .tracks
                .iter()
                .flat_map(|t| &t.keys)
                .any(|k| matches!(k.ease, Ease::CubicBezier { .. }))
        );
        let curve = Ease::CubicBezier {
            x1: 0.25,
            y1: 0.1,
            x2: 0.25,
            y2: 1.,
        };
        assert!((curve.apply(0.5) - 0.8024).abs() < 0.001);
        assert_eq!(Ease::Hold.apply(0.99), 0.);
        assert_eq!(Ease::Hold.apply(1.), 1.);
        let mut cyclic = v.clone();
        cyclic["assets"] = serde_json::json!([{"id":"cycle","w":12,"h":12,"layers":[{"ty":0,"refId":"cycle","ip":0,"op":60}]}]);
        cyclic["layers"] = serde_json::json!([{"ty":0,"refId":"cycle","ip":0,"op":60}]);
        assert!(
            crate::motion::import_lottie(&cyclic.to_string())
                .err()
                .unwrap()
                .contains("Cycle")
        );
    }
    #[test]
    fn nested_resource_errors_are_named_and_cancellable() {
        let mut root = child();
        root.layers.clear();
        let mut inner = child();
        inner.width = 1_000_000.;
        inner.height = 1_000_000.;
        root.motion.compositions.push(Composition::new(inner));
        let visible = crate::compositor::render_export_at(&root, 1., 0., true).unwrap();
        assert_eq!((visible.width(), visible.height()), (12, 12));
        assert_eq!(visible.pixel(2, 2).unwrap().alpha(), 255);
        let mut checks = 0;
        assert!(
            crate::compositor::render_export_at_cancellable(&root, 1., 0., true, &mut || {
                checks += 1;
                checks < 2
            })
            .unwrap_err()
            .contains("Cancelled")
        );
        root.motion.compositions[0].speed = 0.;
        assert!(validate(&root).unwrap_err().contains("Transform"));
        let mut root = child();
        for _ in 0..9 {
            let mut doc = child();
            doc.motion.compositions.push(Composition::new(root));
            root = doc;
        }
        assert!(validate(&root).unwrap_err().contains("Depth"));
    }
}
