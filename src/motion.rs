//! Motion clip: rest pose stays in the document, view pose is evaluated at t.
//!
//! Tracks are offsets from rest (X/Y/rotation) or absolute (scale, opacity).
//! Animated SVG is CSS @keyframes. Lottie is the Bodymovin 5.x shape subset.

use crate::color::Rgba;
use crate::document::{Cap, Document, Fill, Join, Shape, Stroke, Style};
use crate::geom::{Anchor, Geom, Pt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Prop {
    X,
    Y,
    Rotation,
    Scale,
    Opacity,
}

impl Prop {
    pub fn name(self) -> &'static str {
        match self {
            Prop::X => "X",
            Prop::Y => "Y",
            Prop::Rotation => "Rotate",
            Prop::Scale => "Scale",
            Prop::Opacity => "Opacity",
        }
    }

    pub fn identity(self) -> f32 {
        match self {
            Prop::Scale => 1.0,
            Prop::Opacity => 1.0,
            _ => 0.0,
        }
    }

    pub fn all() -> [Prop; 5] {
        [Prop::X, Prop::Y, Prop::Rotation, Prop::Scale, Prop::Opacity]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ease {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Ease {
    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "Linear",
            Ease::EaseIn => "In",
            Ease::EaseOut => "Out",
            Ease::EaseInOut => "In-Out",
        }
    }

    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::EaseIn => t * t,
            Ease::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Ease::EaseInOut => t * t * (3.0 - 2.0 * t),
        }
    }

    pub fn css(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::EaseIn => "ease-in",
            Ease::EaseOut => "ease-out",
            Ease::EaseInOut => "ease-in-out",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Ease::Linear => Ease::EaseIn,
            Ease::EaseIn => Ease::EaseOut,
            Ease::EaseOut => Ease::EaseInOut,
            Ease::EaseInOut => Ease::Linear,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub t: f32,
    pub value: f32,
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub shape: u64,
    pub prop: Prop,
    pub keys: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub duration: f32,
    pub fps: f32,
    pub looped: bool,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            duration: 2.0,
            fps: 30.0,
            looped: true,
            tracks: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub dx: f32,
    pub dy: f32,
    pub rotation: f32,
    pub scale: f32,
    pub opacity: Option<f32>,
}

impl Pose {
    pub fn identity() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            rotation: 0.0,
            scale: 1.0,
            opacity: None,
        }
    }

    pub fn is_identity(self) -> bool {
        self.dx.abs() < 1e-5
            && self.dy.abs() < 1e-5
            && self.rotation.abs() < 1e-5
            && (self.scale - 1.0).abs() < 1e-5
            && self.opacity.is_none()
    }

    pub fn map(self, center: Pt, p: Pt) -> Pt {
        let mut q = p - center;
        q = q * self.scale;
        q = q.rotate(self.rotation);
        q + center + Pt::new(self.dx, self.dy)
    }

    pub fn unmap(self, center: Pt, p: Pt) -> Pt {
        let mut q = p - Pt::new(self.dx, self.dy) - center;
        q = q.rotate(-self.rotation);
        if self.scale.abs() > 1e-6 {
            q = q / self.scale;
        }
        q + center
    }

    pub fn map_bounds(self, b: crate::geom::Bounds) -> crate::geom::Bounds {
        let c = b.center();
        let corners = [
            Pt::new(b.min.x, b.min.y),
            Pt::new(b.max.x, b.min.y),
            Pt::new(b.max.x, b.max.y),
            Pt::new(b.min.x, b.max.y),
        ];
        let mut out = crate::geom::Bounds::from_pt(self.map(c, corners[0]));
        for p in corners.iter().skip(1) {
            out.union_pt(self.map(c, *p));
        }
        out
    }

    pub fn to_skia(self, center: Pt) -> tiny_skia::Transform {
        use tiny_skia::Transform;
        if self.is_identity() {
            return Transform::identity();
        }
        let deg = self.rotation.to_degrees();
        let mut t = Transform::from_translate(-center.x, -center.y);
        t = Transform::from_scale(self.scale, self.scale).pre_concat(t);
        if deg.abs() > 1e-5 {
            t = Transform::from_rotate(deg).pre_concat(t);
        }
        Transform::from_translate(center.x + self.dx, center.y + self.dy).pre_concat(t)
    }
}

impl Motion {
    pub fn is_empty(&self) -> bool {
        self.tracks.iter().all(|t| t.keys.is_empty())
    }

    pub fn value(&self, shape: u64, prop: Prop, t: f32) -> Option<f32> {
        let track = self
            .tracks
            .iter()
            .find(|tr| tr.shape == shape && tr.prop == prop)?;
        eval_keys(&track.keys, t, self.duration)
    }

