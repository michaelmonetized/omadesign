//! Deterministic CPU triangles, linear-light PBR and depth-sorted transparency.
use super::*;
use model::{AlphaMode, CameraProjection, ModelLightKind, Texture, Wrap, vec3};

const MAX_PIXELS: usize = 16_000_000;
const MAX_TRIANGLES: usize = 250_000;
const MAX_FRAGMENTS: usize = 2_000_000;
const MAX_SAMPLES: usize = 100_000_000;

/// Floating-point premultiplied linear RGBA, before output encoding.
pub struct LinearFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}
impl LinearFrame {
    /// Composite a linear scene over an existing sRGB surface.
    /// Args: same-size destination. Returns an error when dimensions differ.
    pub fn composite(&self, dst: &mut tiny_skia::Pixmap) -> Result<(), String> {
        if dst.width() != self.width
            || dst.height() != self.height
            || self.pixels.len() != (self.width as usize).saturating_mul(self.height as usize)
        {
            return Err("SceneSurface: size mismatch".into());
        }
        if self
            .pixels
            .iter()
            .any(|p| p.iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&p[3]))
        {
            return Err("SceneColor: invalid linear surface".into());
        }
        for (src, dst) in self.pixels.iter().zip(dst.data_mut().chunks_exact_mut(4)) {
            if src[3] <= 0. {
                continue;
            }
            let da = dst[3] as f64 / 255.;
            let a = src[3] as f64 + da * (1. - src[3] as f64);
            for k in 0..3 {
                let back = if da > 0. {
                    decode((dst[k] as f64 / 255. / da).clamp(0., 1.)) * da
                } else {
                    0.
                };
                let color = src[k] as f64 + back * (1. - src[3] as f64);
                dst[k] = (encode((color / a).max(0.)).clamp(0., 1.) * a * 255.)
                    .round()
                    .clamp(0., 255.) as u8;
            }
            dst[3] = (a * 255.).round().clamp(0., 255.) as u8;
        }
        Ok(())
    }
    /// Encode the float scene to a portable PNG.
    /// Args: float frame. Returns independently decodable sRGB PNG bytes.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut pm = tiny_skia::Pixmap::new(self.width, self.height)
            .ok_or("SceneSurface: allocation failed")?;
        self.composite(&mut pm)?;
        pm.encode_png().map_err(|e| e.to_string())
    }
}

/// Projection area and affine document-to-output mapping.
#[derive(Clone, Copy)]
pub struct View {
    pub width: u32,
    pub height: u32,
    pub document_width: f64,
    pub document_height: f64,
    pub affine: [f64; 6],
}
impl View {
    pub fn image(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            document_width: width as f64,
            document_height: height as f64,
            affine: [1., 0., 0., 1., 0., 0.],
        }
    }
}
struct CameraView {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    projection: CameraProjection,
}
impl CameraView {
    fn position(&self, p: Vec3) -> Vec3 {
        let d = p - self.eye;
        vec3(d.dot(self.right), d.dot(self.up), d.dot(self.forward))
    }
    fn distances(&self, p: Vec3, aspect: f64) -> [f64; 6] {
        match self.projection {
            CameraProjection::Perspective {
                yfov,
                aspect: given,
                znear,
                zfar,
            } => {
                let h = (yfov * 0.5).tan() * p.z;
                let w = h * given.unwrap_or(aspect);
                [
                    p.z - znear,
                    zfar.map_or(1., |f| f - p.z),
                    w + p.x,
                    w - p.x,
                    h + p.y,
                    h - p.y,
                ]
            }
            CameraProjection::Orthographic {
                xmag,
                ymag,
                znear,
                zfar,
            } => [
                p.z - znear,
                zfar - p.z,
                xmag + p.x,
                xmag - p.x,
                ymag + p.y,
                ymag - p.y,
            ],
        }
    }
    fn project(&self, p: Vec3, view: View) -> [f64; 3] {
        let (x, y, recip) = match self.projection {
            CameraProjection::Perspective { yfov, aspect, .. } => {
                let h = p.z * (yfov * 0.5).tan();
                (
                    p.x / (h * aspect.unwrap_or(view.document_width / view.document_height)),
                    p.y / h,
                    1. / p.z,
                )
            }
            CameraProjection::Orthographic { xmag, ymag, .. } => (p.x / xmag, p.y / ymag, 1.),
        };
        let x = (x * 0.5 + 0.5) * view.document_width;
        let y = (0.5 - y * 0.5) * view.document_height;
        let [sx, ky, kx, sy, tx, ty] = view.affine;
        [sx * x + kx * y + tx, ky * x + sy * y + ty, recip]
    }
}

