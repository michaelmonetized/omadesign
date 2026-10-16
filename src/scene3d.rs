//! Editable 3D model composition shared by the canvas and image/animation export.
pub mod model;
pub mod render;

use model::{Mat4, Model, Vec3};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Object {
    pub id: u64,
    pub name: String,
    pub model: Arc<Model>,
    pub translation: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
    pub animation: Option<usize>,
    pub time_offset: f64,
    pub time_scale: f64,
    pub looped: bool,
    pub opacity: f32,
    pub visible: bool,
}
impl Default for Object {
    fn default() -> Self {
        Self {
            id: crate::document::next_id(),
            name: "Model".into(),
            model: Arc::new(Model::default()),
            translation: [0.; 3],
            rotation: [0.; 3],
            scale: [1.; 3],
            animation: None,
            time_offset: 0.,
            time_scale: 1.,
            looped: true,
            opacity: 1.,
            visible: true,
        }
    }
}
impl Object {
    /// Build the scene transform.
    /// Args: stored translation, rotation in degrees and scale. Returns model-to-scene matrix.
    pub fn transform(&self) -> Mat4 {
        Mat4::translate(self.translation.into())
            * Mat4::rotate_z(self.rotation[2])
            * Mat4::rotate_y(self.rotation[1])
            * Mat4::rotate_x(self.rotation[0])
            * Mat4::scale(self.scale.into())
    }
    /// Sample the selected model animation clock.
    /// Args: composition time in seconds. Returns model clip seconds, looped when requested.
    pub fn local_time(&self, time: f64) -> f64 {
        let t = self.time_offset + time * self.time_scale;
        let d = self
            .animation
            .and_then(|i| self.model.animations.get(i))
            .map_or(0., |a| a.duration());
        if self.looped && d > 0. {
            t.rem_euclid(d)
        } else {
            t
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    Perspective { fov: f64 },
    Orthographic { height: f64 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub up: [f64; 3],
    pub projection: Projection,
    pub near: f64,
    pub far: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0., 0., 5.],
            target: [0.; 3],
            up: [0., 1., 0.],
            projection: Projection::Perspective { fov: 45. },
            near: 0.01,
            far: 10000.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Point,
    Spot { inner: f64, outer: f64 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
    pub range: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub objects: Vec<Object>,
    pub camera: Camera,
    pub imported_camera: Option<(u64, usize)>,
    pub lights: Vec<Light>,
    pub ambient: [f64; 3],
    pub imported_lights: bool,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            objects: vec![],
            camera: Camera::default(),
            imported_camera: None,
            lights: vec![Light {
                kind: LightKind::Directional,
                position: [0.; 3],
                direction: [-0.4, -0.6, -1.],
                color: [1.; 3],
                intensity: 2.,
                range: None,
            }],
            ambient: [0.12; 3],
            imported_lights: true,
        }
    }
}
impl Scene {
    /// Add a fully embedded model and fit the first camera.
    /// Args: editable model and display name. Returns persistent scene object id or validation error.
    pub fn add(&mut self, model: Model, name: String) -> Result<u64, String> {
        model.validate().map_err(|e| e.to_string())?;
        if self.objects.len() >= 32 {
            return Err("SceneLimit: at most 32 model objects".into());
        }
        let id = crate::document::next_id();
        let animation = (!model.animations.is_empty()).then_some(0);
        if self.objects.is_empty() {
            if let Some((lo, hi)) = model.bounds() {
                let lo = Vec3::from(lo);
                let hi = Vec3::from(hi);
                let center = (lo + hi) * 0.5;
                let radius = (hi - lo).length().max(0.01) * 0.5;
                self.camera.target = [center.x, center.y, center.z];
                self.camera.position = [
                    center.x,
                    center.y,
                    center.z + radius / (22.5_f64.to_radians().sin()) * 1.2,
                ];
                self.camera.near = (radius * 0.001).max(0.00001);
                self.camera.far = radius * 1000. + self.camera.position[2].abs();
            }
            if let Some(camera) = model.placed(false, animation, 0.).first() {
                self.imported_camera = Some((id, camera.node));
            }
            if !model.lights.is_empty() {
                self.lights.clear();
            }
        }
        self.objects.push(Object {
            id,
            name,
            model: Arc::new(model),
            animation,
            ..Object::default()
        });
        Ok(id)
    }
    /// Validate editable scene state before use.
    /// Args: scene model and controls. Returns a named error for invalid or oversized data.
    pub fn validate(&self) -> Result<(), String> {
        if self.objects.len() > 32 || self.lights.len() > 64 {
            return Err("SceneLimit: too many objects or lights".into());
        }
        let finite = |v: &[f64]| v.iter().all(|x| x.is_finite() && x.abs() <= 1e12);
        let mut ids = std::collections::HashSet::new();
        for object in &self.objects {
            if object.id == 0 || object.id >= u64::MAX - 200_000 || !ids.insert(object.id) {
                return Err("SceneId: duplicate model id".into());
            }
            object.model.validate().map_err(|e| e.to_string())?;
            if !finite(&object.translation)
                || !finite(&object.rotation)
                || !finite(&object.scale)
                || object.scale.contains(&0.)
                || !object.opacity.is_finite()
                || !(0.0..=1.).contains(&object.opacity)
                || !finite(&[object.time_scale, object.time_offset])
                || object
                    .animation
                    .is_some_and(|i| i >= object.model.animations.len())
            {
                return Err("SceneTransform: invalid transform, opacity or clip".into());
            }
        }
        let c = &self.camera;
        if !finite(&c.position)
            || !finite(&c.target)
            || !finite(&c.up)
            || !finite(&[c.near, c.far])
            || c.near <= 0.
            || c.far <= c.near
            || (Vec3::from(c.target) - Vec3::from(c.position))
                .cross(c.up.into())
                .length()
                < 1e-10
        {
            return Err("SceneCamera: invalid camera or clipping range".into());
        }
        match c.projection {
            Projection::Perspective { fov } if !fov.is_finite() || !(1.0..179.).contains(&fov) => {
                return Err("SceneCamera: field of view must be 1–179 degrees".into());
            }
            Projection::Orthographic { height } if !height.is_finite() || height <= 0. => {
                return Err("SceneCamera: orthographic height must be positive".into());
            }
            _ => {}
        }
        if self
            .ambient
            .iter()
            .any(|x| !x.is_finite() || *x < 0. || *x > 1000.)
        {
            return Err("SceneLight: invalid ambient light".into());
        }
        for l in &self.lights {
            if !finite(&l.position)
                || !finite(&l.direction)
                || !finite(&l.color)
                || l.color.iter().any(|v| *v < 0.)
                || !l.intensity.is_finite()
                || !(0.0..=1e12).contains(&l.intensity)
                || l.range.is_some_and(|r| !r.is_finite() || r <= 0.)
                || Vec3::from(l.direction).length() < 1e-10
            {
                return Err("SceneLight: invalid light".into());
            }
            if let LightKind::Spot { inner, outer } = l.kind
                && (!inner.is_finite()
                    || !outer.is_finite()
                    || inner < 0.
                    || outer < inner
                    || outer > std::f64::consts::FRAC_PI_2)
            {
                return Err("SceneLight: invalid spot cone".into());
            }
        }
        if let Some((id, node)) = self.imported_camera
            && !self
                .objects
                .iter()
                .any(|o| o.id == id && o.model.nodes.get(node).is_some_and(|n| n.camera.is_some()))
        {
            return Err("SceneCamera: imported camera node is missing".into());
        }
        Ok(())
    }
}

/// Import a scene using bounded resources from the source directory.
/// Args: model path. Returns an embedded model; source files are never modified.
pub fn import_model(path: &Path) -> Result<Model, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let base = path.parent().ok_or("ModelPath: missing source directory")?;
    fn read(path: &Path) -> Option<Vec<u8>> {
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
            return None;
        }
        std::fs::read(path).ok()
    }
    let bytes = read(&path).ok_or("ModelLimit: source must be a regular file under 64 MiB")?;
    model::load(&path.to_string_lossy(), &bytes, &|uri| {
        let resolved = base.join(uri).canonicalize().ok()?;
        if !resolved.starts_with(base) {
            return None;
        }
        read(&resolved)
    })
    .map_err(|e| e.to_string())
}
