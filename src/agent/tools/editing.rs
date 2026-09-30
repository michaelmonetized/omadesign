use super::*;
use crate::{document::RASTER_ID, tools::Persona};
pub const ACTIONS: &[&str] = &[
    "undo",
    "redo",
    "duplicate",
    "delete",
    "nudge",
    "align_left",
    "align_center",
    "align_right",
    "align_top",
    "align_middle",
    "align_bottom",
    "distribute_x",
    "distribute_y",
    "flip_horizontal",
    "flip_vertical",
    "bring_to_front",
    "send_to_back",
    "bring_forward",
    "send_backward",
    "union",
    "subtract",
    "intersect",
    "xor",
    "divide",
    "combine",
    "release_compound",
    "convert_to_path",
    "expand_strokes",
    "simplify",
    "auto_layout",
    "create_component",
    "detach_instance",
    "reset_instance",
    "text_inside_shape",
    "attach_text_path",
    "release_text_path",
    "delete_layer",
    "duplicate_layer",
    "layer_forward",
    "layer_backward",
    "trace_raster",
    "mask_selection",
    "clear_pixels",
];

pub fn editable_layer(studio: &Studio, li: usize) -> Result<(), String> {
    if !studio.layer_unlocked(li) || !studio.doc.layer_visible(li) {
        return Err("Choose an unlocked, visible layer".into());
    }
    Ok(())
}
pub fn target(studio: &Studio, li: usize, id: u64) -> Result<(), String> {
    editable_layer(studio, li)?;
    if id == RASTER_ID && studio.doc.layers[li].kind.pixels().is_some() {
        Ok(())
    } else {
        object(studio, li, id).map(|_| ())
    }
}
pub fn targets(studio: &Studio, args: &Value) -> Result<Vec<(usize, u64)>, String> {
    let values = if let Some(values) = args.get("targets") {
        let values = values.as_array().ok_or("targets must be an array")?;
        if values.len() > 100 {
            return Err("At most 100 targets per call".into());
        }
        values
            .iter()
            .map(|v| Ok((index(v, "layer")?, index(v, "id")? as u64)))
            .collect::<Result<Vec<_>, String>>()?
    } else {
        studio.selection.clone()
    };
    if values.is_empty() {
        return Err("Choose at least one object or raster layer".into());
    }
    let mut seen = std::collections::HashSet::new();
    for &(li, id) in &values {
        target(studio, li, id)?;
        if !seen.insert((li, id)) {
            return Err("Duplicate target".into());
        }
    }
    Ok(values)
}
pub fn merge(base: &mut Value, patch: &Value) -> Result<(), String> {
    let patch = patch.as_object().ok_or("changes must be an object")?;
    let base = base.as_object_mut().ok_or("Expected native properties")?;
    for (key, value) in patch {
        let old = base
            .get_mut(key)
            .ok_or_else(|| format!("Unknown property {key}; inspect native defaults first"))?;
        if value.is_object() && old.is_object() {
            merge(old, value)?;
        } else {
            *old = value.clone();
        }
    }
    Ok(())
}
pub fn finite(value: &Value) -> Result<(), String> {
    match value {
        Value::Number(n)
            if n.as_f64()
                .is_none_or(|v| !v.is_finite() || v.abs() > 100_000.) =>
        {
            Err("Numeric properties must be finite and within ±100000".into())
        }
        Value::Array(values) => {
            if values.len() > 4096 {
                return Err("Too many property values".into());
            }
            for v in values {
                finite(v)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for v in values.values() {
                finite(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
pub fn execute(studio: &mut Studio, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "configure_canvas" => {
            let before = crate::document::CanvasSettings::read(&studio.doc);
            let mut after = before.clone();
            after.name = text(args, "name", &before.name)?;
            after.width = number(args, "width", before.width)?;
            after.height = number(args, "height", before.height)?;
            after.dpi = number(args, "dpi", before.dpi)?;
            after.transparent = boolean(args, "transparent", before.transparent)?;
            if !(1.0..=32768.).contains(&after.width)
                || !(1.0..=32768.).contains(&after.height)
                || !(1.0..=9600.).contains(&after.dpi)
            {
                return Err("Canvas edges must be 1–32768 pixels and DPI 1–9600".into());
            }
            let mut check = studio.doc.clone();
            let resized = before.width != after.width || before.height != after.height;
            let mut commands = vec![Cmd::SetCanvas {
                before: before.clone(),
                after: after.clone(),
            }];
            if resized && args.get("artboards").is_none() && studio.doc.artboards.len() == 1 {
                let board = &studio.doc.artboards[0];
                if board.origin == Pt::ZERO && board.size == Pt::new(before.width, before.height) {
                    let mut boards = studio.doc.artboards.clone();
                    boards[0].size = Pt::new(after.width, after.height);
                    commands.push(Cmd::SetArtboards {
                        before: studio.doc.artboards.clone(),
                        after: boards,
                    });
                }
            }
            for key in ["artboards", "guides", "layout_tokens"] {
                if let Some(value) = args.get(key) {
                    let values = value
                        .as_array()
                        .ok_or("Canvas collections must be arrays")?;
                    if values.len() > if key == "artboards" { 256 } else { 1000 } {
                        return Err("Too many canvas items".into());
                    }
                    match key {
                        "artboards" => {
                            let after: Vec<crate::document::Artboard> =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            let mut seen = std::collections::HashSet::new();
                            for board in &after {
                                if !seen.insert(board.id) || board.id == 0 {
                                    return Err("Artboards need distinct positive IDs".into());
                                }
                                let bounds = board.bounds();
                                if ![bounds.min.x, bounds.min.y, bounds.width(), bounds.height()]
                                    .iter()
                                    .all(|n| n.is_finite() && n.abs() <= 100000.)
                                    || bounds.width() <= 0.
                                    || bounds.height() <= 0.
                                {
                                    return Err("Invalid artboard bounds".into());
                                }
                            }
                            commands.push(Cmd::SetArtboards {
                                before: check.artboards.clone(),
                                after,
                            });
                        }
                        "guides" => {
                            finite(value)?;
                            let after =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            commands.push(Cmd::SetGuides {
                                before: check.guides.clone(),
                                after,
                            });
                        }
                        "layout_tokens" => {
                            let after =
                                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                            commands.push(Cmd::SetLayoutTokens {
                                before: check.layout_tokens.clone(),
                                after,
                            });
                        }
                        _ => unreachable!(),
                    }
                }
            }
            let command = Cmd::Batch(commands);
            crate::document::apply(&mut check, &command);
            check.validate_hierarchy()?;
            studio.commit(command);
            studio.need_fit = true;
        }
        "patch_object" => {
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            let patch = args["changes"]
                .as_object()
                .ok_or("changes must be an object")?;
            let original = if patch
                .keys()
                .all(|k| matches!(k.as_str(), "locked" | "visible" | "guide"))
            {
                layer(studio, li)?;
                let shape = studio
                    .doc
                    .find_shape(li, id)
                    .ok_or("Object no longer exists")?
                    .clone();
                if let Some(parent) = shape.layout.parent {
                    object(studio, li, parent)?;
                }
                shape
            } else {
                object(studio, li, id)?
            };
            let allowed = [
                "geom",
                "style",
                "corners",
                "text_wrap",
                "opacity",
                "fill_opacity",
                "blend_interior",
                "blend",
                "rotation",
                "name",
                "visible",
                "locked",
                "guide",
            ];
            for key in patch.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(format!("Unsupported object property {key}"));
                }
            }
            finite(&args["changes"])?;
            let mut value = serde_json::to_value(&original).map_err(|e| e.to_string())?;
            for (key, v) in patch {
                value[key] = v.clone();
            }
            let mut shape: Shape = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if let Geom::Text(run) = &shape.geom {
                if run.content.len() > 64000 {
                    return Err("Text content is too long".into());
                }
            }
            if patch.contains_key("geom") {
                crate::text::fill_contours(&mut shape.geom);
            }
            if !(0.0..=1.0).contains(&shape.opacity) || !(0.0..=1.0).contains(&shape.fill_opacity) {
                return Err("Opacity must be within 0–1".into());
            }
            let bounds = shape.geom.bbox();
            if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 100000.)
            {
                return Err("Invalid object geometry".into());
            }
            let before = studio.doc.layers[li].kind.shapes().unwrap().to_vec();
            let mut after = before.clone();
            *after.iter_mut().find(|s| s.id == id).unwrap() = shape;
            studio.commit(Cmd::Batch(vec![Cmd::SetVectorShapes {
                layer: li,
                before,
                after,
            }]));
        }
        "set_mode" => {
            let mode: Persona =
                serde_json::from_value(args["mode"].clone()).map_err(|e| e.to_string())?;
            studio.switch_persona(mode);
        }
        "transform_raster" => {
            let li = index(args, "layer")?;
            editable_layer(studio, li)?;
            let l = &studio.doc.layers[li];
            let before = l.kind.raster_xform().ok_or("Choose a raster layer")?;
            let size = Pt::new(
                number(args, "width", before.1.x)?,
                number(args, "height", before.1.y)?,
            );
            if size.x <= 0. || size.y <= 0. {
                return Err("Raster dimensions must be positive".into());
            }
            let after = (
                Pt::new(
                    number(args, "x", before.0.x)?,
                    number(args, "y", before.0.y)?,
                ),
                size,
                number(args, "rotation", before.2.to_degrees())?.to_radians(),
            );
            let shear = number(args, "shear", l.kind.raster_shear())?;
            let opacity = number(args, "opacity", l.opacity)?.clamp(0., 1.);
            studio.commit(Cmd::Batch(vec![
                Cmd::SetRasterXform {
                    layer: li,
                    before,
                    after,
                },
                Cmd::SetRasterShear {
                    layer: li,
                    before: l.kind.raster_shear(),
                    after: shear,
                },
                Cmd::SetLayerMeta {
                    index: li,
                    name: l.name.clone(),
                    visible: l.visible,
                    locked: l.locked,
                    opacity,
                    blend: l.blend,
                    before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
                },
            ]));
            studio.selection = vec![(li, RASTER_ID)];
            studio.active_layer = Some(li);
        }
        "set_layout" => {
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            let shape = object(studio, li, id)?;
            let patch = args["changes"]
                .as_object()
                .ok_or("changes must be an object")?;
            let keys = [
                "frame",
                "parent",
                "stack",
                "constraint_x",
                "constraint_y",
                "placeholder",
                "width",
                "height",
                "min_width",
                "max_width",
                "min_height",
                "max_height",
                "absolute",
                "clip",
                "text_size",
                "aspect_ratio",
                "breakpoints",
                "interactions",
                "tokens",
            ];
            for key in patch.keys() {
                if !keys.contains(&key.as_str()) {
                    return Err(format!("Unsupported layout property {key}"));
                }
            }
            finite(&args["changes"])?;
            let mut value = serde_json::to_value(&shape.layout).unwrap();
            for (key, v) in patch {
                value[key] = v.clone();
            }
            let after: crate::layout::FrameLayout =
                serde_json::from_value(value).map_err(|e| e.to_string())?;
            if let Some(parent) = after.parent {
                let p = object(studio, li, parent)?;
                if !p.layout.frame || parent == id {
                    return Err("Parent must be a different editable frame".into());
                }
            }
            if let Some(stack) = &after.stack {
                if stack.columns > 256
                    || stack.gap.abs() > 10000.
                    || stack.padding.iter().any(|p| p.abs() > 10000.)
                {
                    return Err("Layout spacing/columns exceeds supported limits".into());
                }
            }
            if after.breakpoints.len() > 100 {
                return Err("At most 100 breakpoints".into());
            }
            let mut check = studio.doc.clone();
            check.find_shape_mut(li, id).unwrap().layout = after.clone();
            check.validate_hierarchy()?;
            studio.commit(Cmd::Batch(vec![Cmd::SetLayout {
                layer: li,
                id,
                before: shape.layout,
                after,
            }]));
        }
        "set_filter_stack" => {
            let li = index(args, "layer")?;
            editable_layer(studio, li)?;
            let id = args
                .get("id")
                .map(|_| index(args, "id").map(|id| id as u64))
                .transpose()?;
            if let Some(id) = id {
                object(studio, li, id)?;
            }
            let effects = args["effects"]
                .as_array()
                .ok_or("effects must be an array")?;
            if effects.len() > 32 {
                return Err("At most 32 effects".into());
            }
            let mut items = vec![];
            for spec in effects {
                let name = text(spec, "name", "")?;
                let (_, factory) = crate::filter::Fx::catalog()
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(&name))
                    .ok_or("Unknown effect; use get_editor_capabilities")?;
                let mut value = serde_json::to_value(factory()).unwrap();
                if let Some(parameters) = spec.get("parameters") {
                    finite(parameters)?;
                    if let Some(params) = parameters.as_object() {
                        for (key, v) in params {
                            if matches!(
                                key.as_str(),
                                "std" | "blur" | "radius" | "spread" | "choke"
                            ) && v.as_f64().is_some_and(|n| !(0.0..=256.0).contains(&n))
                            {
                                return Err("Effect radii must be within 0–256".into());
                            }
                        }
                        if params
                            .get("octaves")
                            .and_then(Value::as_u64)
                            .is_some_and(|n| n > 8)
                        {
                            return Err("At most 8 noise octaves".into());
                        }
                    }
                    merge(
                        value.as_object_mut().unwrap().values_mut().next().unwrap(),
                        parameters,
                    )?;
                }
                items.push(serde_json::from_value(value).map_err(|e| e.to_string())?);
            }
            let after = crate::filter::FilterStack {
                items,
                ..Default::default()
            };
            let cmd = if let Some(id) = id {
                Cmd::SetShapeFilters {
                    layer: li,
                    id,
                    before: studio.doc.find_shape(li, id).unwrap().filters.clone(),
                    after,
                }
            } else {
                Cmd::SetFilters {
                    index: li,
                    before: studio.doc.layers[li].filters.clone(),
                    after,
                }
            };
            studio.commit(Cmd::Batch(vec![cmd]));
        }
        "get_editor_capabilities" => {
            return Ok(json!({
                "modes":["design","pixel","photo","layout","motion"],"actions":ACTIONS,
                "effects":crate::filter::Fx::catalog().iter().map(|(name,f)|json!({"name":name,"native":f()})).collect::<Vec<_>>(),
                "motion_properties":crate::motion::Prop::all().iter().map(|p|json!({"property":p,"label":p.name()})).collect::<Vec<_>>(),
                "motion_presets":crate::motion_presets::Preset::ALL.iter().map(|p|p.name()).collect::<Vec<_>>(),
                "layout_example":{"frame":true,"parent":null,"clip":true,"stack":crate::layout::AutoStack::default(),"width":"Fixed","height":"Fixed","constraint_x":"Start","constraint_y":"Start","min_width":null,"max_width":null,"min_height":null,"max_height":null,"text_size":null,"aspect_ratio":null,"absolute":false,"breakpoints":[]},
                "photo_adjustments":crate::photo::DevelopParams::default(),
                "photo_ranges":{"exposure":[-16,16],"temperature_tint":[-100,100],"contrast":[0,4],"saturation_vibrance":[0,2],"grain":[0,1],"hue":[-180,180],"tone_clarity_vignette_dehaze_split":[-1,1],"curve":[0,1],"crop":"normalized [left,top,right,bottom] or null","rotate":[0,90,180,270]}
            }));
        }
        "editor_action" => action(studio, args)?,
        _ => return Err(format!("Unknown editor tool {name}")),
    }
    Ok(
        json!({"revision":studio.canvas_gen,"selection":studio.selection,"mode":studio.persona,"photo_revision":studio.photo.edit_revision,"status":studio.status}),
    )
}
fn action(studio: &mut Studio, args: &Value) -> Result<(), String> {
    let action = text(args, "action", "")?;
    if !ACTIONS.contains(&action.as_str()) {
        return Err("Unknown editor action".into());
    }
    if matches!(action.as_str(), "undo" | "redo") {
        let photo = match args["history"].as_str() {
            Some("photo") => true,
            Some("canvas") => false,
            None => studio.persona == Persona::Photo,
            _ => return Err("Unknown history".into()),
        };
        if photo {
            if action == "undo" {
                studio.photo.undo();
            } else {
                studio.photo.redo();
            }
            studio.mark();
        } else {
            let persona = studio.persona;
            studio.persona = Persona::Design;
            if action == "undo" {
                studio.undo();
            } else {
                studio.redo();
            }
            studio.persona = persona;
        }
        return Ok(());
    }
    if matches!(
        action.as_str(),
        "delete_layer" | "duplicate_layer" | "layer_forward" | "layer_backward"
    ) {
        let li = index(args, "layer")?;
        editable_layer(studio, li)?;
        for child in studio.layer_tree_indices(li) {
            editable_layer(studio, child)?;
        }
        match action.as_str() {
            "delete_layer" => studio.delete_layer_tree(li),
            "layer_forward" => studio.move_layer_tree(li, true),
            "layer_backward" => studio.move_layer_tree(li, false),
            "duplicate_layer" => {
                studio.activate_layer_tree(li);
                studio.duplicate_selection();
            }
            _ => unreachable!(),
        }
        return Ok(());
    }
    let selection = targets(studio, args)?;
    let dx = number(args, "dx", 10.)?;
    let dy = number(args, "dy", 10.)?;
    let raster = selection.iter().any(|t| t.1 == RASTER_ID);
    if raster
        && matches!(
            action.as_str(),
            "union"
                | "subtract"
                | "intersect"
                | "xor"
                | "divide"
                | "combine"
                | "release_compound"
                | "convert_to_path"
                | "expand_strokes"
                | "simplify"
                | "auto_layout"
                | "create_component"
                | "detach_instance"
                | "reset_instance"
                | "text_inside_shape"
                | "attach_text_path"
                | "release_text_path"
        )
    {
        return Err("This operation needs vector objects; raster transforms, masks, painting, effects and motion are available directly".into());
    }
    studio.selection = selection;
    studio.active_layer = Some(studio.selection[0].0);
    studio.selected_layer = None;
    use crate::boolean::BoolOp;
    match action.as_str() {
        "duplicate" => studio.duplicate_selection_by(Pt::new(dx, dy)),
        "delete" => studio.delete_selection(),
        "nudge" => studio.nudge(dx, dy),
        "align_left" => studio.align_sel(crate::align::Align::Left),
        "align_center" => studio.align_sel(crate::align::Align::CenterX),
        "align_right" => studio.align_sel(crate::align::Align::Right),
        "align_top" => studio.align_sel(crate::align::Align::Top),
        "align_middle" => studio.align_sel(crate::align::Align::CenterY),
        "align_bottom" => studio.align_sel(crate::align::Align::Bottom),
        "distribute_x" => studio.distribute_sel(crate::align::Distribute::Horizontal),
        "distribute_y" => studio.distribute_sel(crate::align::Distribute::Vertical),
        "flip_horizontal" => studio.flip_selection(true),
        "flip_vertical" => studio.flip_selection(false),
        "bring_to_front" => studio.bring_to_front(),
        "send_to_back" => studio.send_to_back(),
        "bring_forward" => studio.bring_forward(),
        "send_backward" => studio.send_backward(),
        "union" => studio.apply_boolean_multi(BoolOp::Union),
        "subtract" => studio.apply_boolean_multi(BoolOp::Subtract),
        "intersect" => studio.apply_boolean_multi(BoolOp::Intersect),
        "xor" => studio.apply_boolean_multi(BoolOp::Xor),
        "divide" => studio.divide_selection(),
        "combine" => studio.combine_selected(),
        "release_compound" => studio.release_compound(),
        "convert_to_path" => {
            for (li, id) in studio.selection.clone() {
                studio.convert_object_to_path(li, id);
            }
        }
        "expand_strokes" => studio.expand_strokes(),
        "simplify" => studio.simplify_selection(),
        "auto_layout" => studio.auto_layout_selection(),
        "create_component" => studio.create_layout_component(),
        "detach_instance" => studio.detach_layout_instance(),
        "reset_instance" => studio.reset_layout_instance(),
        "text_inside_shape" => studio.text_inside_shape(),
        "attach_text_path" => studio.attach_text_path(),
        "release_text_path" => studio.release_text_path(),
        "trace_raster" => {
            if !raster {
                return Err("Select a raster image to trace".into());
            }
            studio.trace_active_raster();
        }
        "mask_selection" => {
            for (li, id) in studio.selection.clone() {
                if id == RASTER_ID {
                    studio.mask_from_selection(li);
                } else {
                    studio.mask_object_from_selection(li, id);
                }
            }
        }
        "clear_pixels" => {
            if !studio.clear_selected_pixels() {
                return Err("Choose a pixel selection on a raster layer".into());
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