fn camera(scene: &Scene, time: f64) -> Result<CameraView, String> {
    if let Some((id, node)) = scene.imported_camera {
        let o = scene
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or("SceneCamera: object missing")?;
        let placed = o
            .model
            .placed(false, o.animation, o.local_time(time))
            .into_iter()
            .find(|p| p.node == node)
            .ok_or("SceneCamera: node missing")?;
        let m = o.transform() * placed.world;
        if !matrix_finite(m) {
            return Err("SceneTransform: nonfinite animated camera".into());
        }
        let eye = m.apply(Vec3::ZERO);
        let forward = m.apply_vec(vec3(0., 0., -1.)).normalize();
        let right = forward.cross(m.apply_vec(vec3(0., 1., 0.))).normalize();
        let up = right.cross(forward).normalize();
        return Ok(CameraView {
            eye,
            right,
            up,
            forward,
            projection: o.model.cameras[placed.index].projection,
        });
    }
    let c = &scene.camera;
    let eye = c.position.into();
    let forward = (Vec3::from(c.target) - eye).normalize();
    let right = forward.cross(c.up.into()).normalize();
    let up = right.cross(forward).normalize();
    let projection = match c.projection {
        Projection::Perspective { fov } => CameraProjection::Perspective {
            yfov: fov.to_radians(),
            aspect: None,
            znear: c.near,
            zfar: Some(c.far),
        },
        Projection::Orthographic { height } => CameraProjection::Orthographic {
            xmag: height * 0.5,
            ymag: height * 0.5,
            znear: c.near,
            zfar: c.far,
        },
    };
    Ok(CameraView {
        eye,
        right,
        up,
        forward,
        projection,
    })
}
fn scene_lights(scene: &Scene, time: f64) -> Result<Vec<Light>, String> {
    let mut lights = scene.lights.clone();
    if scene.imported_lights {
        for object in scene.objects.iter().filter(|o| o.visible) {
            for placed in object
                .model
                .placed(true, object.animation, object.local_time(time))
            {
                let m = object.transform() * placed.world;
                if !matrix_finite(m) {
                    return Err("SceneTransform: nonfinite animated light".into());
                }
                let source = &object.model.lights[placed.index];
                let p = m.apply(Vec3::ZERO);
                let d = m.apply_vec(vec3(0., 0., -1.)).normalize();
                lights.push(Light {
                    kind: match source.kind {
                        ModelLightKind::Directional => LightKind::Directional,
                        ModelLightKind::Point => LightKind::Point,
                        ModelLightKind::Spot { inner, outer } => LightKind::Spot { inner, outer },
                    },
                    position: [p.x, p.y, p.z],
                    direction: [d.x, d.y, d.z],
                    color: source.color,
                    intensity: source.intensity,
                    range: source.range,
                });
                if lights.len() > 128 {
                    return Err("SceneLight: at most 128 active lights".into());
                }
            }
        }
    }
    Ok(lights)
}
#[derive(Clone, Copy)]
struct Vertex {
    p: Vec3,
    view: Vec3,
    n: Vec3,
    t: Vec3,
    hand: f64,
    uv: [f64; 2],
}
impl Vertex {
    fn mix(self, b: Self, u: f64) -> Self {
        Self {
            p: self.p + (b.p - self.p) * u,
            view: self.view + (b.view - self.view) * u,
            n: self.n + (b.n - self.n) * u,
            t: self.t + (b.t - self.t) * u,
            hand: self.hand + (b.hand - self.hand) * u,
            uv: [
                self.uv[0] + (b.uv[0] - self.uv[0]) * u,
                self.uv[1] + (b.uv[1] - self.uv[1]) * u,
            ],
        }
    }
}
fn finite(v: Vec3) -> bool {
    [v.x, v.y, v.z].iter().all(|x| x.is_finite())
}
fn determinant(m: Mat4) -> f64 {
    let m = m.0;
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}
fn matrix_finite(m: Mat4) -> bool {
    m.0.iter().flatten().all(|x| x.is_finite())
}
fn clip(mut poly: Vec<Vertex>, camera: &CameraView, aspect: f64) -> Vec<Vertex> {
    for plane in 0..6 {
        if poly.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(poly.len() + 1);
        let mut a = *poly.last().unwrap();
        let mut da = camera.distances(a.view, aspect)[plane];
        for &b in &poly {
            let db = camera.distances(b.view, aspect)[plane];
            if (da >= 0.) != (db >= 0.) {
                next.push(a.mix(b, da / (da - db)));
            }
            if db >= 0. {
                next.push(b);
            }
            a = b;
            da = db;
        }
        poly = next;
    }
    poly
}
struct Fragment {
    pixel: usize,
    depth: f64,
    color: [f32; 4],
}
struct Raster {
    frame: LinearFrame,
    depth: Vec<f64>,
    transparent: Vec<Fragment>,
    samples: usize,
    triangles: usize,
}

