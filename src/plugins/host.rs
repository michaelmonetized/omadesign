use super::*;
use crate::{
    color::Rgba,
    document::{self, Fill, Layer, Shape, Stroke, Style},
    geom::{Geom, Pt},
    gradient::{Gradient, GradientKind},
};
use mlua::{Error, Result as LuaResult};
use std::{cell::RefCell, rc::Rc};
pub(super) type Shared = Rc<RefCell<State>>;
pub(super) struct State {
    pub doc: Document,
    pub commands: Vec<Cmd>,
    pub selection: Vec<(usize, u64)>,
    pub active_layer: Option<usize>,
    pub brush: Option<Brush>,
    pub palettes: Vec<Palette>,
    pub message: String,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    started: Instant,
    edit_bytes: usize,
}
impl State {
    pub fn new(
        doc: Document,
        selection: Vec<(usize, u64)>,
        active_layer: Option<usize>,
        root: PathBuf,
        cancel: Arc<AtomicBool>,
    ) -> Shared {
        Rc::new(RefCell::new(Self {
            doc,
            selection,
            active_layer,
            root,
            cancel,
            started: Instant::now(),
            edit_bytes: 0,
            commands: vec![],
            brush: None,
            palettes: vec![],
            message: String::new(),
        }))
    }
    pub fn check(&self) -> LuaResult<()> {
        if self.cancel.load(Ordering::Relaxed) || self.started.elapsed() > Duration::from_secs(15) {
            return Err(Error::runtime("Plugin cancelled or timed out"));
        }
        if self.commands.len() >= MAX_OPERATIONS {
            return Err(Error::runtime("Plugin exceeds 20,000 edit operations"));
        }
        Ok(())
    }
    fn push(&mut self, command: Cmd) -> LuaResult<()> {
        self.check()?;
        let json_size = |value: serde_json::Value| {
            serde_json::to_vec(&value)
                .map(|v| v.len())
                .unwrap_or(usize::MAX)
        };
        let bytes = match &command {
            Cmd::Pixels { before, after, .. } => before.len().saturating_add(after.len()),
            Cmd::AddShape { shape, .. } => json_size(serde_json::json!(shape)),
            Cmd::RemoveShapes { shapes, .. } => json_size(serde_json::json!(shapes)),
            Cmd::SetGeom { before, after, .. } => json_size(serde_json::json!([before, after])),
            Cmd::SetStyle { before, after, .. } => json_size(serde_json::json!([before, after])),
            Cmd::SetShapeFilters { before, after, .. } => {
                json_size(serde_json::json!([before, after]))
            }
            _ => 1024,
        };
        self.edit_bytes = self.edit_bytes.saturating_add(bytes);
        if self.edit_bytes > 256 * 1024 * 1024 {
            return Err(Error::runtime("Plugin edits exceed 256 MiB"));
        }
        document::apply(&mut self.doc, &command);
        self.commands.push(command);
        Ok(())
    }
    fn layer(&mut self, requested: Option<usize>) -> LuaResult<usize> {
        if let Some(i) = requested
            .or(self.active_layer)
            .filter(|&i| self.doc.layer_editable(i) && self.doc.layers[i].kind.shapes().is_some())
        {
            return Ok(i);
        }
        if requested.is_some() {
            return Err(Error::runtime("Layer is not an editable vector layer"));
        }
        let i = self.doc.layers.len();
        self.push(Cmd::AddLayer {
            index: i,
            layer: Layer::vector("Plugin artwork"),
        })?;
        self.active_layer = Some(i);
        Ok(i)
    }
    fn shape(&self, layer: usize, id: u64) -> LuaResult<Shape> {
        self.check()?;
        self.doc
            .find_shape(layer, id)
            .filter(|_| shape_editable(&self.doc, layer, id))
            .cloned()
            .ok_or_else(|| {
                Error::runtime("Shape or its parent is missing, hidden, locked or a guide")
            })
    }
}
pub(super) fn shape_editable(doc: &Document, layer: usize, id: u64) -> bool {
    if !doc.layer_editable(layer) {
        return false;
    }
    let mut current = Some(id);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) || seen.len() > 64 {
            return false;
        }
        let Some(shape) = doc.find_shape(layer, id) else {
            return false;
        };
        if !shape.visible || shape.locked || shape.guide {
            return false;
        }
        current = shape.layout.parent;
    }
    true
}
fn finite(value: f32) -> LuaResult<f32> {
    if value.is_finite() && value.abs() <= 1_000_000. {
        Ok(value)
    } else {
        Err(Error::runtime(
            "Coordinates must be finite and within ±1,000,000",
        ))
    }
}
fn num(t: &Table, key: &str, default: f32) -> LuaResult<f32> {
    finite(t.get::<Option<f32>>(key)?.unwrap_or(default))
}
fn color(s: &str) -> LuaResult<Rgba> {
    Rgba::parse_hex(s.trim_start_matches('#'))
        .ok_or_else(|| Error::runtime("Use a hex color such as #BAC2DE or #BAC2DE80"))
}
fn checked_geometry(geom: &Geom) -> LuaResult<()> {
    let bytes = serde_json::to_vec(geom).map_err(Error::external)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::runtime("Geometry exceeds 4 MiB"));
    }
    match geom {
        Geom::Polygon { sides, .. } if *sides > 4096 => {
            return Err(Error::runtime("Too many polygon sides"));
        }
        Geom::Star { points, .. } if *points > 2048 => {
            return Err(Error::runtime("Too many star points"));
        }
        Geom::Text(_) => return Err(Error::runtime("Use native text tools for text geometry")),
        Geom::Poly { contours, .. } => {
            for p in contours.iter().flatten() {
                finite(p.x)?;
                finite(p.y)?;
            }
        }
        _ => (),
    }
    let b = geom.bbox();
    for n in [b.min.x, b.min.y, b.max.x, b.max.y] {
        finite(n)?;
    }
    for a in geom.anchors() {
        for n in [
            a.pt.x, a.pt.y, a.h_in.x, a.h_in.y, a.h_out.x, a.h_out.y, a.radius,
        ] {
            finite(n)?;
        }
    }
    Ok(())
}
pub(super) fn context(lua: &Lua, state: &Shared, gesture: Option<Gesture>) -> LuaResult<Table> {
    let s = state.borrow();
    let t = lua.create_table()?;
    t.set("api", API_VERSION)?;
    t.set("width", s.doc.width)?;
    t.set("height", s.doc.height)?;
    t.set("name", s.doc.name.clone())?;
    t.set("active_layer", s.active_layer)?;
    let items: Vec<_> = s.selection.iter().filter_map(|&(layer,id)| s.doc.find_shape(layer,id).map(|shape| serde_json::json!({"layer":layer,"id":id,"name":shape.name,"geom":shape.geom,"style":shape.style,"rotation":shape.rotation}))).collect();
    t.set("selection", lua.to_value(&items)?)?;
    let layers: Vec<_> = s.doc.layers.iter().enumerate().map(|(index,l)| serde_json::json!({"index":index,"name":l.name,"locked":l.locked,"visible":l.visible,"raster":l.kind.pixels().map(|p|[p.w,p.h])})).collect();
    t.set("layers", lua.to_value(&layers)?)?;
    t.set("gesture", lua.to_value(&gesture)?)?;
    Ok(t)
}
pub(super) fn register(lua: &Lua, state: Shared) -> LuaResult<()> {
    let api = lua.create_table()?;
    api.set("api", API_VERSION)?;
    api.set(
        "color",
        lua.create_function(|lua, value: String| lua.to_value(&color(&value)?))?,
    )?;
    let s = state.clone();
    api.set(
        "message",
        lua.create_function(move |_, text: String| {
            if text.len() > 4096 {
                return Err(Error::runtime("Message too long"));
            }
            s.borrow_mut().message = text;
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "add_shape",
        lua.create_function(move |lua, t: Table| {
            let x = num(&t, "x", 0.)?;
            let y = num(&t, "y", 0.)?;
            let w = num(&t, "width", 100.)?.max(0.);
            let h = num(&t, "height", 100.)?.max(0.);
            let kind = t
                .get::<Option<String>>("kind")?
                .unwrap_or_else(|| "rect".into());
            let geom = match kind.as_str() {
                "rect" => Geom::Rect {
                    origin: Pt::new(x, y),
                    size: Pt::new(w, h),
                    radius: num(&t, "radius", 0.)?.max(0.),
                },
                "ellipse" => Geom::Ellipse {
                    center: Pt::new(x + w * 0.5, y + h * 0.5),
                    radii: Pt::new(w * 0.5, h * 0.5),
                },
                "line" => Geom::Line {
                    a: Pt::new(x, y),
                    b: Pt::new(x + w, y + h),
                },
                "path" => {
                    let value = t.get::<Value>("points")?;
                    let points: Vec<[f32; 2]> = lua.from_value(value)?;
                    if points.len() > 100_000 {
                        return Err(Error::runtime("Too many path points"));
                    }
                    let anchors = points
                        .into_iter()
                        .map(|[x, y]| {
                            Ok(crate::geom::Anchor::corner(Pt::new(finite(x)?, finite(y)?)))
                        })
                        .collect::<LuaResult<Vec<_>>>()?;
                    Geom::Path {
                        anchors,
                        closed: t.get::<Option<bool>>("closed")?.unwrap_or(false),
                    }
                }
                "geometry" => lua.from_value(t.get("geom")?)?,
                _ => {
                    return Err(Error::runtime(
                        "Shape kind must be rect, ellipse, line, path or geometry",
                    ));
                }
            };
            checked_geometry(&geom)?;
            let mut style = Style::default();
            if let Some(c) = t.get::<Option<String>>("fill")? {
                style.fill = if c == "none" {
                    Fill::None
                } else {
                    Fill::Solid(color(&c)?)
                };
            }
            if let Some(c) = t.get::<Option<String>>("stroke")? {
                style.stroke = Some(Stroke {
                    color: color(&c)?,
                    width: num(&t, "stroke_width", 1.)?.clamp(0., 10000.),
                    ..Stroke::default()
                });
            }
            let mut shape = Shape::new(geom, style);
            if let Some(name) = t.get::<Option<String>>("name")? {
                shape.name = name;
            }
            let mut s = s.borrow_mut();
            let layer = s.layer(t.get("layer")?)?;
            let id = shape.id;
            s.push(Cmd::AddShape { layer, shape })?;
            s.selection.push((layer, id));
            Ok((layer, id))
        })?,
    )?;
    let s = state.clone();
    api.set(
        "translate",
        lua.create_function(move |_, (layer, id, dx, dy): (usize, u64, f32, f32)| {
            let mut s = s.borrow_mut();
            let shape = s.shape(layer, id)?;
            let mut after = shape.geom.clone();
            after.translate(Pt::new(finite(dx)?, finite(dy)?));
            // Moving existing live text keeps its font and contents; scripts cannot
            // inject a font path through add_shape or set_geometry.
            if let Geom::Text(text) = &after {
                finite(text.origin.x)?;
                finite(text.origin.y)?;
            } else {
                checked_geometry(&after)?;
            }
            s.push(Cmd::SetGeom {
                layer,
                id,
                before: shape.geom,
                after,
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            })
        })?,
    )?;
    let s = state.clone();
    api.set(
        "set_geometry",
        lua.create_function(move |lua, (layer, id, value): (usize, u64, Value)| {
            let after: Geom = lua.from_value(value)?;
            checked_geometry(&after)?;
            let mut s = s.borrow_mut();
            let shape = s.shape(layer, id)?;
            s.push(Cmd::SetGeom {
                layer,
                id,
                before: shape.geom,
                after,
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            })
        })?,
    )?;
    let s = state.clone();
    api.set(
        "remove",
        lua.create_function(move |_, (layer, id): (usize, u64)| {
            let mut s = s.borrow_mut();
            let shape = s.shape(layer, id)?;
            s.push(Cmd::RemoveShapes {
                layer,
                shapes: vec![shape],
            })?;
            s.selection.retain(|item| *item != (layer, id));
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "set_fill",
        lua.create_function(move |lua, (layer, id, value): (usize, u64, Value)| {
            let fill = match value {
                Value::String(v) => {
                    let v = v.to_str()?;
                    if &*v == "none" {
                        Fill::None
                    } else {
                        Fill::Solid(color(&v)?)
                    }
                }
                v => lua.from_value(v)?,
            };
            if let Some(g) = fill.gradient() {
                if g.stops.len() > 256 || g.stops.len() < 2 {
                    return Err(Error::runtime("Gradients need 2–256 stops"));
                }
                for n in g
                    .from
                    .into_iter()
                    .chain(g.to)
                    .chain(g.stops.iter().map(|s| s.offset))
                {
                    finite(n)?;
                }
            }
            let mut s = s.borrow_mut();
            let shape = s.shape(layer, id)?;
            let mut style = shape.style.clone();
            style.fill = fill;
            s.push(Cmd::SetStyle {
                layer,
                id,
                before: shape.style,
                after: style,
            })
        })?,
    )?;
    api.set(
        "gradient",
        lua.create_function(|lua, (colors, kind): (Vec<String>, Option<String>)| {
            if colors.len() < 2 || colors.len() > 256 {
                return Err(Error::runtime("Gradients need 2–256 colors"));
            }
            let kind = match kind.as_deref().unwrap_or("linear") {
                "linear" => GradientKind::Linear,
                "radial" => GradientKind::Radial,
                "conic" => GradientKind::Conic,
                "shape" => GradientKind::Shape,
                _ => return Err(Error::runtime("Unknown gradient kind")),
            };
            let mut gradient =
                Gradient::new(kind, color(&colors[0])?, color(colors.last().unwrap())?);
            gradient.stops = colors
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    Ok(crate::gradient::GradientStop {
                        offset: i as f32 / (colors.len() - 1) as f32,
                        color: color(c)?,
                    })
                })
                .collect::<LuaResult<_>>()?;
            lua.to_value(&Fill::Gradient(gradient))
        })?,
    )?;
    let s = state.clone();
    api.set(
        "set_effects",
        lua.create_function(move |lua, (layer, id, value): (usize, u64, Value)| {
            let items: Vec<crate::filter::Fx> = lua.from_value(value)?;
            if items.len() > 32 {
                return Err(Error::runtime("Limit effects to 32 per shape"));
            }
            for fx in &items {
                use crate::filter::Fx;
                let within = |n: f32, limit: f32| n.is_finite() && n.abs() <= limit;
                let valid = match fx {
                    Fx::Blur { std } => within(*std, 512.) && *std >= 0.,
                    Fx::Shadow { dx, dy, blur, .. } | Fx::InnerShadow { dx, dy, blur, .. } => {
                        within(*dx, 4096.)
                            && within(*dy, 4096.)
                            && within(*blur, 512.)
                            && *blur >= 0.
                    }
                    Fx::Offset { dx, dy } => within(*dx, 4096.) && within(*dy, 4096.),
                    Fx::Morphology { radius, .. } => within(*radius, 64.) && *radius >= 0.,
                    Fx::Turbulence { base, octaves, .. } => {
                        within(*base, 1.) && *base > 0. && *octaves <= 8
                    }
                    Fx::Displacement { scale, x_ch, y_ch } => {
                        within(*scale, 4096.) && *x_ch < 4 && *y_ch < 4
                    }
                    Fx::ColorMatrix { values } => values.iter().all(|n| within(*n, 100.)),
                    Fx::HueRotate { degrees } => within(*degrees, 36000.),
                    Fx::Saturate { amount }
                    | Fx::Brightness { amount }
                    | Fx::Contrast { amount }
                    | Fx::Invert { amount } => within(*amount, 100.),
                };
                if !valid {
                    return Err(Error::runtime("Effect parameters exceed supported limits"));
                }
            }
            let mut s = s.borrow_mut();
            let shape = s.shape(layer, id)?;
            s.push(Cmd::SetShapeFilters {
                layer,
                id,
                before: shape.filters,
                after: crate::filter::FilterStack {
                    enabled: true,
                    items,
                },
            })
        })?,
    )?;
    let s = state.clone();
    api.set(
        "brush",
        lua.create_function(move |_, t: Table| {
            let mut s = s.borrow_mut();
            s.brush = Some(Brush {
                size: num(&t, "size", 24.)?.clamp(1., 2048.),
                hardness: num(&t, "hardness", 0.75)?.clamp(0., 1.),
                opacity: num(&t, "opacity", 1.)?.clamp(0., 1.),
                flow: num(&t, "flow", 0.85)?.clamp(0., 1.),
                spacing: num(&t, "spacing", 0.18)?.clamp(0.05, 4.),
                color: color(
                    &t.get::<Option<String>>("color")?
                        .unwrap_or_else(|| "#BAC2DE".into()),
                )?,
            });
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "palette",
        lua.create_function(move |_, (name, colors): (String, Vec<String>)| {
            if colors.len() > 4096 {
                return Err(Error::runtime("Too many swatches"));
            }
            let palette = Palette::new(
                name,
                colors.iter().map(|c| color(c)).collect::<LuaResult<_>>()?,
            );
            crate::palette::encode(std::slice::from_ref(&palette)).map_err(Error::external)?;
            let mut s = s.borrow_mut();
            if s.palettes.len() >= 256 {
                return Err(Error::runtime("Too many palettes"));
            }
            s.palettes.push(palette);
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "read_asset",
        lua.create_function(move |_, name: String| {
            let s = s.borrow();
            s.check()?;
            let root = s.root.canonicalize().map_err(Error::external)?;
            let path = root.join(name).canonicalize().map_err(Error::external)?;
            if !path.starts_with(&root)
                || std::fs::metadata(&path).map_err(Error::external)?.len() > 4 * 1024 * 1024
            {
                return Err(Error::runtime(
                    "Assets must be inside this plugin and at most 4 MiB",
                ));
            }
            std::fs::read_to_string(path).map_err(Error::external)
        })?,
    )?;
    let s = state.clone();
    api.set(
        "svg",
        lua.create_function(move |_, (svg, x, y, width): (String, f32, f32, f32)| {
            // SVG plugin assets cannot fetch local files or remote resources.
            if svg.len() > 4 * 1024 * 1024 {
                return Err(Error::runtime("SVG exceeds 4 MiB"));
            }
            let xml = roxmltree::Document::parse(&svg).map_err(Error::external)?;
            for n in xml.descendants() {
                for a in n.attributes() {
                    if a.name() == "href" && !a.value().starts_with('#') {
                        return Err(Error::runtime(
                            "SVG assets must embed their paths, without external references",
                        ));
                    }
                }
            }
            // CSS is unnecessary for bundled path icons and can obscure URLs with escapes.
            if xml.descendants().any(|n| n.has_tag_name("style")) || svg.contains("\\") {
                return Err(Error::runtime(
                    "SVG assets must use inline attributes without stylesheets or CSS escapes",
                ));
            }
            for suffix in svg.to_ascii_lowercase().split("url(").skip(1) {
                let target = suffix
                    .split(')')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches(['\'', '"']);
                if !target.starts_with('#') {
                    return Err(Error::runtime("External SVG URLs are disabled"));
                }
            }
            let (doc, _) =
                crate::formats::svg::read(&svg, "Plugin asset").map_err(Error::external)?;
            let x = finite(x)?;
            let y = finite(y)?;
            let width = finite(width)?.clamp(1., 100000.);
            let src = crate::geom::Bounds::from_min_size(Pt::ZERO, Pt::new(doc.width, doc.height));
            let dst = crate::geom::Bounds::from_min_size(
                Pt::new(x, y),
                Pt::new(width, width * doc.height / doc.width.max(1.)),
            );
            let mut s = s.borrow_mut();
            let layer = s.layer(None)?;
            for source in &doc.layers {
                if let Some(shapes) = source.kind.shapes() {
                    for shape in shapes {
                        let mut shape = shape.clone();
                        shape.id = document::next_id();
                        shape.geom.map_into(src, dst);
                        shape.layout.parent = None;
                        checked_geometry(&shape.geom)?;
                        let id = shape.id;
                        s.push(Cmd::AddShape { layer, shape })?;
                        s.selection.push((layer, id));
                    }
                }
            }
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "pixel",
        lua.create_function(move |_, (layer, x, y): (usize, i64, i64)| {
            let s = s.borrow();
            s.check()?;
            let pixels = s
                .doc
                .layers
                .get(layer)
                .and_then(|l| l.kind.pixels())
                .ok_or_else(|| Error::runtime("Choose a raster layer"))?;
            if x < 0 || y < 0 || x >= pixels.w as i64 || y >= pixels.h as i64 {
                return Ok((0, 0, 0, 0));
            }
            let i = (y as usize * pixels.w as usize + x as usize) * 4;
            Ok((
                pixels.data[i],
                pixels.data[i + 1],
                pixels.data[i + 2],
                pixels.data[i + 3],
            ))
        })?,
    )?;
    let s = state.clone();
    api.set(
        "map_pixels",
        lua.create_function(move |_, (layer, function): (usize, Function)| {
            let (w, h, before) = {
                let s = s.borrow();
                s.check()?;
                if !s.doc.layer_editable(layer) {
                    return Err(Error::runtime(
                        "Raster layer or its parent is hidden or locked",
                    ));
                }
                let l = s
                    .doc
                    .layers
                    .get(layer)
                    .filter(|l| l.visible && !l.locked)
                    .ok_or_else(|| Error::runtime("Raster layer missing, hidden or locked"))?;
                let p = l
                    .kind
                    .pixels()
                    .ok_or_else(|| Error::runtime("Choose a raster layer"))?;
                if p.data.len() > 128 * 1024 * 1024 {
                    return Err(Error::runtime("Raster filter input exceeds 128 MiB"));
                }
                (p.w, p.h, p.data.clone())
            };
            let mut after = before.clone();
            for (i, pixel) in after.chunks_exact_mut(4).enumerate() {
                if i % 1024 == 0 {
                    s.borrow().check()?;
                }
                let (r, g, b, a): (f64, f64, f64, f64) = function.call((
                    pixel[0],
                    pixel[1],
                    pixel[2],
                    pixel[3],
                    i as u32 % w,
                    i as u32 / w,
                ))?;
                for (target, value) in pixel.iter_mut().zip([r, g, b, a]) {
                    if !value.is_finite() {
                        return Err(Error::runtime("Filter returned a nonfinite channel"));
                    }
                    *target = value.round().clamp(0., 255.) as u8;
                }
            }
            let _ = h;
            s.borrow_mut().push(Cmd::Pixels {
                layer,
                mask: false,
                before,
                after,
            })
        })?,
    )?;
    lua.globals().set("oma", api)?;
    Ok(())
}
