//! Native, validated editor operations exposed to the connected agent.
use crate::{
    app::Studio,
    color::Rgba,
    document::{Cmd, Fill, Layer, Shape, Stroke, Style},
    geom::{Anchor, Geom, Pt},
};
use base64::Engine;
use serde_json::{Value, json};

pub fn catalog() -> Vec<Value> {
    let number = json!({"type":"number"});
    let integer = json!({"type":"integer","minimum":0});
    let string = json!({"type":"string"});
    let shape = json!({"type":"object","additionalProperties":false,"properties":{
        "kind":{"type":"string","enum":["rect","ellipse","line","path","text","frame"]},
        "name":string,"x":number,"y":number,"width":number,"height":number,"radius":number,
        "fill":string,"stroke":string,"stroke_width":number,"rotation":number,"opacity":number,
        "text":string,"font":string,"font_size":number,"tracking":number,"leading":number,
        "align":{"type":"string","enum":["start","center","end"]},"parent":integer,
        "points":{"type":"array","items":{"type":"array","items":number,"minItems":2,"maxItems":2},"maxItems":4096},
        "closed":{"type":"boolean"},"gradient":{"type":"array","items":string,"minItems":2,"maxItems":32},
        "gradient_kind":{"type":"string","enum":["linear","radial","conic","shape"]}
    }});
    let specs = vec![
        (
            "get_document",
            "Read live dimensions, selection, revision and up to 50 layers with 20 object summaries each. Use layer_offset and get_objects to inspect more. Call before editing; user edits can change the revision.",
            json!({"layer_offset":integer}),
            vec![],
            false,
        ),
        (
            "get_objects",
            "Inspect native geometry and styling of up to 50 objects. Layer is an index; IDs remain stable. Use next_offset for pagination. Pixel masks are summarized, and geometry exceeding 4096 points is omitted; use a canvas snapshot to inspect it.",
            json!({"layer":integer,"offset":integer}),
            vec!["layer"],
            false,
        ),
        (
            "add_shape",
            "Add one editable native rectangle, ellipse, path, line, frame or text object, immediately visible on the canvas. Text uses top-left x/y, optional wrap width and installed font name. Rotation is degrees. Fill/stroke are hex colors or none. Gradients are arrays of hex colors. Do not send SVG or files. Returns ID and next revision.",
            json!({"layer":integer,"shape":shape}),
            vec!["layer", "shape"],
            true,
        ),
        (
            "update_shape",
            "Update an existing native object's position, dimensions, text, fill, gradient, stroke, rotation or opacity. Supply only changed properties. Preserves its stable ID and editability.",
            json!({"layer":integer,"id":integer,"changes":shape}),
            vec!["layer", "id", "changes"],
            true,
        ),
        (
            "remove_shapes",
            "Remove specified unlocked objects, as one undoable edit. Never clears unrelated artwork.",
            json!({"layer":integer,"ids":{"type":"array","items":integer,"minItems":1,"maxItems":100}}),
            vec!["layer", "ids"],
            true,
        ),
        (
            "create_layer",
            "Create an editable vector layer and return its index. Use separate layers for meaningful parts of a composition.",
            json!({"name":string}),
            vec!["name"],
            true,
        ),
        (
            "update_layer",
            "Rename an editable layer or change its opacity (0–1).",
            json!({"layer":integer,"name":string,"opacity":number}),
            vec!["layer"],
            true,
        ),
        (
            "set_effects",
            "Set native editable blur and drop-shadow effects on an object. Omitted effects are removed. Radius limits protect live rendering.",
            json!({"layer":integer,"id":integer,"blur":number,"shadow":{"type":"object","properties":{"x":number,"y":number,"blur":number,"color":string},"required":["color"]}}),
            vec!["layer", "id"],
            true,
        ),
        (
            "select_objects",
            "Highlight objects on the live canvas for the user.",
            json!({"layer":integer,"ids":{"type":"array","items":integer,"maxItems":100}}),
            vec!["layer", "ids"],
            false,
        ),
        (
            "list_fonts",
            "List installed fonts by name. Use a returned name for editable text, never a guessed file path.",
            json!({"query":string}),
            vec![],
            false,
        ),
        (
            "get_canvas_snapshot",
            "See the current rendered design as a PNG image, up to 960 pixels wide. Inspect composition after meaningful changes.",
            json!({}),
            vec![],
            false,
        ),
        (
            "get_documentation",
            "Read version-matched Omadesign documentation for learning or design work.",
            json!({"topic":{"type":"string","enum":["manual","layout","tools"]}}),
            vec!["topic"],
            false,
        ),
    ];
    specs.into_iter().map(|(name, description, mut properties, mut required, mutation)| {
        if mutation { properties["revision"] = integer.clone(); required.push("revision"); }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":!mutation,"openWorldHint":false}})
    }).collect()
}