/// Render meshes, animated nodes/skins, cameras, lights, materials and alpha.
/// Args: validated scene, bounded output view, time and cancellation callback.
/// Returns premultiplied float linear pixels or a named error without partial publication.
pub fn render(
    scene: &Scene,
    view: View,
    time: f64,
    keep: &mut dyn FnMut() -> bool,
) -> Result<LinearFrame, String> {
    if !keep() {
        return Err("Cancelled: render cancelled".into());
    }
    scene.validate()?;
    let count = (view.width as usize)
        .checked_mul(view.height as usize)
        .filter(|n| *n > 0 && *n <= MAX_PIXELS)
        .ok_or("ScenePixels: at most 16 million output pixels")?;
    if !time.is_finite()
        || ![view.document_width, view.document_height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
        || !view.affine.iter().all(|v| v.is_finite())
    {
        return Err("SceneView: invalid output transform or time".into());
    }
    let mut camera = camera(scene, time)?;
    if let CameraProjection::Orthographic { xmag, .. } = &mut camera.projection
        && scene.imported_camera.is_none()
    {
        *xmag *= view.document_width / view.document_height;
    }
    if !finite(camera.eye)
        || camera.forward.length() < 0.9
        || camera.right.length() < 0.9
        || camera.up.length() < 0.9
    {
        return Err("SceneCamera: degenerate animated camera".into());
    }
    let lights = scene_lights(scene, time)?;
    let mut r = Raster {
        frame: LinearFrame {
            width: view.width,
            height: view.height,
            pixels: vec![[0.; 4]; count],
        },
        depth: vec![f64::INFINITY; count],
        transparent: vec![],
        samples: MAX_SAMPLES,
        triangles: 0,
    };
    for object in scene.objects.iter().filter(|o| o.visible && o.opacity > 0.) {
        let local = object.local_time(time);
        for instance in object.model.instances(object.animation, local) {
            for primitive in &instance.mesh.primitives {
                let material = primitive
                    .material
                    .and_then(|i| object.model.materials.get(i))
                    .cloned()
                    .unwrap_or_else(|| model::Material::plain([0.7, 0.7, 0.7, 1.], 0., 0.8));
                for tri in primitive.indices.chunks_exact(3) {
                    if !keep() {
                        return Err("Cancelled: render cancelled".into());
                    }
                    r.triangles += 1;
                    if r.triangles > MAX_TRIANGLES {
                        return Err("SceneTriangles: at most 250000 rendered triangles".into());
                    }
                    let mut vertices = Vec::with_capacity(3);
                    for &k in tri {
                        let k = k as usize;
                        let m = object.transform() * model::vertex_matrix(&instance, primitive, k);
                        let p = m.apply(Vec3::from(primitive.positions[k].map(|v| v as f64)));
                        let inv = m
                            .inverse()
                            .filter(|m| matrix_finite(*m))
                            .ok_or("SceneTransform: singular animated mesh transform")?;
                        let n = inv
                            .transpose()
                            .apply_vec(Vec3::from(
                                primitive
                                    .normals
                                    .get(k)
                                    .copied()
                                    .unwrap_or([0.; 3])
                                    .map(|v| v as f64),
                            ))
                            .normalize();
                        let tangent = primitive
                            .tangents
                            .get(k)
                            .copied()
                            .unwrap_or([1., 0., 0., 1.]);
                        let t = m
                            .apply_vec(vec3(
                                tangent[0] as f64,
                                tangent[1] as f64,
                                tangent[2] as f64,
                            ))
                            .normalize();
                        let uv = primitive.uvs.get(k).copied().unwrap_or([0.; 2]);
                        let v = Vertex {
                            p,
                            view: camera.position(p),
                            n,
                            t,
                            hand: tangent[3] as f64 * determinant(m).signum(),
                            uv: uv.map(|v| v as f64),
                        };
                        if !matrix_finite(m)
                            || !finite(v.p)
                            || !finite(v.view)
                            || !finite(v.n)
                            || !finite(v.t)
                        {
                            return Err("SceneTransform: nonfinite animated mesh".into());
                        }
                        vertices.push(v);
                    }
                    let geometric =
                        (vertices[1].p - vertices[0].p).cross(vertices[2].p - vertices[0].p);
                    for v in &mut vertices {
                        if v.n.length() < 1e-10 {
                            v.n = geometric.normalize();
                        }
                    }
                    if !finite(geometric) {
                        return Err("SceneTransform: triangle normal overflow".into());
                    }
                    let normals = vertices.iter().fold(Vec3::ZERO, |a, v| a + v.n);
                    let sign = if geometric.dot(normals) < 0. { -1. } else { 1. };
                    let toward_camera = match camera.projection {
                        CameraProjection::Orthographic { .. } => -camera.forward,
                        _ => camera.eye - vertices[0].p,
                    };
                    let front = (geometric * sign).dot(toward_camera) > 0.;
                    if !front && !material.double_sided {
                        continue;
                    }
                    if !front {
                        for v in &mut vertices {
                            v.n = -v.n;
                        }
                    }
                    let poly = clip(
                        vertices,
                        &camera,
                        view.document_width / view.document_height,
                    );
                    for i in 1..poly.len().saturating_sub(1) {
                        r.triangle(
                            [poly[0], poly[i], poly[i + 1]],
                            view,
                            &camera,
                            &material,
                            &object.model,
                            object.opacity,
                            scene.ambient,
                            &lights,
                            keep,
                        )?;
                    }
                }
            }
        }
    }
    r.transparent.sort_by(|a, b| {
        a.pixel
            .cmp(&b.pixel)
            .then_with(|| b.depth.total_cmp(&a.depth))
    });
    for (i, f) in r.transparent.into_iter().enumerate() {
        if i % 4096 == 0 && !keep() {
            return Err("Cancelled: render cancelled".into());
        }
        if f.depth <= r.depth[f.pixel] {
            let dst = &mut r.frame.pixels[f.pixel];
            for (d, s) in dst.iter_mut().zip(f.color) {
                *d = s + *d * (1. - f.color[3]);
            }
        }
    }
    Ok(r.frame)
}
fn edge(a: [f64; 3], b: [f64; 3], p: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
impl Raster {
    fn triangle(
        &mut self,
        v: [Vertex; 3],
        view: View,
        camera: &CameraView,
        material: &model::Material,
        model: &Model,
        opacity: f32,
        ambient: [f64; 3],
        lights: &[Light],
        keep: &mut dyn FnMut() -> bool,
    ) -> Result<(), String> {
        let s = v.map(|v| camera.project(v.view, view));
        if s.iter().flatten().any(|x| !x.is_finite()) {
            return Err("SceneProjection: nonfinite projected vertex".into());
        }
        let area = edge(s[0], s[1], [s[2][0], s[2][1]]);
        if area.abs() < 1e-12 {
            return Ok(());
        }
        let minx = s
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0., view.width as f64) as usize;
        let maxx = s
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0., view.width as f64) as usize;
        let miny = s
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0., view.height as f64) as usize;
        let maxy = s
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0., view.height as f64) as usize;
        for y in miny..maxy {
            if !keep() {
                return Err("Cancelled: render cancelled".into());
            }
            for x in minx..maxx {
                self.samples = self
                    .samples
                    .checked_sub(1)
                    .ok_or("SceneWork: triangle sample budget exhausted")?;
                let p = [x as f64 + 0.5, y as f64 + 0.5];
                let bary = [
                    edge(s[1], s[2], p) / area,
                    edge(s[2], s[0], p) / area,
                    edge(s[0], s[1], p) / area,
                ];
                let top_left =
                    |a: [f64; 3], b: [f64; 3]| b[1] < a[1] || (b[1] == a[1] && b[0] > a[0]);
                if bary.iter().enumerate().any(|(i, b)| {
                    *b < 0.
                        || (*b == 0.
                            && if area > 0. {
                                !top_left(s[(i + 1) % 3], s[(i + 2) % 3])
                            } else {
                                !top_left(s[(i + 2) % 3], s[(i + 1) % 3])
                            })
                }) {
                    continue;
                }
                let denom = (0..3).map(|i| bary[i] * s[i][2]).sum::<f64>();
                let weights = std::array::from_fn::<_, 3, _>(|i| bary[i] * s[i][2] / denom);
                let depth = (0..3).map(|i| v[i].view.z * weights[i]).sum::<f64>();
                let pixel = y * view.width as usize + x;
                if depth > self.depth[pixel] {
                    continue;
                }
                let interpolate = |get: fn(Vertex) -> Vec3| {
                    (0..3).fold(Vec3::ZERO, |a, i| a + get(v[i]) * weights[i])
                };
                let point = interpolate(|v| v.p);
                let mut normal = interpolate(|v| v.n).normalize();
                let tangent = interpolate(|v| v.t).normalize();
                let uv = [
                    (0..3).map(|i| v[i].uv[0] * weights[i]).sum(),
                    (0..3).map(|i| v[i].uv[1] * weights[i]).sum(),
                ];
                if let Some(map) = material
                    .normal_tex
                    .and_then(|t| model.textures.get(t.texture))
                {
                    let tex = sample(map, uv, false);
                    let map = vec3(
                        (tex[0] * 2. - 1.) * material.normal_scale as f64,
                        (tex[1] * 2. - 1.) * material.normal_scale as f64,
                        tex[2] * 2. - 1.,
                    )
                    .normalize();
                    let tangent = (tangent - normal * normal.dot(tangent)).normalize();
                    let bitangent = normal.cross(tangent) * v[0].hand;
                    normal = (tangent * map.x + bitangent * map.y + normal * map.z).normalize();
                }
                let color = shade(
                    material,
                    model,
                    uv,
                    point,
                    normal,
                    match camera.projection {
                        CameraProjection::Orthographic { .. } => -camera.forward,
                        _ => (camera.eye - point).normalize(),
                    },
                    ambient,
                    lights,
                    opacity,
                );
                if color.iter().any(|v| !v.is_finite()) {
                    return Err("SceneColor: shading exceeded finite float range".into());
                }
                if color[3] <= 0. {
                    continue;
                }
                if color[3] >= 1. {
                    self.depth[pixel] = depth;
                    self.frame.pixels[pixel] = color;
                } else {
                    if self.transparent.len() >= MAX_FRAGMENTS {
                        return Err(
                            "SceneTransparency: at most two million translucent fragments".into(),
                        );
                    }
                    self.transparent.push(Fragment {
                        pixel,
                        depth,
                        color,
                    });
                }
            }
        }
        Ok(())
    }
}
fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(v: f64) -> f64 {
    if v >= 1. {
        return 1.;
    }
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn sample(texture: &Texture, uv: [f64; 2], srgb: bool) -> [f64; 4] {
    fn index(v: i64, n: i64, w: Wrap) -> usize {
        match w {
            Wrap::Clamp => v.clamp(0, n - 1) as usize,
            Wrap::Repeat => v.rem_euclid(n) as usize,
            Wrap::Mirror => {
                let v = v.rem_euclid(n * 2);
                if v < n {
                    v as usize
                } else {
                    (n * 2 - 1 - v) as usize
                }
            }
        }
    }
    let x = uv[0] * texture.width as f64 - 0.5;
    let y = uv[1] * texture.height as f64 - 0.5;
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (tx, ty) = (x - x.floor(), y - y.floor());
    let at = |x, y| {
        let mut value = texture.data[index(y, texture.height as i64, texture.wrap[1])
            * texture.width as usize
            + index(x, texture.width as i64, texture.wrap[0])]
        .map(|c| c as f64);
        if srgb {
            for c in &mut value[..3] {
                *c = decode(*c);
            }
        }
        value
    };
    let a = at(ix, iy);
    let b = at(ix.saturating_add(1), iy);
    let c = at(ix, iy.saturating_add(1));
    let d = at(ix.saturating_add(1), iy.saturating_add(1));
    std::array::from_fn(|k| {
        (a[k] * (1. - tx) + b[k] * tx) * (1. - ty) + (c[k] * (1. - tx) + d[k] * tx) * ty
    })
}
fn shade(
    m: &model::Material,
    model: &Model,
    uv: [f64; 2],
    p: Vec3,
    n: Vec3,
    view: Vec3,
    ambient: [f64; 3],
    lights: &[Light],
    opacity: f32,
) -> [f32; 4] {
    let mut base = m.base_color.map(|x| x as f64);
    if let Some(t) = m.base_color_tex.and_then(|t| model.textures.get(t.texture)) {
        let tex = sample(t, uv, true);
        for k in 0..3 {
            base[k] *= tex[k];
        }
        base[3] *= tex[3];
    }
    let alpha = match m.alpha_mode {
        AlphaMode::Opaque => 1.,
        AlphaMode::Mask => {
            if base[3] >= m.alpha_cutoff as f64 {
                1.
            } else {
                0.
            }
        }
        AlphaMode::Blend => base[3],
    } * opacity as f64;
    if alpha <= 0. {
        return [0.; 4];
    }
    let mut color = [0.; 3];
    if m.unlit {
        color.copy_from_slice(&base[..3]);
    } else {
        let mut metallic = m.metallic as f64;
        let mut rough = m.roughness as f64;
        if let Some(t) = m
            .metallic_roughness_tex
            .and_then(|t| model.textures.get(t.texture))
        {
            let tex = sample(t, uv, false);
            rough *= tex[1];
            metallic *= tex[2];
        }
        rough = rough.clamp(0.045, 1.);
        metallic = metallic.clamp(0., 1.);
        let ao = m
            .occlusion_tex
            .and_then(|t| model.textures.get(t.texture))
            .map_or(1., |t| {
                1. + m.occlusion_strength as f64 * (sample(t, uv, false)[0] - 1.)
            });
        for k in 0..3 {
            color[k] = base[k] * ambient[k] * ao;
        }
        let nv = n.dot(view).max(1e-6);
        for light in lights {
            let (l, mut attenuation) = match light.kind {
                LightKind::Directional => (-Vec3::from(light.direction).normalize(), 1.),
                _ => {
                    let delta = Vec3::from(light.position) - p;
                    let d = delta.length().max(1e-6);
                    let fade = light
                        .range
                        .map_or(1., |r| (1. - (d / r).powi(4)).clamp(0., 1.).powi(2));
                    (delta / d, fade / (d * d))
                }
            };
            if let LightKind::Spot { inner, outer } = light.kind {
                let theta = (-l).dot(Vec3::from(light.direction).normalize());
                let u = if inner == outer {
                    if theta >= outer.cos() { 1. } else { 0. }
                } else {
                    ((theta - outer.cos()) / (inner.cos() - outer.cos())).clamp(0., 1.)
                };
                attenuation *= u * u * (3. - 2. * u);
            }
            let nl = n.dot(l).max(0.);
            if nl <= 0. {
                continue;
            }
            let h = (view + l).normalize();
            let nh = n.dot(h).max(0.);
            let vh = view.dot(h).max(0.);
            let a2 = rough.powi(4);
            let d = a2 / (std::f64::consts::PI * (nh * nh * (a2 - 1.) + 1.).powi(2));
            let k = (rough + 1.).powi(2) / 8.;
            let g = nv / (nv * (1. - k) + k) * nl / (nl * (1. - k) + k);
            for k in 0..3 {
                let f0 = 0.04 * (1. - metallic) + base[k] * metallic;
                let f = f0 + (1. - f0) * (1. - vh).powi(5);
                let spec = d * g * f / (4. * nv * nl).max(1e-6);
                let diffuse = (1. - f) * (1. - metallic) * base[k] / std::f64::consts::PI;
                color[k] += (diffuse + spec) * light.color[k] * light.intensity * attenuation * nl;
            }
        }
        let tex = m
            .emissive_tex
            .and_then(|t| model.textures.get(t.texture))
            .map(|t| sample(t, uv, true));
        for k in 0..3 {
            color[k] += m.emissive[k] as f64 * tex.map_or(1., |t| t[k]);
        }
    }
    [
        (color[0].max(0.) * alpha) as f32,
        (color[1].max(0.) * alpha) as f32,
        (color[2].max(0.) * alpha) as f32,
        alpha as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle(color: [f32; 4], alpha: AlphaMode, z: f32) -> Model {
        let mut p = model::Primitive {
            positions: vec![[-1., -1., z], [1., -1., z], [0., 1., z]],
            indices: vec![0, 1, 2],
            material: Some(0),
            ..Default::default()
        };
        p.compute_normals();
        let mut m = model::Material::plain(color, 0., 0.8);
        m.alpha_mode = alpha;
        m.unlit = true;
        Model::single(
            model::Mesh {
                name: "Triangle".into(),
                primitives: vec![p],
            },
            vec![m],
        )
    }
    fn scene(model: Model) -> Scene {
        let mut s = Scene::default();
        s.add(model, "Test".into()).unwrap();
        s.camera = Camera {
            position: [0., 0., 3.],
            target: [0.; 3],
            projection: Projection::Orthographic { height: 2. },
            near: 0.1,
            far: 10.,
            ..Camera::default()
        };
        s
    }
    fn draw(scene: &Scene, time: f64) -> LinearFrame {
        render(scene, View::image(64, 64), time, &mut || true).unwrap()
    }
    #[test]
    fn independent_png_alpha_and_linear_composition() {
        let s = scene(triangle([1., 0., 0., 0.5], AlphaMode::Blend, 0.));
        let frame = draw(&s, 0.);
        let pixel = frame.pixels[32 * 64 + 32];
        assert!((pixel[0] - 0.5).abs() < 1e-6);
        assert_eq!(pixel[3], 0.5);
        let image = image::load_from_memory(&frame.png().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (64, 64));
        assert_eq!(image.get_pixel(32, 32).0, [255, 0, 0, 128]);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        let mut pm = tiny_skia::Pixmap::new(64, 64).unwrap();
        pm.fill(tiny_skia::Color::WHITE);
        frame.composite(&mut pm).unwrap();
        let image = image::load_from_memory(&pm.encode_png().unwrap())
            .unwrap()
            .to_rgba8();
        let p = image.get_pixel(32, 32).0;
        assert_eq!(p[0], 255);
        assert!((p[1] as i32 - 188).abs() <= 1);
        assert_eq!(p[3], 255);
    }
    #[test]
    fn occlusion_and_transparency_are_independent_of_object_order() {
        let mut s = scene(triangle([1., 0., 0., 0.5], AlphaMode::Blend, 0.5));
        s.add(
            triangle([0., 0., 1., 1.], AlphaMode::Opaque, 0.),
            "Back".into(),
        )
        .unwrap();
        let a = draw(&s, 0.);
        s.objects.reverse();
        let b = draw(&s, 0.);
        assert_eq!(a.pixels, b.pixels);
        let p = a.pixels[32 * 64 + 32];
        assert!((p[0] - 0.5).abs() < 1e-6 && (p[2] - 0.5).abs() < 1e-6 && p[3] == 1.);
        s.objects[1].model = Arc::new(triangle([1., 0., 0., 0.25], AlphaMode::Mask, 0.5));
        let p = draw(&s, 0.).pixels[32 * 64 + 32];
        assert_eq!(p, [0., 0., 1., 1.]);
    }
    #[test]
    fn camera_light_node_animation_and_save_reopen_render() {
        let mut m = triangle([0.8, 0.4, 0.1, 1.], AlphaMode::Opaque, 0.);
        m.materials[0].unlit = false;
        m.cameras.push(model::ModelCamera {
            name: "Camera".into(),
            projection: CameraProjection::Perspective {
                yfov: 45_f64.to_radians(),
                aspect: None,
                znear: 0.1,
                zfar: Some(10.),
            },
        });
        m.lights.push(model::ModelLight {
            name: "Key".into(),
            kind: ModelLightKind::Directional,
            color: [1.; 3],
            intensity: 3.,
            range: None,
        });
        m.nodes.push(model::Node {
            translation: [0., 0., 3.],
            camera: Some(0),
            ..Default::default()
        });
        m.nodes.push(model::Node {
            light: Some(0),
            ..Default::default()
        });
        m.roots.extend([1, 2]);
        m.animations.push(model::Animation {
            name: "Move".into(),
            channels: vec![model::Channel {
                node: 0,
                path: model::ChannelPath::Translation,
                interp: model::Interp::Linear,
                times: vec![0., 1.],
                values: vec![-0.5, 0., 0., 0.5, 0., 0.],
            }],
        });
        let mut s = Scene::default();
        s.add(m, "Animated".into()).unwrap();
        s.objects[0].looped = false;
        let a = draw(&s, 0.);
        let b = draw(&s, 1.);
        assert_ne!(a.pixels, b.pixels);
        assert!(a.pixels.iter().any(|p| p[0] > 0.1));
        let mut doc = crate::document::Document::new("3D", 64., 64., 96.);
        doc.layers.clear();
        doc.artboards.clear();
        doc.artboardless = true;
        doc.transparent = true;
        doc.motion.scene3d = Some(s.clone());
        let text = crate::project::encode(&doc).unwrap();
        let reopened = crate::project::decode(&text).unwrap();
        assert_eq!(reopened.motion.scene3d, Some(s));
        let png = crate::anim_export::render_frame(&reopened, 1., 1., true).unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_rgba8(),
            image::load_from_memory(&b.png().unwrap())
                .unwrap()
                .to_rgba8()
        );
        let settings = crate::anim_export::AnimSettings {
            scale: 1.,
            fps: 2.,
            start: 0.,
            end: 1.,
            looped: false,
            transparent: true,
        };
        let apng = crate::anim_export::export_animation(
            &reopened,
            crate::anim_export::AnimKind::Apng,
            &settings,
            |_, _| true,
        )
        .unwrap();
        assert!(apng.windows(4).any(|s| s == b"acTL"));
        let mut unlit = reopened.motion.scene3d.clone().unwrap();
        unlit.imported_lights = false;
        unlit.lights.clear();
        unlit.ambient = [0.; 3];
        assert!(draw(&unlit, 0.).pixels.iter().all(|p| p[..3] == [0.; 3]));
    }
    #[test]
    fn clipping_transform_work_limits_and_cancellation() {
        let mut s = scene(triangle([1., 1., 1., 1.], AlphaMode::Opaque, 0.));
        let mut polls = 0;
        assert!(
            render(&s, View::image(64, 64), 0., &mut || {
                polls += 1;
                polls < 5
            })
            .err()
            .unwrap()
            .contains("Cancelled")
        );
        s.camera.far = 1.;
        assert!(draw(&s, 0.).pixels.iter().all(|p| p[3] == 0.));
        s.camera.far = 10.;
        s.objects[0].translation = [0.5, 0., 0.];
        s.objects[0].rotation = [0., 0., 30.];
        s.objects[0].scale = [0.5, 1., 1.];
        assert_ne!(
            draw(&s, 0.).pixels,
            draw(
                &scene(triangle([1., 1., 1., 1.], AlphaMode::Opaque, 0.)),
                0.
            )
            .pixels
        );
        assert!(
            render(&s, View::image(4001, 4000), 0., &mut || true)
                .err()
                .unwrap()
                .contains("ScenePixels")
        );
        let mut m = (*s.objects[0].model).clone();
        m.nodes[0].scale = [0.; 3];
        s.objects[0].model = Arc::new(m);
        assert!(
            render(&s, View::image(64, 64), 0., &mut || true)
                .err()
                .unwrap()
                .contains("Transform")
        );
    }
    #[test]
    fn color_texture_filtering_decodes_before_interpolation() {
        let texture = Texture {
            width: 2,
            height: 1,
            data: vec![[0., 0., 0., 1.], [1., 1., 1., 1.]],
            wrap: [Wrap::Clamp; 2],
        };
        let sampled = sample(&texture, [0.5, 0.5], true);
        assert_eq!(sampled, [0.5, 0.5, 0.5, 1.]);
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    #[test]
    fn hdr_output_preserves_premultiplied_alpha_and_rejects_overflow() {
        let frame = LinearFrame {
            width: 1,
            height: 1,
            pixels: vec![[2., 1., 0.5, 0.5]],
        };
        let mut pm = tiny_skia::Pixmap::new(1, 1).unwrap();
        frame.composite(&mut pm).unwrap();
        assert!(pm.data()[..3].iter().all(|v| *v <= pm.data()[3]));
        assert_eq!(pm.data(), [128, 128, 128, 128]);
        let mut scene = Scene::default();
        scene.lights[0].intensity = f64::MAX;
        assert!(scene.validate().unwrap_err().contains("SceneLight"));
    }
    #[test]
    fn mirrored_normal_map_keeps_the_transformed_bitangent() {
        let mut primitive = model::Primitive {
            positions: vec![[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]],
            normals: vec![[0., 0., 1.]; 3],
            tangents: vec![[1., 0., 0., 1.]; 3],
            uvs: vec![[0., 0.]; 3],
            indices: vec![0, 1, 2],
            material: Some(0),
            ..Default::default()
        };
        primitive.compute_normals();
        let mut material = model::Material::plain([1.; 4], 0., 1.);
        material.normal_tex = Some(model::TexRef {
            texture: 0,
            uv_set: 0,
        });
        let mut model = Model::single(
            model::Mesh {
                name: "Normal mapped".into(),
                primitives: vec![primitive],
            },
            vec![material],
        );
        model
            .textures
            .push(Arc::new(Texture::solid([0.5, 1., 0.5, 1.])));
        let mut scene = Scene::default();
        scene.add(model, "Mirror".into()).unwrap();
        scene.camera = Camera {
            position: [0., 0., 3.],
            projection: Projection::Orthographic { height: 2. },
            ..Camera::default()
        };
        scene.ambient = [0.; 3];
        scene.lights[0].direction = [0., -1., 0.];
        let a = render(&scene, View::image(32, 32), 0., &mut || true).unwrap();
        scene.objects[0].scale = [-1., 1., 1.];
        let b = render(&scene, View::image(32, 32), 0., &mut || true).unwrap();
        let index = 16 * 32 + 16;
        assert!(a.pixels[index][0] > 0.1);
        assert!((a.pixels[index][0] - b.pixels[index][0]).abs() < 1e-5);
    }
}

#[cfg(test)]
mod qualification_tests {
    use super::*;
    fn cube() -> Scene {
        let params = model::prim::PrimParams {
            size: [2.; 3],
            radius: 1.,
            tube: 0.25,
            height: 2.,
            segments: 12,
            rings: 8,
        };
        let mut m = model::prim::model(
            model::prim::PrimitiveKind::Cube,
            &params,
            model::Material::plain([0.8, 0.12, 0.04, 0.7], 0., 0.7),
        );
        m.materials[0].alpha_mode = AlphaMode::Blend;
        m.animations.push(model::Animation {
            name: "Move".into(),
            channels: vec![model::Channel {
                node: 0,
                path: model::ChannelPath::Translation,
                interp: model::Interp::Linear,
                times: vec![0., 1.],
                values: vec![-0.5, 0., 0., 0.5, 0., 0.],
            }],
        });
        let mut s = Scene::default();
        s.add(m, "Cube".into()).unwrap();
        s.objects[0].looped = false;
        s.objects[0].rotation = [22., 35., 0.];
        s
    }
    #[test]
    fn orthographic_off_axis_faces_use_parallel_view_rays() {
        let mut s = cube();
        s.camera = Camera {
            position: [0., 0., 3.],
            projection: Projection::Orthographic { height: 12. },
            ..Camera::default()
        };
        s.objects[0].translation = [4., 0., 0.];
        s.objects[0].rotation = [0., 53., 0.];
        let f = render(&s, View::image(64, 64), 0., &mut || true).unwrap();
        assert!(f.pixels.iter().any(|p| p[3] > 0.));
    }
    #[test]
    fn full_export_bounds_scene_allocation_before_creating_a_surface() {
        let mut doc = crate::document::Document::new("Oversized", 64., 64., 96.);
        doc.layers.clear();
        doc.width = 1_000_000.;
        doc.height = 1_000_000.;
        doc.motion.scene3d = Some(cube());
        assert!(
            crate::anim_export::render_frame(&doc, 0., 1., true)
                .unwrap_err()
                .contains("ScenePixels")
        );
        assert!(
            crate::compositor::export_png(&doc, 1)
                .unwrap_err()
                .contains("ScenePixels")
        );
        let invalid = LinearFrame {
            width: 1,
            height: 1,
            pixels: vec![[f32::INFINITY, 0., 0., 1.]],
        };
        assert!(invalid.png().unwrap_err().contains("SceneColor"));
    }
    #[test]
    fn independent_output_receipt_and_native_disk_reopen() {
        let s = cube();
        let mut doc = crate::document::Document::new("Craft motion qualification", 96., 64., 96.);
        doc.layers.clear();
        doc.artboards.clear();
        doc.artboardless = true;
        doc.transparent = true;
        doc.motion.scene3d = Some(s);
        let a = crate::anim_export::render_frame(&doc, 0., 1., true).unwrap();
        let b = crate::anim_export::render_frame(&doc, 1., 1., true).unwrap();
        assert_ne!(a, b);
        let output = std::env::var_os("CRAFT_MOTION_QA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!("craft-motion-{}", crate::document::next_id()))
            });
        assert!(output.starts_with("/home/"));
        std::fs::create_dir_all(&output).unwrap();
        let path = output.join("scene.oma");
        crate::project::save_to(&doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(reopened.motion, doc.motion);
        assert_eq!(
            crate::anim_export::render_frame(&reopened, 1., 1., true).unwrap(),
            b
        );
        std::fs::write(output.join("scene-start.png"), &a).unwrap();
        std::fs::write(output.join("scene-end.png"), &b).unwrap();
        std::fs::write(
            output.join("scene.omamotion"),
            crate::motion::compositions::export(&doc).unwrap(),
        )
        .unwrap();
        let settings = crate::anim_export::AnimSettings {
            scale: 1.,
            fps: 4.,
            start: 0.,
            end: 1.,
            looped: true,
            transparent: true,
        };
        let apng = crate::anim_export::export_animation(
            &doc,
            crate::anim_export::AnimKind::Apng,
            &settings,
            |_, _| true,
        )
        .unwrap();
        std::fs::write(output.join("scene-animation.png"), apng).unwrap();
        let decoded = image::load_from_memory(&b).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (96, 64));
        assert!(decoded.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255));
        std::fs::write(output.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"width":96,"height":64,"samples":[0,1],"frames_differ":a!=b,"native_disk_reopen":true,"independent_decoder":"image PNG decoder","linear_float_before_encoding":true,"physical_or_installed_runtime_proof":false})).unwrap()).unwrap();
        if std::env::var_os("CRAFT_MOTION_QA_DIR").is_none() {
            std::fs::remove_dir_all(output).unwrap();
        }
    }
}