    pub fn pose(&self, shape: u64, t: f32) -> Pose {
        Pose {
            dx: self.value(shape, Prop::X, t).unwrap_or(0.0),
            dy: self.value(shape, Prop::Y, t).unwrap_or(0.0),
            rotation: self.value(shape, Prop::Rotation, t).unwrap_or(0.0),
            scale: self.value(shape, Prop::Scale, t).unwrap_or(1.0),
            opacity: self.value(shape, Prop::Opacity, t),
        }
    }

    pub fn set_key(&mut self, shape: u64, prop: Prop, t: f32, value: f32, ease: Ease) {
        let duration = self.duration.max(0.05);
        let t = t.clamp(0.0, duration);
        let idx = self.ensure_track(shape, prop);
        let keys = &mut self.tracks[idx].keys;
        if keys.is_empty() && t > 1e-3 {
            keys.push(Key {
                t: 0.0,
                value: prop.identity(),
                ease,
            });
        }
        if let Some(k) = keys.iter_mut().find(|k| (k.t - t).abs() < 1.0 / 120.0) {
            k.value = value;
            k.ease = ease;
        } else {
            keys.push(Key { t, value, ease });
            keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    pub fn remove_key(&mut self, shape: u64, prop: Prop, index: usize) {
        if let Some(track) = self
            .tracks
            .iter_mut()
            .find(|tr| tr.shape == shape && tr.prop == prop)
            && index < track.keys.len()
        {
            track.keys.remove(index);
        }
        self.tracks.retain(|tr| !tr.keys.is_empty());
    }

    pub fn drop_shape(&mut self, id: u64) {
        self.tracks.retain(|tr| tr.shape != id);
    }

    pub fn drop_shapes(&mut self, ids: &[u64]) {
        self.tracks.retain(|tr| !ids.contains(&tr.shape));
    }

    pub fn shapes(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.tracks.iter().map(|t| t.shape).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn key_times(&self, shape: u64) -> Vec<f32> {
        let mut ts: Vec<f32> = self
            .tracks
            .iter()
            .filter(|tr| tr.shape == shape)
            .flat_map(|tr| tr.keys.iter().map(|k| k.t))
            .collect();
        ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ts.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        ts
    }

    fn ensure_track(&mut self, shape: u64, prop: Prop) -> usize {
        if let Some(i) = self
            .tracks
            .iter()
            .position(|tr| tr.shape == shape && tr.prop == prop)
        {
            i
        } else {
            self.tracks.push(Track {
                shape,
                prop,
                keys: vec![],
            });
            self.tracks.len() - 1
        }
    }

    pub fn css_keyframes(&self, shape: u64, name: &str) -> Option<String> {
        let times = self.key_times(shape);
        if times.is_empty() {
            return None;
        }
        let dur = self.duration.max(0.05);
        let mut css = format!("@keyframes {name} {{\n");
        for t in times {
            let pose = self.pose(shape, t);
            let pct = (t / dur * 100.0).clamp(0.0, 100.0);
            let ease = self
                .tracks
                .iter()
                .filter(|tr| tr.shape == shape)
                .flat_map(|tr| tr.keys.iter())
                .find(|k| (k.t - t).abs() < 1e-4)
                .map(|k| k.ease.css())
                .unwrap_or("ease-in-out");
            let op = pose.opacity.unwrap_or(1.0);
            css.push_str(&format!(
                "  {pct:.3}% {{ transform: translate({:.3}px, {:.3}px) rotate({:.3}deg) scale({:.4}); opacity: {:.3}; animation-timing-function: {ease}; }}\n",
                pose.dx,
                pose.dy,
                pose.rotation.to_degrees(),
                pose.scale,
                op
            ));
        }
        css.push_str("}\n");
        Some(css)
    }
}

fn eval_keys(keys: &[Key], t: f32, duration: f32) -> Option<f32> {
    if keys.is_empty() {
        return None;
    }
    let t = t.clamp(0.0, duration.max(keys.last().map(|k| k.t).unwrap_or(0.0)));
    if t <= keys[0].t {
        return Some(keys[0].value);
    }
    let last = keys.last().unwrap();
    if t >= last.t {
        return Some(last.value);
    }
    for w in keys.windows(2) {
        if t >= w[0].t && t <= w[1].t {
            let span = (w[1].t - w[0].t).max(1e-9);
            let u = w[0].ease.apply((t - w[0].t) / span);
            return Some(w[0].value + (w[1].value - w[0].value) * u);
        }
    }
    Some(last.value)
}

pub fn hit_test(
    doc: &Document,
    t: f32,
    overrides: &HashMap<u64, Pose>,
    p: Pt,
    slack: f32,
) -> Option<(usize, u64)> {
    for (li, layer) in doc.layers.iter().enumerate().rev() {
        if !layer.visible || layer.locked {
            continue;
        }
        if let Some(shapes) = layer.kind.shapes() {
            for shape in shapes.iter().rev() {
                let pose = overrides
                    .get(&shape.id)
                    .copied()
                    .unwrap_or_else(|| doc.motion.pose(shape.id, t));
                let c = shape.world_bbox().center();
                let q = pose.unmap(c, p);
                let s = slack / pose.scale.max(0.05);
                if shape.contains_world(q) || shape.dist_world(q) <= s {
                    return Some((li, shape.id));
                }
            }
        }
    }
    None
}

pub fn hits_in_rect(
    doc: &Document,
    t: f32,
    overrides: &HashMap<u64, Pose>,
    r: crate::geom::Bounds,
) -> Vec<(usize, u64)> {
    let mut out = vec![];
    for (li, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || layer.locked {
            continue;
        }
        if let Some(shapes) = layer.kind.shapes() {
            for shape in shapes {
                let pose = overrides
                    .get(&shape.id)
                    .copied()
                    .unwrap_or_else(|| doc.motion.pose(shape.id, t));
                let b = pose.map_bounds(shape.world_bbox());
                if b.intersects(r) {
                    out.push((li, shape.id));
                }
            }
        }
    }
    out
}

/// Lottie 5.x JSON. Shape layers only. Playable in lottie-web / dotLottie.
pub fn export_lottie(doc: &Document) -> Result<String, String> {
    let fps = doc.motion.fps.clamp(1.0, 120.0);
    let op = (doc.motion.duration * fps).round().max(1.0);
    let mut layers = Vec::new();
    let mut ind = 1i32;
    for layer in doc.layers.iter().rev() {
        if !layer.visible {
            continue;
        }
        let Some(shapes) = layer.kind.shapes() else {
            continue;
        };
        for shape in shapes.iter().rev() {
            if let Some(lottie_layer) = shape_layer(shape, &doc.motion, fps, op, ind) {
                layers.push(lottie_layer);
                ind += 1;
            }
        }
    }
    let v = json!({
        "v": "5.7.4",
        "fr": fps,
        "ip": 0,
        "op": op,
        "w": doc.width.round() as i32,
        "h": doc.height.round() as i32,
        "nm": doc.name,
        "ddd": 0,
        "assets": [],
        "layers": layers,
    });
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

fn shape_layer(shape: &Shape, motion: &Motion, fps: f32, op: f32, ind: i32) -> Option<Value> {
    let b = shape.world_bbox();
    let c = b.center();
    let (path_item, closed) = lottie_path(shape, c)?;
    let mut items = vec![path_item];
    match &shape.style.fill {
        Fill::None => {}
        Fill::Solid(col) => items.push(lottie_fill(*col, shape.opacity)),
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => {
            items.push(lottie_fill(*c0, shape.opacity))
        }
    }
    if let Some(st) = &shape.style.stroke
        && st.width > 0.0
    {
        items.push(lottie_stroke(st, shape.opacity));
    }
    items.push(json!({
        "ty": "tr",
        "p": {"a": 0, "k": [0, 0]},
        "a": {"a": 0, "k": [0, 0]},
        "s": {"a": 0, "k": [100, 100]},
        "r": {"a": 0, "k": 0},
        "o": {"a": 0, "k": 100},
        "sk": {"a": 0, "k": 0},
        "sa": {"a": 0, "k": 0},
    }));
    Some(json!({
        "ddd": 0,
        "ind": ind,
        "ty": 4,
        "nm": shape.name,
        "sr": 1,
        "ks": lottie_transform(shape, motion, fps, c),
        "ao": 0,
        "shapes": [{
            "ty": "gr",
            "nm": shape.name,
            "it": items,
            "np": items.len(),
            "cix": 2,
            "bm": 0,
            "ix": 1,
            "mn": "ADBE Vector Group",
            "hd": false
        }],
        "ip": 0,
        "op": op,
        "st": 0,
        "bm": 0,
        "hasMask": false,
        "closed": closed,
    }))
}

fn lottie_path(shape: &Shape, center: Pt) -> Option<(Value, bool)> {
    if let Geom::Path { anchors, closed } = &shape.geom {
        if anchors.len() < 2 {
            return None;
        }
        let mut v = Vec::new();
        let mut i = Vec::new();
        let mut o = Vec::new();
        for a in anchors {
            let p = if shape.rotation.abs() > 1e-5 {
                a.pt.rotate_about(center, shape.rotation)
            } else {
                a.pt
            };
            v.push(json!([p.x - center.x, p.y - center.y]));
            i.push(json!([a.h_in.x, a.h_in.y]));
            o.push(json!([a.h_out.x, a.h_out.y]));
        }
        return Some((
            json!({
                "ty": "sh",
                "nm": "Path",
                "ks": {"a": 0, "k": {"c": closed, "v": v, "i": i, "o": o}},
                "hd": false
            }),
            *closed,
        ));
    }
    let contours = shape.world_contours(64);
    let contour = contours.iter().find(|c| c.len() >= 2)?;
    let closed = shape.geom.is_closed();
    let mut v = Vec::new();
    let mut i = Vec::new();
    let mut o = Vec::new();
    let n = if closed && contour.len() > 1 && (contour[0] - *contour.last().unwrap()).length() < 0.5
    {
        contour.len() - 1
    } else {
        contour.len()
    };
    for p in contour.iter().take(n) {
        v.push(json!([p.x - center.x, p.y - center.y]));
        i.push(json!([0, 0]));
        o.push(json!([0, 0]));
    }
    Some((
        json!({
            "ty": "sh",
            "nm": "Path",
            "ks": {"a": 0, "k": {"c": closed, "v": v, "i": i, "o": o}},
            "hd": false
        }),
        closed,
    ))
}

fn lottie_fill(c: Rgba, opacity: f32) -> Value {
    json!({
        "ty": "fl",
        "nm": "Fill",
        "c": {"a": 0, "k": [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, 1.0]},
        "o": {"a": 0, "k": (c.a as f32 / 255.0) * opacity * 100.0},
        "r": 1,
        "bm": 0,
        "hd": false
    })
}

fn lottie_stroke(st: &Stroke, opacity: f32) -> Value {
    let cap = match st.cap {
        Cap::Butt => 1,
        Cap::Round => 2,
        Cap::Square => 3,
    };
    let join = match st.join {
        Join::Miter => 1,
        Join::Round => 2,
        Join::Bevel => 3,
    };
    json!({
        "ty": "st",
        "nm": "Stroke",
        "c": {"a": 0, "k": [
            st.color.r as f32 / 255.0,
            st.color.g as f32 / 255.0,
            st.color.b as f32 / 255.0,
            1.0
        ]},
        "o": {"a": 0, "k": (st.color.a as f32 / 255.0) * opacity * 100.0},
        "w": {"a": 0, "k": st.width},
        "lc": cap,
        "lj": join,
        "ml": 4,
        "bm": 0,
        "hd": false
    })
}

fn lottie_transform(shape: &Shape, motion: &Motion, fps: f32, center: Pt) -> Value {
    let times = {
        let mut ts = motion.key_times(shape.id);
        if ts.is_empty() {
            ts.push(0.0);
        }
        ts
    };
    let animated = !motion
        .tracks
        .iter()
        .filter(|tr| tr.shape == shape.id)
        .all(|tr| tr.keys.len() <= 1);
    let pos = lottie_vec2_anim(
        animated && (motion.value(shape.id, Prop::X, 0.0).is_some()
            || motion.value(shape.id, Prop::Y, 0.0).is_some()
            || times.len() > 1),
        &times,
        fps,
        |t| {
            let p = motion.pose(shape.id, t);
            [center.x + p.dx, center.y + p.dy]
        },
        motion,
        shape.id,
        Prop::X,
    );
    let sc = lottie_vec2_anim(
        motion.value(shape.id, Prop::Scale, 0.0).is_some(),
        &times,
        fps,
        |t| {
            let s = motion.pose(shape.id, t).scale * 100.0;
            [s, s]
        },
        motion,
        shape.id,
        Prop::Scale,
    );
    let rot = lottie_scalar_anim(
        motion.value(shape.id, Prop::Rotation, 0.0).is_some(),
        &times,
        fps,
        |t| motion.pose(shape.id, t).rotation.to_degrees(),
        motion,
        shape.id,
        Prop::Rotation,
    );
    let op = lottie_scalar_anim(
        motion.value(shape.id, Prop::Opacity, 0.0).is_some(),
        &times,
        fps,
        |t| {
            motion
                .pose(shape.id, t)
                .opacity
                .unwrap_or(shape.opacity)
                * 100.0
        },
        motion,
        shape.id,
        Prop::Opacity,
    );
    json!({
        "o": op,
        "r": rot,
        "p": pos,
        "a": {"a": 0, "k": [0, 0, 0]},
        "s": sc
    })
}

fn lottie_vec2_anim(
    animated: bool,
    times: &[f32],
    fps: f32,
    sample: impl Fn(f32) -> [f32; 2],
    motion: &Motion,
    shape: u64,
    prop: Prop,
) -> Value {
    if !animated || times.len() < 2 {
        let v = sample(times.first().copied().unwrap_or(0.0));
        return json!({"a": 0, "k": [v[0], v[1], 0]});
    }
    let mut keys = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let v = sample(*t);
        let ease = motion
            .tracks
            .iter()
            .find(|tr| tr.shape == shape && tr.prop == prop)
            .and_then(|tr| tr.keys.iter().find(|k| (k.t - *t).abs() < 1e-4))
            .map(|k| k.ease)
            .unwrap_or(Ease::EaseInOut);
        let (ix, iy, ox, oy) = lottie_ease(ease);
        let mut kf = json!({
            "t": (*t * fps),
            "s": [v[0], v[1], 0],
            "i": {"x": [ix], "y": [iy]},
            "o": {"x": [ox], "y": [oy]},
        });
        if i + 1 < times.len() {
            let e = sample(times[i + 1]);
            kf["e"] = json!([e[0], e[1], 0]);
        }
        keys.push(kf);
    }
    json!({"a": 1, "k": keys})
}

fn lottie_scalar_anim(
    animated: bool,
    times: &[f32],
    fps: f32,
    sample: impl Fn(f32) -> f32,
    motion: &Motion,
    shape: u64,
    prop: Prop,
) -> Value {
    if !animated || times.len() < 2 {
        let v = sample(times.first().copied().unwrap_or(0.0));
        return json!({"a": 0, "k": v});
    }
    let mut keys = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let v = sample(*t);
        let ease = motion
            .tracks
            .iter()
            .find(|tr| tr.shape == shape && tr.prop == prop)
            .and_then(|tr| tr.keys.iter().find(|k| (k.t - *t).abs() < 1e-4))
            .map(|k| k.ease)
            .unwrap_or(Ease::EaseInOut);
        let (ix, iy, ox, oy) = lottie_ease(ease);
        let mut kf = json!({
            "t": (*t * fps),
            "s": [v],
            "i": {"x": [ix], "y": [iy]},
            "o": {"x": [ox], "y": [oy]},
        });
        if i + 1 < times.len() {
            kf["e"] = json!([sample(times[i + 1])]);
        }
        keys.push(kf);
    }
    json!({"a": 1, "k": keys})
}

fn lottie_ease(ease: Ease) -> (f32, f32, f32, f32) {
    match ease {
        Ease::Linear => (0.167, 0.167, 0.833, 0.833),
        Ease::EaseIn => (0.4, 0.0, 1.0, 1.0),
        Ease::EaseOut => (0.0, 0.0, 0.2, 1.0),
        Ease::EaseInOut => (0.667, 1.0, 0.333, 0.0),
    }
}

pub struct LottieImport {
    pub width: f32,
    pub height: f32,
    pub motion: Motion,
    pub shapes: Vec<Shape>,
}

/// Import a Lottie 5.x JSON. Shape layers (ty=4) with path/ellipse/rect.
pub fn import_lottie(json: &str) -> Result<LottieImport, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let w = js_f32(&v["w"]).unwrap_or(1280.0);
    let h = js_f32(&v["h"]).unwrap_or(800.0);
    let fps = js_f32(&v["fr"]).unwrap_or(30.0).clamp(1.0, 120.0);
    let op = js_f32(&v["op"]).unwrap_or(fps * 2.0);
    let ip = js_f32(&v["ip"]).unwrap_or(0.0);
    let duration = ((op - ip) / fps).max(0.1);
    let mut motion = Motion {
        duration,
        fps,
        looped: true,
        tracks: vec![],
    };
    let mut shapes = Vec::new();
    let layers = v["layers"].as_array().cloned().unwrap_or_default();
    for layer in layers.iter().rev() {
        let ty = js_f32(&layer["ty"]).unwrap_or(-1.0) as i32;
        if ty != 4 {
            continue;
        }
        let Some((geom, fill, stroke)) = layer_geom(layer) else {
            continue;
        };
        let mut shape = Shape::new(
            geom,
            Style {
                fill,
                stroke,
            },
        );
        if let Some(nm) = layer["nm"].as_str() {
            shape.name = nm.to_string();
        }
        let ks = &layer["ks"];
        let rest = parse_anim_vec2(&ks["p"])
            .first()
            .map(|(_, p)| *p)
            .or_else(|| js_vec2(&ks["p"]))
            .unwrap_or(Pt::ZERO);
        shape.geom.translate(rest);
        apply_lottie_transform(&mut motion, shape.id, ks, fps, ip, rest, shape.opacity);
        shapes.push(shape);
    }
    if shapes.is_empty() {
        return Err("no shape layers in this Lottie".into());
    }
    Ok(LottieImport {
        width: w,
        height: h,
        motion,
        shapes,
    })
}

fn apply_lottie_transform(
    motion: &mut Motion,
    id: u64,
    ks: &Value,
    fps: f32,
    ip: f32,
    rest: Pt,
    rest_opacity: f32,
) {
    let pos = parse_anim_vec2(&ks["p"]);
    if pos.len() > 1 {
        for (t, p) in &pos {
            motion.set_key(
                id,
                Prop::X,
                frame_to_t(*t, fps, ip),
                p.x - rest.x,
                Ease::EaseInOut,
            );
            motion.set_key(
                id,
                Prop::Y,
                frame_to_t(*t, fps, ip),
                p.y - rest.y,
                Ease::EaseInOut,
            );
        }
    }
    let rot = parse_anim_scalar(&ks["r"]);
    if rot.len() > 1 || rot.first().is_some_and(|(_, v)| v.abs() > 1e-3) {
        for (t, v) in &rot {
            motion.set_key(
                id,
                Prop::Rotation,
                frame_to_t(*t, fps, ip),
                v.to_radians(),
                Ease::EaseInOut,
            );
        }
    }
    let sc = parse_anim_vec2(&ks["s"]);
    if sc.len() > 1 || sc.first().is_some_and(|(_, p)| (p.x - 100.0).abs() > 0.5) {
        for (t, p) in &sc {
            motion.set_key(
                id,
                Prop::Scale,
                frame_to_t(*t, fps, ip),
                (p.x / 100.0).max(0.01),
                Ease::EaseInOut,
            );
        }
    }
    let op = parse_anim_scalar(&ks["o"]);
    if op.len() > 1 || op.first().is_some_and(|(_, v)| (*v / 100.0 - rest_opacity).abs() > 0.01) {
        for (t, v) in &op {
            motion.set_key(
                id,
                Prop::Opacity,
                frame_to_t(*t, fps, ip),
                (*v / 100.0).clamp(0.0, 1.0),
                Ease::EaseInOut,
            );
        }
    }
}

fn frame_to_t(frame: f32, fps: f32, ip: f32) -> f32 {
    ((frame - ip) / fps.max(1.0)).max(0.0)
}

fn layer_geom(layer: &Value) -> Option<(Geom, Fill, Option<Stroke>)> {
    let shapes = layer["shapes"].as_array()?;
    let mut geom = None;
    let mut fill = Fill::Solid(Rgba::from_hex(0x4F8CFF));
    let mut stroke = None;
    fn walk(items: &[Value], geom: &mut Option<Geom>, fill: &mut Fill, stroke: &mut Option<Stroke>) {
        for it in items {
            match it["ty"].as_str().unwrap_or("") {
                "gr" => {
                    if let Some(inner) = it["it"].as_array() {
                        walk(inner, geom, fill, stroke);
                    }
                }
                "sh" => {
                    if geom.is_none() {
                        *geom = lottie_path_to_geom(&it["ks"]);
                    }
                }
                "el" => {
                    if geom.is_none() {
                        let p = js_vec2(&it["p"]).unwrap_or(Pt::ZERO);
                        let s = js_vec2(&it["s"]).unwrap_or(Pt::new(40.0, 40.0));
                        *geom = Some(Geom::Ellipse {
                            center: p,
                            radii: Pt::new(s.x.abs() * 0.5, s.y.abs() * 0.5),
                        });
                    }
                }
                "rc" => {
                    if geom.is_none() {
                        let p = js_vec2(&it["p"]).unwrap_or(Pt::ZERO);
                        let s = js_vec2(&it["s"]).unwrap_or(Pt::new(40.0, 40.0));
                        let r = js_f32(&it["r"]["k"]).or_else(|| js_f32(&it["r"])).unwrap_or(0.0);
                        *geom = Some(Geom::Rect {
                            origin: Pt::new(p.x - s.x.abs() * 0.5, p.y - s.y.abs() * 0.5),
                            size: Pt::new(s.x.abs(), s.y.abs()),
                            radius: r,
                        });
                    }
                }
                "fl" => {
                    if let Some(c) = js_color(&it["c"]) {
                        let a = js_f32(&it["o"]["k"]).unwrap_or(100.0) / 100.0;
                        *fill = Fill::Solid(Rgba::new(
                            c.r,
                            c.g,
                            c.b,
                            (a * 255.0).round() as u8,
                        ));
                    }
                }
                "st" => {
                    if let Some(c) = js_color(&it["c"]) {
                        let w = js_f32(&it["w"]["k"]).or_else(|| js_f32(&it["w"])).unwrap_or(2.0);
                        let a = js_f32(&it["o"]["k"]).unwrap_or(100.0) / 100.0;
                        *stroke = Some(Stroke {
                            color: Rgba::new(c.r, c.g, c.b, (a * 255.0).round() as u8),
                            width: w,
                            cap: Cap::Round,
                            join: Join::Round,
                            dash: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    walk(shapes, &mut geom, &mut fill, &mut stroke);
    Some((geom?, fill, stroke))
}

fn lottie_path_to_geom(ks: &Value) -> Option<Geom> {
    let k = if ks["a"].as_i64() == Some(1) {
        ks["k"].as_array()?.first()?
    } else {
        &ks["k"]
    };
    let data = if k.get("s").is_some() { &k["s"] } else { k };
    let vs = data["v"].as_array()?;
    let ins = data["i"].as_array();
    let outs = data["o"].as_array();
    let closed = data["c"].as_bool().unwrap_or(true);
    let mut anchors = Vec::new();
    for (n, v) in vs.iter().enumerate() {
        let pt = js_pt(v)?;
        let h_in = ins
            .and_then(|a| a.get(n))
            .and_then(js_pt)
            .unwrap_or(Pt::ZERO);
        let h_out = outs
            .and_then(|a| a.get(n))
            .and_then(js_pt)
            .unwrap_or(Pt::ZERO);
        anchors.push(Anchor { pt, h_in, h_out });
    }
    if anchors.len() < 2 {
        return None;
    }
    Some(Geom::Path { anchors, closed })
}

fn parse_anim_vec2(v: &Value) -> Vec<(f32, Pt)> {
    if v.is_null() {
        return vec![];
    }
    if v["a"].as_i64() == Some(1) || v["k"].as_array().is_some_and(|a| a.first().is_some_and(|x| x.get("t").is_some())) {
        let mut out = vec![];
        if let Some(arr) = v["k"].as_array() {
            for kf in arr {
                let t = js_f32(&kf["t"]).unwrap_or(0.0);
                if let Some(p) = js_vec_from(&kf["s"]).or_else(|| js_vec2(kf)) {
                    out.push((t, p));
                }
            }
        }
        return out;
    }
    if let Some(p) = js_vec2(&v["k"]).or_else(|| js_vec2(v)) {
        return vec![(0.0, p)];
    }
    vec![]
}

fn parse_anim_scalar(v: &Value) -> Vec<(f32, f32)> {
    if v.is_null() {
        return vec![];
    }
    if v["a"].as_i64() == Some(1) || v["k"].as_array().is_some_and(|a| a.first().is_some_and(|x| x.get("t").is_some())) {
        let mut out = vec![];
        if let Some(arr) = v["k"].as_array() {
            for kf in arr {
                let t = js_f32(&kf["t"]).unwrap_or(0.0);
                if let Some(s) = js_f32(&kf["s"]).or_else(|| {
                    kf["s"].as_array().and_then(|a| a.first()).and_then(js_f32)
                }) {
                    out.push((t, s));
                }
            }
        }
        return out;
    }
    if let Some(s) = js_f32(&v["k"]).or_else(|| {
        v["k"].as_array().and_then(|a| a.first()).and_then(js_f32)
    }) {
        return vec![(0.0, s)];
    }
    vec![]
}

fn js_f32(v: &Value) -> Option<f32> {
    v.as_f64()
        .map(|x| x as f32)
        .or_else(|| v.as_i64().map(|x| x as f32))
        .or_else(|| v.as_u64().map(|x| x as f32))
        .or_else(|| v.as_array().and_then(|a| a.first()).and_then(js_f32))
}

fn js_pt(v: &Value) -> Option<Pt> {
    let a = v.as_array()?;
    Some(Pt::new(js_f32(&a[0])?, js_f32(a.get(1)?)?))
}

fn js_vec2(v: &Value) -> Option<Pt> {
    if let Some(p) = js_pt(v) {
        return Some(p);
    }
    js_pt(&v["k"]).or_else(|| {
        v["k"].as_array().and_then(|a| {
            if a.first().is_some_and(|x| x.get("t").is_some()) {
                a.first().and_then(|kf| js_pt(&kf["s"]))
            } else {
                js_pt(&Value::Array(a.clone()))
            }
        })
    })
}

fn js_vec_from(v: &Value) -> Option<Pt> {
    js_pt(v)
}

fn js_color(v: &Value) -> Option<Rgba> {
    let a = if let Some(arr) = v["k"].as_array() {
        if arr.first().is_some_and(|x| x.get("t").is_some()) {
            arr.first()?.get("s")?.as_array()?
        } else {
            arr
        }
    } else {
        v.as_array()?
    };
    let r = js_f32(&a[0])?;
    let g = js_f32(&a[1])?;
    let b = js_f32(&a[2])?;
    let scale = if r > 1.0 || g > 1.0 || b > 1.0 { 1.0 } else { 255.0 };
    Some(Rgba::rgb(
        (r * scale).round().clamp(0.0, 255.0) as u8,
        (g * scale).round().clamp(0.0, 255.0) as u8,
        (b * scale).round().clamp(0.0, 255.0) as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, apply};

    #[test]
    fn ease_endpoints() {
        for e in [Ease::Linear, Ease::EaseIn, Ease::EaseOut, Ease::EaseInOut] {
            assert!((e.apply(0.0) - 0.0).abs() < 1e-6);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6);
        }
        assert!(Ease::EaseIn.apply(0.5) < 0.5);
        assert!(Ease::EaseOut.apply(0.5) > 0.5);
    }

    #[test]
    fn pose_eval_lerps() {
        let mut m = Motion::default();
        m.set_key(1, Prop::X, 0.0, 0.0, Ease::Linear);
        m.set_key(1, Prop::X, 2.0, 100.0, Ease::Linear);
        let p = m.pose(1, 1.0);
        assert!((p.dx - 50.0).abs() < 0.01);
        assert!((p.scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn first_key_inserts_rest_at_zero() {
        let mut m = Motion::default();
        m.set_key(7, Prop::Y, 1.0, 40.0, Ease::EaseInOut);
        let y0 = m.value(7, Prop::Y, 0.0).unwrap();
        let y1 = m.value(7, Prop::Y, 1.0).unwrap();
        assert!((y0 - 0.0).abs() < 1e-5);
        assert!((y1 - 40.0).abs() < 1e-5);
    }

    #[test]
    fn pose_map_roundtrip() {
        let pose = Pose {
            dx: 12.0,
            dy: -8.0,
            rotation: 0.4,
            scale: 1.25,
            opacity: None,
        };
        let c = Pt::new(100.0, 80.0);
        let p = Pt::new(130.0, 60.0);
        let q = pose.unmap(c, pose.map(c, p));
        assert!((q.x - p.x).abs() < 0.02);
        assert!((q.y - p.y).abs() < 0.02);
    }

    #[test]
    fn lottie_roundtrip_keeps_motion() {
        let mut doc = Document::new("clip", 400.0, 300.0, 72.0);
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(40.0, 40.0),
                size: Pt::new(80.0, 50.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = shape.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape,
            },
        );
        doc.motion.set_key(id, Prop::X, 0.0, 0.0, Ease::Linear);
        doc.motion.set_key(id, Prop::X, 2.0, 60.0, Ease::Linear);
        doc.motion.set_key(id, Prop::Rotation, 0.0, 0.0, Ease::EaseInOut);
        doc.motion.set_key(id, Prop::Rotation, 2.0, 0.5, Ease::EaseInOut);
        let json = export_lottie(&doc).unwrap();
        assert!(json.contains("\"ty\": 4"));
        let imported = import_lottie(&json).unwrap();
        assert!(!imported.shapes.is_empty());
        assert!((imported.motion.duration - 2.0).abs() < 0.05);
        let nid = imported.shapes[0].id;
        let p = imported.motion.pose(nid, 2.0);
        assert!(p.dx.abs() > 40.0, "expected travel, got dx={}", p.dx);
        assert!(p.rotation.abs() > 0.3, "expected rotation, got {}", p.rotation);
    }

    #[test]
    fn animated_svg_has_keyframes() {
        let mut doc = Document::new("clip", 200.0, 200.0, 72.0);
        let shape = Shape::new(
            Geom::Ellipse {
                center: Pt::new(80.0, 80.0),
                radii: Pt::new(30.0, 30.0),
            },
            Style::default(),
        );
        let id = shape.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape,
            },
        );
        doc.motion.set_key(id, Prop::Scale, 0.0, 1.0, Ease::EaseInOut);
        doc.motion.set_key(id, Prop::Scale, 2.0, 1.4, Ease::EaseInOut);
        let svg = crate::svg::export_animated(&doc).unwrap();
        assert!(svg.contains("@keyframes"));
        assert!(svg.contains("oma-"));
        assert!(svg.contains("animation-name"));
    }
}