pub fn mutates(name: &str) -> bool {
    matches!(
        name,
        "add_shape"
            | "update_shape"
            | "remove_shapes"
            | "create_layer"
            | "update_layer"
            | "set_effects"
    )
}

fn text(value: &Value, key: &str, default: &str) -> Result<String, String> {
    let s = match value.get(key) {
        Some(v) => v.as_str().ok_or_else(|| format!("{key} must be text"))?,
        None => default,
    };
    if s.len() > 16_384 {
        return Err(format!("{key} is too long"));
    }
    Ok(s.into())
}
fn number(value: &Value, key: &str, default: f32) -> Result<f32, String> {
    let v = match value.get(key) {
        Some(v) => v.as_f64().ok_or_else(|| format!("{key} must be numeric"))? as f32,
        None => default,
    };
    if !v.is_finite() || v.abs() > 100_000.0 {
        return Err(format!("{key} must be finite and within ±100000"));
    }
    Ok(v)
}
fn index(args: &Value, key: &str) -> Result<usize, String> {
    args[key]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("Missing integer {key}"))
}
fn color(value: &str) -> Result<Rgba, String> {
    Rgba::parse_hex(value.trim_start_matches('#'))
        .ok_or_else(|| "Use a hex color such as #89B4FA".into())
}
fn layer(studio: &Studio, i: usize) -> Result<(), String> {
    if !studio.doc.layer_editable(i)
        || studio
            .doc
            .layers
            .get(i)
            .is_none_or(|l| l.kind.shapes().is_none())
    {
        return Err("Choose an unlocked, visible vector layer".into());
    }
    Ok(())
}
fn object(studio: &Studio, i: usize, id: u64) -> Result<Shape, String> {
    layer(studio, i)?;
    let shape = studio
        .doc
        .find_shape(i, id)
        .ok_or("Object no longer exists")?;
    let mut current = Some(id);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) || seen.len() > 64 {
            return Err("Invalid object hierarchy".into());
        }
        let s = studio.doc.find_shape(i, id).ok_or("Missing parent")?;
        if !s.visible || s.locked || s.guide {
            return Err("Object or parent is hidden, locked or a guide".into());
        }
        current = s.layout.parent;
    }
    Ok(shape.clone())
}
fn native_shape(
    studio: &Studio,
    li: usize,
    spec: &Value,
    original: Option<&Shape>,
) -> Result<Shape, String> {
    if !spec.is_object() {
        return Err("Shape properties must be an object".into());
    }
    let bounds = original.map(|s| s.geom.bbox());
    let x = number(spec, "x", bounds.map_or(0.0, |b| b.min.x))?;
    let y = number(spec, "y", bounds.map_or(0.0, |b| b.min.y))?;
    let width = number(spec, "width", bounds.map_or(100.0, |b| b.width()))?;
    let height = number(spec, "height", bounds.map_or(100.0, |b| b.height()))?;
    if width < 0.0 || height < 0.0 {
        return Err("Dimensions cannot be negative".into());
    }
    let fallback = match original.map(|s| &s.geom) {
        Some(Geom::Ellipse { .. }) => "ellipse",
        Some(Geom::Line { .. }) => "line",
        Some(Geom::Path { .. }) => "path",
        Some(Geom::Text(_)) => "text",
        _ => "rect",
    };
    let kind = text(spec, "kind", fallback)?;
    let geometry_changed = original.is_none()
        || [
            "kind",
            "x",
            "y",
            "width",
            "height",
            "radius",
            "text",
            "font",
            "font_size",
            "tracking",
            "leading",
            "align",
            "points",
            "closed",
        ]
        .iter()
        .any(|key| spec.get(key).is_some());
    let only_bounds = original.is_some()
        && !matches!(original.map(|s| &s.geom), Some(Geom::Text(_)))
        && !["kind", "radius", "points", "closed"]
            .iter()
            .any(|k| spec.get(k).is_some());
    let geom = if !geometry_changed {
        original.unwrap().geom.clone()
    } else if only_bounds {
        let original = original.unwrap();
        let mut geom = original.geom.clone();
        geom.map_into(
            original.geom.bbox(),
            crate::geom::Bounds::from_min_size(Pt::new(x, y), Pt::new(width, height)),
        );
        geom
    } else {
        match kind.as_str() {
            "rect" | "frame" => Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(width, height),
                radius: number(
                    spec,
                    "radius",
                    original
                        .and_then(|s| {
                            if let Geom::Rect { radius, .. } = s.geom {
                                Some(radius)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0.0),
                )?
                .max(0.0),
            },
            "ellipse" => Geom::Ellipse {
                center: Pt::new(x + width / 2.0, y + height / 2.0),
                radii: Pt::new(width / 2.0, height / 2.0),
            },
            "line" => Geom::Line {
                a: Pt::new(x, y),
                b: Pt::new(x + width, y + height),
            },
            "text" => {
                let mut run = original
                    .and_then(|s| {
                        if let Geom::Text(t) = &s.geom {
                            Some(t.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                // Text origins are baselines; tools expose top-left coordinates.
                let old_px = run.px;
                run.px = number(spec, "font_size", run.px)?.clamp(1.0, 2048.0);
                run.content = text(spec, "text", &run.content)?;
                if let Some(font) = spec.get("font") {
                    let name = font.as_str().ok_or("Font must be an installed font name")?;
                    run.font = if name.is_empty() {
                        String::new()
                    } else {
                        crate::text::all_fonts_cached()
                            .iter()
                            .find(|f| f.name.eq_ignore_ascii_case(name))
                            .ok_or("Font is not installed; use list_fonts")?
                            .path
                            .to_string_lossy()
                            .into_owned()
                    };
                }
                run.origin = if let Some(Geom::Text(old)) = original.map(|s| &s.geom) {
                    Pt::new(
                        if spec.get("x").is_some() {
                            x
                        } else {
                            old.origin.x
                        },
                        if spec.get("y").is_some() {
                            y + run.px
                        } else {
                            old.origin.y + run.px - old_px
                        },
                    )
                } else {
                    Pt::new(x, y + run.px)
                };
                if spec.get("width").is_some() {
                    run.wrap_width = (width > 0.0).then_some(width);
                }
                run.tracking = number(spec, "tracking", run.tracking)?;
                run.leading = number(spec, "leading", run.leading)?.max(0.0);
                if let Some(a) = spec.get("align") {
                    run.align = match a.as_str() {
                        Some("start") => crate::geom::TextAlign::Start,
                        Some("center") => crate::geom::TextAlign::Center,
                        Some("end") => crate::geom::TextAlign::End,
                        _ => return Err("Unknown text alignment".into()),
                    };
                }
                let mut g = Geom::Text(run);
                crate::text::fill_contours(&mut g);
                g
            }
            "path" => {
                let anchors = if let Some(points) = spec.get("points") {
                    let points = points
                        .as_array()
                        .ok_or("Points must be an array of [x,y]")?;
                    if !(2..=4096).contains(&points.len()) {
                        return Err("Paths need 2–4096 points".into());
                    }
                    points
                        .iter()
                        .map(|p| {
                            if p.as_array().is_none_or(|p| p.len() != 2) {
                                return Err("Each point needs x and y".into());
                            }
                            let v = json!({"x":p[0],"y":p[1]});
                            Ok(Anchor::corner(Pt::new(
                                number(&v, "x", 0.0)?,
                                number(&v, "y", 0.0)?,
                            )))
                        })
                        .collect::<Result<Vec<_>, String>>()?
                } else if let Some(Geom::Path { anchors, .. }) = original.map(|s| &s.geom) {
                    anchors.clone()
                } else {
                    return Err("Path points are required".into());
                };
                Geom::Path {
                    anchors,
                    closed: spec
                        .get("closed")
                        .and_then(Value::as_bool)
                        .unwrap_or_else(|| original.is_some_and(|s| s.geom.is_closed())),
                }
            }
            _ => return Err("Unknown native shape kind".into()),
        }
    };
    let mut shape = original.cloned().unwrap_or_else(|| {
        Shape::new(
            geom.clone(),
            Style {
                fill: Fill::Solid(Rgba::BLACK),
                stroke: None,
            },
        )
    });
    shape.geom = geom;
    shape.name = text(spec, "name", &shape.name)?;
    if let Some(fill) = spec.get("fill") {
        let fill = fill.as_str().ok_or("Fill must be hex or none")?;
        shape.style.fill = if fill == "none" {
            Fill::None
        } else {
            Fill::Solid(color(fill)?)
        };
    }
    if let Some(colors) = spec.get("gradient") {
        use crate::gradient::{Gradient, GradientKind, GradientStop};
        let colors = colors.as_array().ok_or("Gradient must be an array")?;
        if !(2..=32).contains(&colors.len()) {
            return Err("Gradients need 2–32 colors".into());
        }
        let colors = colors
            .iter()
            .map(|v| color(v.as_str().unwrap_or_default()))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match spec["gradient_kind"].as_str().unwrap_or("linear") {
            "linear" => GradientKind::Linear,
            "radial" => GradientKind::Radial,
            "conic" => GradientKind::Conic,
            "shape" => GradientKind::Shape,
            _ => return Err("Unknown gradient kind".into()),
        };
        let mut gradient = Gradient::new(kind, colors[0], colors[colors.len() - 1]);
        gradient.stops = colors
            .iter()
            .enumerate()
            .map(|(i, c)| GradientStop {
                offset: i as f32 / (colors.len() - 1) as f32,
                color: *c,
            })
            .collect();
        shape.style.fill = Fill::Gradient(gradient);
    }
    if let Some(stroke) = spec.get("stroke") {
        let stroke = stroke.as_str().ok_or("Stroke must be hex or none")?;
        shape.style.stroke = if stroke == "none" {
            None
        } else {
            Some(Stroke {
                color: color(stroke)?,
                width: number(spec, "stroke_width", 1.0)?.clamp(0.0, 1000.0),
                ..Default::default()
            })
        };
    } else if let Some(stroke) = &mut shape.style.stroke {
        stroke.width = number(spec, "stroke_width", stroke.width)?.clamp(0.0, 1000.0);
    }
    shape.rotation = number(spec, "rotation", shape.rotation.to_degrees())?.to_radians();
    shape.opacity = number(spec, "opacity", shape.opacity)?.clamp(0.0, 1.0);
    if kind == "frame" {
        shape.layout = crate::layout::FrameLayout::frame();
    }
    if spec.get("parent").is_some() {
        let id = index(spec, "parent")? as u64;
        let parent = object(studio, li, id)?;
        if parent.id == shape.id || !parent.layout.frame {
            return Err("Parent must be a different editable frame".into());
        }
        let mut ancestor = Some(parent.id);
        for _ in 0..65 {
            if ancestor == Some(shape.id) {
                return Err("Cannot create a frame cycle".into());
            }
            ancestor = ancestor
                .and_then(|id| studio.doc.find_shape(li, id))
                .and_then(|s| s.layout.parent);
            if ancestor.is_none() {
                break;
            }
        }
        shape.layout.parent = Some(id);
    }
    Ok(shape)
}

pub fn execute(
    studio: &mut Studio,
    name: &str,
    args: &Value,
    editable: bool,
) -> Result<Value, String> {
    if mutates(name) {
        if !editable {
            return Err("This session is read-only. Enable live edits in the agent panel to modify the design.".into());
        }
        if args["revision"].as_u64() != Some(studio.canvas_gen) {
            return Err(format!(
                "Canvas changed. Call get_document and retry with revision {}.",
                studio.canvas_gen
            ));
        }
        if studio.op.is_some() || studio.type_edit.is_some() || studio.deformation.is_some() || studio.pixel_edit.is_some() {
            return Err(
                "The designer is editing the canvas. Retry after the gesture finishes.".into(),
            );
        }
    }
    let result = match name {
        "get_document" => {
            let offset = args["layer_offset"].as_u64().unwrap_or(0) as usize;
            let layers: Vec<_> = studio
                .doc
                .layers
                .iter()
                .enumerate()
                .skip(offset)
                .take(50)
                .map(|(i, l)| {
                    json!({"index":i,"name":l.name.chars().take(200).collect::<String>(),
                    "editable":studio.doc.layer_editable(i),"vector":l.kind.shapes().is_some(),
                    "objects":l.kind.shapes().map_or(0,|s|s.len()),
                    "items":l.kind.shapes().map(|s|s.iter().take(20).map(|s|json!({
                        "id":s.id,"name":s.name.chars().take(200).collect::<String>(),
                        "bounds":s.world_bbox(),"locked":s.locked
                    })).collect::<Vec<_>>())})
                })
                .collect();
            let next = offset.saturating_add(layers.len());
            json!({"name":studio.doc.name.chars().take(200).collect::<String>(),
                "width":studio.doc.width,"height":studio.doc.height,"revision":studio.canvas_gen,
                "selection":studio.selection.iter().take(100).collect::<Vec<_>>(),
                "selection_count":studio.selection.len(),"active_layer":studio.active_layer,
                "layers":layers,"total_layers":studio.doc.layers.len(),
                "next_layer_offset":(next<studio.doc.layers.len()).then_some(next)})
        }
        "get_objects" => {
            let l = studio
                .doc
                .layers
                .get(index(args, "layer")?)
                .ok_or("Missing layer")?;
            let shapes = l.kind.shapes().ok_or("Not a vector layer")?;
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            let mut objects = vec![];
            let mut bytes = 0;
            for shape in shapes.iter().skip(offset).take(50) {
                let points = match &shape.geom {
                    Geom::Path { anchors, .. } => anchors.len(),
                    Geom::Paths { paths, .. } => paths.iter().map(|p| p.anchors.len()).sum(),
                    Geom::Poly { contours, .. } => contours.iter().map(Vec::len).sum(),
                    Geom::Text(run) if run.content.len() > 64_000 => usize::MAX,
                    _ => 0,
                };
                let item = json!({"id":shape.id,"name":shape.name.chars().take(200).collect::<String>(),
                    "bounds":shape.world_bbox(),"geom":(points<=4096).then_some(&shape.geom),
                    "geometry_omitted":points>4096,"style":shape.style,"rotation":shape.rotation,
                    "opacity":shape.opacity,"blend":shape.blend,"visible":shape.visible,
                    "locked":shape.locked,"guide":shape.guide,"filters":shape.filters,
                    "corners":shape.corners,"layout":shape.layout,
                    "mask":shape.mask.as_ref().map(|m|json!({"width":m.w,"height":m.h}))});
                let size = serde_json::to_vec(&item).map_err(|e| e.to_string())?.len();
                if bytes + size > 1024 * 1024 {
                    if objects.is_empty() {
                        return Err("Object details exceed the context limit; inspect a canvas snapshot instead".into());
                    }
                    break;
                }
                bytes += size;
                objects.push(item);
            }
            let next = offset.saturating_add(objects.len());
            json!({"objects":objects,"total":shapes.len(),"next_offset":(next<shapes.len()).then_some(next),"revision":studio.canvas_gen})
        }
        "add_shape" => {
            let li = index(args, "layer")?;
            layer(studio, li)?;
            if studio.doc.layers[li].kind.shapes().unwrap().len() >= 20_000 {
                return Err("Layer object limit reached".into());
            }
            let shape = native_shape(studio, li, &args["shape"], None)?;
            let id = shape.id;
            studio.commit(Cmd::AddShape { layer: li, shape });
            studio.selection = vec![(li, id)];
            studio.active_layer = Some(li);
            json!({"id":id,"layer":li,"revision":studio.canvas_gen})
        }
        "update_shape" => {
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            let before = object(studio, li, id)?;
            let after = native_shape(studio, li, &args["changes"], Some(&before))?;
            studio.commit(Cmd::Batch(vec![
                Cmd::SetGeom {
                    layer: li,
                    id,
                    before: before.geom,
                    after: after.geom,
                    rot_before: before.rotation,
                    rot_after: after.rotation,
                },
                Cmd::SetStyle {
                    layer: li,
                    id,
                    before: before.style,
                    after: after.style,
                },
                Cmd::SetOpacity {
                    layer: li,
                    id,
                    before: before.opacity,
                    after: after.opacity,
                },
                Cmd::SetShapeMeta {
                    layer: li,
                    id,
                    name: after.name,
                    visible: before.visible,
                    locked: before.locked,
                    before: (before.name, before.visible, before.locked),
                },
                Cmd::SetLayout {
                    layer: li,
                    id,
                    before: before.layout,
                    after: after.layout,
                },
            ]));
            studio.selection = vec![(li, id)];
            json!({"id":id,"revision":studio.canvas_gen})
        }
        "remove_shapes" | "select_objects" => {
            let li = index(args, "layer")?;
            let ids = args["ids"].as_array().ok_or("IDs are required")?;
            if ids.len() > 100 {
                return Err("At most 100 objects per call".into());
            }
            let mut unique = std::collections::HashSet::new();
            let mut shapes = ids
                .iter()
                .map(|v| {
                    let id = v.as_u64().ok_or("Invalid object ID")?;
                    if !unique.insert(id) {
                        return Err("Duplicate object ID".into());
                    }
                    object(studio, li, id)
                })
                .collect::<Result<Vec<_>, String>>()?;
            if name == "remove_shapes" {
                // A frame owns its descendants. Validate the entire removal
                // before committing so locked children cannot be orphaned.
                loop {
                    let children: Vec<_> = studio.doc.layers[li]
                        .kind
                        .shapes()
                        .unwrap()
                        .iter()
                        .filter(|s| {
                            !unique.contains(&s.id)
                                && s.layout.parent.is_some_and(|p| unique.contains(&p))
                        })
                        .map(|s| s.id)
                        .collect();
                    if children.is_empty() {
                        break;
                    }
                    if shapes.len() + children.len() > 1000 {
                        return Err("Remove at most 1000 descendants at a time".into());
                    }
                    for id in children {
                        shapes.push(object(studio, li, id)?);
                        unique.insert(id);
                    }
                }
                let before = studio.doc.layers[li].kind.shapes().unwrap().to_vec();
                let after = before
                    .iter()
                    .filter(|s| !unique.contains(&s.id))
                    .cloned()
                    .collect();
                let motion_before = studio.doc.motion.clone();
                let mut motion_after = motion_before.clone();
                motion_after.drop_shapes(&unique.into_iter().collect::<Vec<_>>());
                studio.commit(Cmd::Batch(vec![
                    Cmd::SetVectorShapes {
                        layer: li,
                        before,
                        after,
                    },
                    Cmd::SetMotion {
                        before: motion_before,
                        after: motion_after,
                    },
                ]));
            } else {
                studio.selection = shapes.iter().map(|s| (li, s.id)).collect();
            }
            json!({"revision":studio.canvas_gen})
        }
        "create_layer" => {
            if studio.doc.layers.len() >= 2048 {
                return Err("Layer limit reached".into());
            }
            let i = studio.doc.layers.len();
            let name = text(args, "name", "Agent artwork")?;
            studio.commit(Cmd::AddLayer {
                index: i,
                layer: Layer::vector(name),
            });
            studio.active_layer = Some(i);
            json!({"layer":i,"revision":studio.canvas_gen})
        }
        "update_layer" => {
            let li = index(args, "layer")?;
            layer(studio, li)?;
            let l = &studio.doc.layers[li];
            studio.commit(Cmd::SetLayerMeta {
                index: li,
                name: text(args, "name", &l.name)?,
                visible: l.visible,
                locked: l.locked,
                opacity: number(args, "opacity", l.opacity)?.clamp(0.0, 1.0),
                blend: l.blend,
                before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
            });
            json!({"revision":studio.canvas_gen})
        }
        "set_effects" => {
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            let s = object(studio, li, id)?;
            let mut items = vec![];
            let blur = number(args, "blur", 0.0)?.clamp(0.0, 128.0);
            if blur > 0.0 {
                items.push(crate::filter::Fx::Blur { std: blur });
            }
            if let Some(v) = args.get("shadow") {
                items.push(crate::filter::Fx::Shadow {
                    dx: number(v, "x", 0.0)?.clamp(-2048.0, 2048.0),
                    dy: number(v, "y", 4.0)?.clamp(-2048.0, 2048.0),
                    blur: number(v, "blur", 8.0)?.clamp(0.0, 128.0),
                    color: color(&text(v, "color", "#00000066")?)?,
                });
            }
            studio.commit(Cmd::SetShapeFilters {
                layer: li,
                id,
                before: s.filters,
                after: crate::filter::FilterStack {
                    enabled: true,
                    items,
                },
            });
            json!({"revision":studio.canvas_gen})
        }
        "list_fonts" => {
            let query = text(args, "query", "")?.to_lowercase();
            json!({"fonts":crate::text::all_fonts_cached().iter().filter(|f|f.name.to_lowercase().contains(&query)).take(200).map(|f|&f.name).collect::<Vec<_>>()})
        }
        "get_canvas_snapshot" => {
            let scale = (960.0 / studio.doc.width.max(studio.doc.height).max(1.0)).min(1.0);
            let (w, h) = (
                (studio.doc.width * scale).ceil().max(1.0) as u32,
                (studio.doc.height * scale).ceil().max(1.0) as u32,
            );
            let pixmap = crate::compositor::render_view(
                &studio.doc,
                crate::compositor::View {
                    scale,
                    offset: Pt::ZERO,
                },
                w,
                h,
                crate::compositor::Draft::none(),
            )
            .ok_or("Unable to render canvas")?;
            let bytes = pixmap.encode_png().map_err(|e| e.to_string())?;
            return Ok(
                json!({"content":[{"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)},{"type":"text","text":format!("Canvas revision {} · {} × {}",studio.canvas_gen,w,h)}]}),
            );
        }
        "get_documentation" => {
            let doc = match args["topic"].as_str() {
                Some("manual") => include_str!("../../docs/MANUAL.md"),
                Some("layout") => include_str!("../../docs/layout.md"),
                Some("tools") => {
                    return Ok(
                        json!({"content":[{"type":"text","text":serde_json::to_string(&catalog()).unwrap()}]}),
                    );
                }
                _ => return Err("Choose manual, layout or tools".into()),
            };
            return Ok(json!({"content":[{"type":"text","text":doc}]}));
        }
        _ => return Err(format!("Unknown design tool: {name}")),
    };
    if mutates(name) {
        studio.show_welcome = false;
        studio.status = format!("Agent · {}", name.replace('_', " "));
    }
    Ok(
        json!({"content":[{"type":"text","text":serde_json::to_string(&result).map_err(|e|e.to_string())?}]}),
    )
}
