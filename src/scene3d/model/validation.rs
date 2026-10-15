//! Import budgets and persistent model validation.
use super::*;
use std::cell::{Cell, RefCell};

pub(super) const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_RESOURCE_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_VERTICES: usize = 2_000_000;
pub(super) const MAX_TRIANGLES: usize = 2_000_000;
pub(super) const MAX_TEXTURE_PIXELS: usize = 16_000_000;
pub(super) const MAX_NODES: usize = 65_536;
pub(super) const MAX_FLOATS: usize = 16_000_000;

pub(super) fn check_source(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(perr("model source exceeds 64 MiB import limit"));
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct ResourceBudget {
    bytes: Cell<usize>,
    count: Cell<usize>,
    error: RefCell<Option<String>>,
}

impl ResourceBudget {
    pub(super) fn resolve(
        &self,
        uri: &str,
        resolve: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        if self.error.borrow().is_some() {
            return None;
        }
        if uri.is_empty()
            || uri.contains(['\0', '\\', ':'])
            || std::path::Path::new(uri).components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
        {
            *self.error.borrow_mut() = Some(format!(
                "resource `{uri}` must be a relative path inside the model directory"
            ));
            return None;
        }
        self.count.set(self.count.get() + 1);
        if self.count.get() > 256 {
            *self.error.borrow_mut() = Some("model exceeds 256 external resource reads".into());
            return None;
        }
        let bytes = resolve(uri)?;
        let total = self.bytes.get().checked_add(bytes.len());
        if bytes.len() > MAX_SOURCE_BYTES || total.is_none_or(|n| n > MAX_RESOURCE_BYTES) {
            *self.error.borrow_mut() = Some(format!(
                "resource `{uri}` exceeds model resource byte budget"
            ));
            return None;
        }
        self.bytes.set(total.unwrap_or(MAX_RESOURCE_BYTES));
        Some(bytes)
    }
    pub(super) fn finish(&self) -> Result<()> {
        if let Some(error) = self.error.borrow().as_ref() {
            return Err(perr(error.clone()));
        }
        Ok(())
    }
}

fn bounded(values: impl IntoIterator<Item = f64>) -> bool {
    values
        .into_iter()
        .all(|v| v.is_finite() && v.abs() <= 1.0e12)
}

fn unit(values: impl IntoIterator<Item = f32>) -> bool {
    values
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
}

impl Model {
    /// Validate an imported or reopened model before rendering.
    /// Takes the model by reference; returns a named error for invalid graph, geometry,
    /// resources or animation, and success for a finite model within the import budgets.
    pub fn validate(&self) -> Result<()> {
        if [
            self.nodes.len(),
            self.meshes.len(),
            self.materials.len(),
            self.cameras.len(),
            self.lights.len(),
            self.animations.len(),
            self.skins.len(),
        ]
        .iter()
        .any(|&n| n > MAX_NODES)
            || self.textures.len() > 256
        {
            return Err(perr("model object count exceeds import limit"));
        }
        let mut pixels = 0usize;
        for (i, t) in self.textures.iter().enumerate() {
            let count = (t.width as usize)
                .checked_mul(t.height as usize)
                .ok_or_else(|| perr("texture size overflow"))?;
            pixels = pixels
                .checked_add(count)
                .ok_or_else(|| perr("texture size overflow"))?;
            if count == 0
                || count != t.data.len()
                || pixels > MAX_TEXTURE_PIXELS
                || !t.data.iter().all(|p| unit(*p))
            {
                return Err(perr(format!(
                    "texture {i}: invalid dimensions, pixels or texture budget"
                )));
            }
        }
        for (i, m) in self.materials.iter().enumerate() {
            if !unit(m.base_color)
                || !unit([
                    m.metallic,
                    m.roughness,
                    m.occlusion_strength,
                    m.alpha_cutoff,
                ])
                || !m.normal_scale.is_finite()
                || m.normal_scale.abs() > 1.0e6
                || !m
                    .emissive
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0e6).contains(v))
            {
                return Err(perr(format!(
                    "material {i}: invalid color, alpha or PBR factor"
                )));
            }
            for r in [
                m.base_color_tex,
                m.metallic_roughness_tex,
                m.normal_tex,
                m.occlusion_tex,
                m.emissive_tex,
            ]
            .into_iter()
            .flatten()
            {
                if r.texture >= self.textures.len() {
                    return Err(perr(format!("material {i}: texture index out of range")));
                }
                if r.uv_set != 0 {
                    return Err(ModelError::Unsupported(format!(
                        "material {i}: texture coordinate set {} (only TEXCOORD_0)",
                        r.uv_set
                    )));
                }
            }
        }
        let mut vertices = 0usize;
        let mut triangles = 0usize;
        let mut primitives = 0usize;
        for (mi, mesh) in self.meshes.iter().enumerate() {
            primitives = primitives
                .checked_add(mesh.primitives.len())
                .ok_or_else(|| perr("primitive count overflow"))?;
            if primitives > MAX_NODES {
                return Err(perr("too many mesh primitives"));
            }
            for (pi, p) in mesh.primitives.iter().enumerate() {
                let bad = |why: &str| perr(format!("mesh {mi} primitive {pi}: {why}"));
                let n = p.positions.len();
                vertices = vertices
                    .checked_add(n)
                    .ok_or_else(|| bad("vertex count overflow"))?;
                triangles = triangles
                    .checked_add(p.triangle_count())
                    .ok_or_else(|| bad("triangle count overflow"))?;
                if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
                    return Err(bad("geometry budget exceeded"));
                }
                if n == 0
                    || p.indices.is_empty()
                    || p.indices.len() % 3 != 0
                    || p.indices.iter().any(|&v| v as usize >= n)
                {
                    return Err(bad("empty or invalid triangle indices"));
                }
                if [
                    p.normals.len(),
                    p.uvs.len(),
                    p.tangents.len(),
                    p.joints.len(),
                    p.weights.len(),
                ]
                .into_iter()
                .any(|len| len != 0 && len != n)
                    || p.joints.is_empty() != p.weights.is_empty()
                {
                    return Err(bad("vertex attribute lengths disagree"));
                }
                if !bounded(p.positions.iter().flatten().map(|&v| v as f64))
                    || !bounded(p.normals.iter().flatten().map(|&v| v as f64))
                    || !bounded(p.uvs.iter().flatten().map(|&v| v as f64))
                    || !bounded(p.tangents.iter().flatten().map(|&v| v as f64))
                {
                    return Err(bad("nonfinite or excessive vertex value"));
                }
                if p.material.is_some_and(|v| v >= self.materials.len()) {
                    return Err(bad("material index out of range"));
                }
                for weights in &p.weights {
                    if !unit(*weights) || (weights.iter().sum::<f32>() - 1.0).abs() > 0.01 {
                        return Err(bad("skin weights must be normalized and nonnegative"));
                    }
                }
            }
        }
        let mut parent = vec![None; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            if !bounded(n.translation)
                || !bounded(n.scale)
                || !bounded(n.rotation)
                || n.rotation.iter().map(|v| v * v).sum::<f64>() < 1.0e-20
                || n.matrix
                    .is_some_and(|m| !bounded(m.0.into_iter().flatten()))
            {
                return Err(perr(format!("node {i}: invalid transform")));
            }
            for (index, count, label) in [
                (n.mesh, self.meshes.len(), "mesh"),
                (n.skin, self.skins.len(), "skin"),
                (n.camera, self.cameras.len(), "camera"),
                (n.light, self.lights.len(), "light"),
            ] {
                if index.is_some_and(|j| j >= count) {
                    return Err(perr(format!("node {i}: {label} index out of range")));
                }
            }
            for &child in &n.children {
                if child >= self.nodes.len() || child == i {
                    return Err(perr(format!("node {i}: invalid child index")));
                }
                if parent[child].replace(i).is_some() {
                    return Err(perr(format!("node {child}: more than one parent")));
                }
            }
            if let (Some(mesh), Some(skin)) = (n.mesh, n.skin) {
                let count = self.skins[skin].joints.len();
                for p in &self.meshes[mesh].primitives {
                    if p.joints.iter().flatten().any(|&j| j as usize >= count) {
                        return Err(perr(format!("node {i}: skin joint index out of range")));
                    }
                }
            }
        }
        let mut visited = vec![false; self.nodes.len()];
        let mut roots = std::collections::HashSet::new();
        for &root in &self.roots {
            if root >= self.nodes.len() || parent[root].is_some() || !roots.insert(root) {
                return Err(perr("invalid or duplicate scene root"));
            }
        }
        let mut stack: Vec<_> = parent
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_none())
            .map(|(i, _)| (i, 0usize, Mat4::IDENTITY))
            .collect();
        while let Some((i, depth, world)) = stack.pop() {
            if depth > 128 || visited[i] {
                return Err(perr("node hierarchy exceeds depth limit or repeats a node"));
            }
            visited[i] = true;
            let world = world * self.nodes[i].local();
            if !bounded(world.0.into_iter().flatten()) {
                return Err(perr(format!(
                    "node {i}: world transform exceeds finite scene bounds"
                )));
            }
            for &c in &self.nodes[i].children {
                stack.push((c, depth + 1, world));
            }
        }
        if visited.contains(&false) {
            return Err(perr("cycle in model node hierarchy"));
        }
        for (i, s) in self.skins.iter().enumerate() {
            if s.joints.is_empty()
                || s.joints.len() != s.inverse_bind.len()
                || s.joints.len() > 65_536
                || s.joints.iter().any(|&j| j >= self.nodes.len())
                || !bounded(
                    s.inverse_bind
                        .iter()
                        .flat_map(|m| m.0.into_iter().flatten()),
                )
            {
                return Err(perr(format!(
                    "skin {i}: invalid joints or inverse bind matrices"
                )));
            }
        }
        let mut keys = 0usize;
        for (i, a) in self.animations.iter().enumerate() {
            if a.channels.len() > MAX_NODES {
                return Err(perr("animation channel limit exceeded"));
            }
            let mut targets = std::collections::HashSet::new();
            for c in &a.channels {
                keys = keys
                    .checked_add(c.times.len())
                    .ok_or_else(|| perr("animation size overflow"))?;
                let stride = c.width()
                    * if c.interp == Interp::CubicSpline {
                        3
                    } else {
                        1
                    };
                if c.node >= self.nodes.len()
                    || self.nodes[c.node].matrix.is_some()
                    || !targets.insert((
                        c.node,
                        match c.path {
                            ChannelPath::Translation => 0,
                            ChannelPath::Rotation => 1,
                            ChannelPath::Scale => 2,
                        },
                    ))
                    || c.times.is_empty()
                    || keys > 1_000_000
                    || c.times.len().checked_mul(stride) != Some(c.values.len())
                    || !c.times.iter().all(|v| v.is_finite() && *v >= 0.0)
                    || c.times.windows(2).any(|v| v[0] >= v[1])
                    || !bounded(c.values.iter().map(|&v| v as f64))
                {
                    return Err(perr(format!(
                        "animation {i}: invalid node, keys, times or values"
                    )));
                }
            }
        }
        for (i, c) in self.cameras.iter().enumerate() {
            let valid = match c.projection {
                CameraProjection::Perspective {
                    yfov,
                    aspect,
                    znear,
                    zfar,
                } => {
                    yfov.is_finite()
                        && (0.0..std::f64::consts::PI).contains(&yfov)
                        && yfov > 0.0
                        && znear.is_finite()
                        && znear > 0.0
                        && zfar.is_none_or(|far| far.is_finite() && far > znear)
                        && aspect.is_none_or(|v| v.is_finite() && v > 0.0)
                }
                CameraProjection::Orthographic {
                    xmag,
                    ymag,
                    znear,
                    zfar,
                } => {
                    if !znear.is_finite() || !zfar.is_finite() || znear < 0.0 || zfar <= znear {
                        return Err(perr(format!(
                            "camera {i}: invalid orthographic clipping planes"
                        )));
                    }
                    xmag.is_finite() && ymag.is_finite() && xmag > 0.0 && ymag > 0.0
                }
            };
            if !valid {
                return Err(perr(format!("camera {i}: invalid projection")));
            }
        }
        for (i, l) in self.lights.iter().enumerate() {
            if !bounded(l.color)
                || l.color.iter().any(|v| *v < 0.0)
                || !l.intensity.is_finite()
                || !(0.0..=1.0e12).contains(&l.intensity)
                || l.range.is_some_and(|v| !v.is_finite() || v <= 0.0)
                || matches!(l.kind,ModelLightKind::Spot{inner,outer} if !inner.is_finite() || !outer.is_finite() || inner<0.0 || outer<=0.0 || inner>outer || outer>std::f64::consts::FRAC_PI_2)
            {
                return Err(perr(format!(
                    "light {i}: invalid color, intensity, range or cone"
                )));
            }
        }
        Ok(())
    }
}
