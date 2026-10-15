use super::*;
use crate::motion::{Ease, Prop};
pub fn execute(studio: &mut Studio, name: &str, args: &Value) -> Result<Value, String> {
    if name == "get_motion" {
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let tracks: Vec<_> = studio
            .doc
            .motion
            .tracks
            .iter()
            .skip(offset)
            .take(50)
            .collect();
        let next = offset.saturating_add(tracks.len());
        return Ok(
            json!({"duration":studio.doc.motion.duration,"fps":studio.doc.motion.fps,"looped":studio.doc.motion.looped,"time":studio.playhead,"playing":studio.playing,"tracks":tracks,"expressions":studio.doc.motion.expressions,"compositions":studio.doc.motion.compositions.iter().map(|c|json!({"id":c.id,"name":c.name,"start":c.start,"end":c.end,"source_start":c.source_start,"speed":c.speed,"looped":c.looped,"position":c.position,"anchor":c.anchor,"time_remap":c.time_remap.iter().map(|k|json!({"time":k.t,"value":k.value,"ease":k.ease})).collect::<Vec<_>>(),"scale":c.scale,"rotation":c.rotation.to_degrees(),"opacity":c.opacity,"visible":c.visible,"width":c.document.width,"height":c.document.height})).collect::<Vec<_>>(),"error":studio.doc.motion.evaluate_frame(studio.playhead,&mut ||true).err(),"next_offset":(next<studio.doc.motion.tracks.len()).then_some(next),"revision":studio.canvas_gen}),
        );
    }
    let mut after = studio.doc.motion.clone();
    let mut composition_id = None;
    match name {
        "set_composition" => {
            let id = args
                .get("id")
                .map(|v| {
                    v.as_u64()
                        .ok_or("Composition id must be an unsigned integer")
                })
                .transpose()?;
            if boolean(args, "remove", false)? {
                let id = id.ok_or("Composition id is required")?;
                if !after.compositions.iter().any(|c| c.id == id) {
                    return Err("Unknown composition".into());
                }
                after.compositions.retain(|c| c.id != id);
                after.drop_shape(id);
            } else {
                let comp = if let Some(id) = id {
                    after
                        .compositions
                        .iter_mut()
                        .find(|c| c.id == id)
                        .ok_or("Unknown composition")?
                } else {
                    let path = files::path(args)?;
                    files::regular(&path, 64 * 1024 * 1024)?;
                    let doc = if path.extension().is_some_and(|e| e == "omamotion") {
                        crate::motion::compositions::import(
                            &std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
                        )?
                    } else {
                        crate::project::load_from(&path)?
                    };
                    after
                        .compositions
                        .push(crate::motion::compositions::Composition::new(doc));
                    after
                        .compositions
                        .last_mut()
                        .ok_or("Could not add composition")?
                };
                comp.name = text(args, "name", &comp.name)?;
                comp.start = number(args, "start", comp.start)?;
                comp.end = number(args, "end", comp.end)?;
                comp.source_start = number(args, "source_start", comp.source_start)?;
                comp.speed = number(args, "speed", comp.speed)?;
                comp.looped = boolean(args, "looped", comp.looped)?;
                comp.position.x = number(args, "x", comp.position.x)?;
                comp.position.y = number(args, "y", comp.position.y)?;
                comp.anchor.x = number(args, "anchor_x", comp.anchor.x)?;
                comp.anchor.y = number(args, "anchor_y", comp.anchor.y)?;
                if let Some(keys) = args.get("time_remap") {
                    comp.time_remap = remap_keys(keys)?;
                }
                comp.scale.x = number(args, "scale_x", comp.scale.x)?;
                comp.scale.y = number(args, "scale_y", comp.scale.y)?;
                comp.rotation = number(args, "rotation", comp.rotation.to_degrees())?.to_radians();
                comp.opacity = number(args, "opacity", comp.opacity)?;
                comp.visible = boolean(args, "visible", comp.visible)?;
                composition_id = Some(comp.id);
                after.duration = after.duration.max(comp.end);
            }
            let mut candidate = studio.doc.clone();
            candidate.motion = after.clone();
            crate::motion::compositions::validate(&candidate)?;
            studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
                before: studio.doc.motion.clone(),
                after,
            }]));
        }
        "set_expression" => {
            let (target_id, kind) = animation_target(studio, args)?;
            let prop: Prop =
                serde_json::from_value(args["property"].clone()).map_err(|_| "Unknown property")?;
            validate_composition_property(kind, prop)?;
            let source = if boolean(args, "remove", false)? {
                None
            } else {
                Some(text(args, "source", "")?)
            };
            after.set_expression(target_id, prop, source)?;
            validate_motion(studio, &after)?;
            studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
                before: studio.doc.motion.clone(),
                after,
            }]));
        }
        "set_motion" => {
            after.duration = number(args, "duration", after.duration)?;
            after.fps = number(args, "fps", after.fps)?;
            if !(0.01..=3600.).contains(&after.duration) || !(1.0..=240.).contains(&after.fps) {
                return Err("Duration must be 0.01–3600 seconds and FPS 1–240".into());
            }
            if after
                .tracks
                .iter()
                .flat_map(|t| &t.keys)
                .any(|k| k.t > after.duration)
            {
                return Err(
                    "Duration cannot end before existing keys; remove or retime them first".into(),
                );
            }
            after.looped = boolean(args, "looped", after.looped)?;
            let time = number(args, "time", studio.playhead)?;
            if time < 0. || time > after.duration {
                return Err("Playhead must be within clip duration".into());
            }
            let playing = boolean(args, "playing", studio.playing)?;
            if after != studio.doc.motion {
                studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
                    before: studio.doc.motion.clone(),
                    after: after.clone(),
                }]));
            }
            studio.playhead = time;
            studio.playing = playing;
            studio.mark();
        }
        "set_keyframes" => {
            let (target_id, kind) = animation_target(studio, args)?;
            let prop: Prop = serde_json::from_value(args["property"].clone())
                .map_err(|_| "Unknown property; use get_editor_capabilities")?;
            validate_composition_property(kind, prop)?;
            if matches!(kind, AnimationTarget::Raster)
                && !matches!(
                    prop,
                    Prop::X
                        | Prop::Y
                        | Prop::Rotation
                        | Prop::Scale
                        | Prop::Width
                        | Prop::Height
                        | Prop::Opacity
                        | Prop::FillReveal
                )
            {
                return Err("That channel requires vector paint; raster images support transform, opacity and fill reveal".into());
            }
            let keys = args["keys"].as_array().ok_or("keys must be an array")?;
            if keys.is_empty() || keys.len() > 1000 {
                return Err("Provide 1–1000 keys".into());
            }
            let remove = boolean(args, "remove", false)?;
            for key in keys {
                let time = number(key, "time", -1.)?;
                if !(0.0..=3600.).contains(&time) {
                    return Err("Key time must be 0–3600 seconds".into());
                }
                if remove {
                    if let Some(track) = after
                        .tracks
                        .iter_mut()
                        .find(|t| t.shape == target_id && t.prop == prop)
                    {
                        track.keys.retain(|k| (k.t - time).abs() > 0.0001);
                    }
                } else {
                    after.duration = after.duration.max(time);
                    let ease: Ease = serde_json::from_value(
                        key.get("ease").cloned().unwrap_or(json!("EaseInOut")),
                    )
                    .map_err(|e| e.to_string())?;
                    if !ease.valid() {
                        return Err(
                            "Curve controls must be finite; x controls must be within 0–1".into(),
                        );
                    }
                    if prop == Prop::Fill {
                        after.set_color_key(
                            target_id,
                            time,
                            color(&text(key, "color", "")?)?,
                            ease,
                        );
                    } else {
                        let value = number(key, "value", prop.identity())?;
                        if matches!(prop, Prop::Opacity | Prop::FillReveal | Prop::StrokeReveal)
                            && !(0.0..=1.).contains(&value)
                        {
                            return Err("Opacity/reveal values must be within 0–1".into());
                        }
                        if !matches!(kind, AnimationTarget::Composition)
                            && matches!(prop, Prop::Scale | Prop::Width | Prop::Height)
                            && value <= 0.
                        {
                            return Err("Scale multipliers must be positive".into());
                        }
                        // API rotation is degrees; the native pose stores radians.
                        let value = if prop == Prop::Rotation {
                            value.to_radians()
                        } else {
                            value
                        };
                        after.set_key(target_id, prop, time, value, ease);
                    }
                    after.duration = after.duration.max(time);
                }
            }
            after.tracks.retain(|t| !t.keys.is_empty());
            validate_motion(studio, &after)?;
            studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
                before: studio.doc.motion.clone(),
                after,
            }]));
        }
        "apply_motion_preset" => {
            let name = text(args, "preset", "")?;
            let preset = crate::motion_presets::Preset::ALL
                .into_iter()
                .find(|p| p.name().eq_ignore_ascii_case(&name))
                .ok_or("Unknown preset; use get_editor_capabilities")?;
            let selection = editing::targets(studio, args)?;
            let mut targets = vec![];
            for &(li, id) in &selection {
                if id == crate::document::RASTER_ID {
                    if preset == crate::motion_presets::Preset::DrawStroke {
                        return Err("Draw stroke needs a vector outline".into());
                    }
                } else if !preset.supports(studio.doc.find_shape(li, id).unwrap()) {
                    return Err("Preset is not compatible with one of the selected objects".into());
                }
                targets.push(
                    crate::motion::target(&studio.doc, li, id).ok_or("Missing animation target")?,
                );
            }
            let options = crate::motion_presets::Options {
                duration: number(args, "duration", 0.8)?,
                delay: number(args, "delay", 0.)?,
                stagger: number(args, "stagger", 0.08)?,
                intensity: number(args, "intensity", 1.)?,
                start_at_playhead: true,
            };
            let time = number(args, "time", studio.playhead)?;
            let after =
                crate::motion_presets::apply(&studio.doc.motion, preset, &targets, time, options)?;
            if !(0.0..=3600.0).contains(&time)
                || !(0.01..=3600.0).contains(&after.duration)
                || after
                    .tracks
                    .iter()
                    .flat_map(|track| &track.keys)
                    .any(|key| !(0.0..=3600.0).contains(&key.t))
            {
                return Err("Preset and all staggered keys must fit within 0–3600 seconds".into());
            }
            studio.commit(Cmd::Batch(vec![Cmd::SetMotion {
                before: studio.doc.motion.clone(),
                after,
            }]));
            studio.selection = selection;
            studio.playing = false;
        }
        _ => return Err("Unknown motion tool".into()),
    }
    Ok(
        json!({"revision":studio.canvas_gen,"duration":studio.doc.motion.duration,"time":studio.playhead,"composition":composition_id}),
    )
}

#[derive(Clone, Copy)]
enum AnimationTarget {
    Object,
    Raster,
    Composition,
}

/// Resolve an explicit animation target without changing selection.
/// Arguments are the live studio and either composition or layer/id fields; returns a persistent id and target kind.
fn animation_target(studio: &Studio, args: &Value) -> Result<(u64, AnimationTarget), String> {
    if let Some(value) = args.get("composition") {
        if args.get("layer").is_some() || args.get("id").is_some() {
            return Err("Choose composition or layer/id, not both".into());
        }
        let id = value
            .as_u64()
            .ok_or("Composition must be an unsigned persistent id")?;
        if !studio.doc.motion.compositions.iter().any(|c| c.id == id) {
            return Err("Unknown composition".into());
        }
        return Ok((id, AnimationTarget::Composition));
    }
    let layer = index(args, "layer")?;
    let id = index(args, "id")? as u64;
    editing::target(studio, layer, id)?;
    let target = crate::motion::target(&studio.doc, layer, id).ok_or("No animation target")?;
    Ok((
        target.id,
        if id == crate::document::RASTER_ID {
            AnimationTarget::Raster
        } else {
            AnimationTarget::Object
        },
    ))
}

fn validate_composition_property(kind: AnimationTarget, prop: Prop) -> Result<(), String> {
    if matches!(kind, AnimationTarget::Composition)
        && !matches!(
            prop,
            Prop::X
                | Prop::Y
                | Prop::Rotation
                | Prop::Scale
                | Prop::Width
                | Prop::Height
                | Prop::Opacity
        )
    {
        return Err("Compositions support X, Y, Rotation, Scale, Width, Height and Opacity".into());
    }
    Ok(())
}

fn validate_motion(studio: &Studio, motion: &crate::motion::Motion) -> Result<(), String> {
    let mut candidate = studio.doc.clone();
    candidate.motion = motion.clone();
    crate::motion::compositions::validate(&candidate)
}

/// Read explicit source-time keys.
/// Argument is an array of time/value/ease objects; returns ordered bounded native keys or a named error.
fn remap_keys(value: &Value) -> Result<Vec<crate::motion::Key>, String> {
    let values = value.as_array().ok_or("time_remap must be an array")?;
    if values.len() > 10_000 {
        return Err("time_remap supports at most 10000 keys".into());
    }
    let mut keys = Vec::with_capacity(values.len());
    for value in values {
        let time = number(value, "time", -1.)?;
        if value.get("value").is_none() {
            return Err("Time-remap source value is required".into());
        }
        let source = number(value, "value", 0.)?;
        let ease: Ease =
            serde_json::from_value(value.get("ease").cloned().unwrap_or(json!("Linear")))
                .map_err(|e| e.to_string())?;
        if !(0.0..=3600.0).contains(&time) || source.abs() > 3600.0 || !ease.valid() {
            return Err(
                "Time-remap keys need time 0–3600, source seconds -3600–3600 and a valid curve"
                    .into(),
            );
        }
        if keys
            .last()
            .is_some_and(|k: &crate::motion::Key| k.t >= time)
        {
            return Err("Time-remap keys must have strictly increasing times".into());
        }
        keys.push(crate::motion::Key::at(time, source, ease));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_time_limit_is_atomic_and_accepts_boundary() {
        let mut studio = Studio::new();
        studio.doc.layers.clear();
        let mut layer = crate::document::Layer::vector("Target");
        let shape = crate::layout::make_frame(crate::geom::Pt::ZERO, crate::geom::Pt::splat(10.));
        let id = shape.id;
        layer.kind.shapes_mut().unwrap().push(shape);
        studio.doc.layers.push(layer);
        let before = crate::project::encode(&studio.doc).unwrap();
        let revision = studio.canvas_gen;
        for time in [-1., 3599.5, 100000.] {
            let args = json!({"preset":"Fade in","targets":[{"layer":0,"id":id}],"time":time,"duration":1.0});
            assert!(execute(&mut studio, "apply_motion_preset", &args).is_err());
            assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
            assert_eq!(studio.canvas_gen, revision);
        }
        execute(&mut studio, "apply_motion_preset", &json!({"preset":"Fade in","targets":[{"layer":0,"id":id}],"time":3599.,"duration":1.0})).unwrap();
        assert_eq!(studio.doc.motion.duration, 3600.);
    }
}

#[cfg(test)]
mod composition_tests {
    use super::*;
    fn studio() -> (Studio, u64) {
        let mut studio = Studio::new();
        studio.doc.layers.clear();
        let child = crate::document::Document::new("Nested", 32., 24., 96.);
        let comp = crate::motion::compositions::Composition::new(child);
        let id = comp.id;
        studio.doc.motion.compositions.push(comp);
        studio.history.clear();
        (studio, id)
    }
    #[test]
    fn composition_tools_author_keys_expressions_anchor_remap_and_save_without_selection() {
        let (mut studio, id) = studio();
        assert!(studio.selection.is_empty());
        let before = studio.doc.motion.clone();
        let result=execute(&mut studio,"set_composition",&json!({"id":id,"anchor_x":4.,"anchor_y":7.,"time_remap":[{"time":0.,"value":1.,"ease":"Hold"},{"time":2.,"value":0.}]})).unwrap();
        assert_eq!(result["composition"], id);
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(studio.doc.motion, before);
        studio.redo();
        execute(&mut studio,"set_keyframes",&json!({"composition":id,"property":"X","keys":[{"time":0.,"value":0.,"ease":"Linear"},{"time":3.,"value":30.,"ease":"Hold"}]})).unwrap();
        assert_eq!(studio.doc.motion.duration, 3.);
        assert_eq!(studio.doc.motion.tracks[0].keys.last().unwrap().t, 3.);
        assert_eq!(studio.history.len(), 2);
        let keys = studio.doc.motion.tracks.clone();
        execute(
            &mut studio,
            "set_expression",
            &json!({"composition":id,"property":"X","source":"value + 2*time"}),
        )
        .unwrap();
        assert_eq!(studio.history.len(), 3);
        assert_eq!(studio.doc.motion.pose(id, 1.5).dx, 18.);
        assert_eq!(studio.doc.motion.tracks, keys);
        let metadata = execute(&mut studio, "get_motion", &json!({})).unwrap();
        assert_eq!(
            metadata["compositions"][0]["anchor"],
            json!({"x":4.,"y":7.})
        );
        assert_eq!(metadata["compositions"][0]["time_remap"][0]["time"], 0.);
        assert_eq!(metadata["compositions"][0]["time_remap"][0]["ease"], "Hold");
        let reopened =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        assert_eq!(reopened.motion, studio.doc.motion);
        execute(
            &mut studio,
            "set_expression",
            &json!({"composition":id,"property":"X","remove":true}),
        )
        .unwrap();
        assert_eq!(studio.history.len(), 4);
        assert!(studio.doc.motion.expressions.is_empty());
        studio.undo();
        assert_eq!(studio.doc.motion, reopened.motion);
    }
    #[test]
    fn composition_tool_validation_is_atomic_and_existing_object_arguments_still_work() {
        let (mut studio, id) = studio();
        let before = studio.doc.motion.clone();
        let revision = studio.canvas_gen;
        for (name, args) in [
            (
                "set_keyframes",
                json!({"composition":id,"layer":0,"id":0,"property":"X","keys":[{"time":0.,"value":1.}]}),
            ),
            (
                "set_keyframes",
                json!({"composition":0,"property":"X","keys":[{"time":0.,"value":1.}]}),
            ),
            (
                "set_keyframes",
                json!({"composition":id,"property":"Fill","keys":[{"time":0.,"color":"#ffffff"}]}),
            ),
            (
                "set_keyframes",
                json!({"composition":id,"property":"Opacity","keys":[{"time":0.,"value":2.}]}),
            ),
            (
                "set_keyframes",
                json!({"composition":id,"property":"X","keys":[{"time":0.,"value":1.,"ease":{"CubicBezier":{"x1":2.,"y1":0.,"x2":1.,"y2":1.}}}]}),
            ),
            (
                "set_expression",
                json!({"composition":id,"property":"StrokeWidth","source":"time"}),
            ),
            (
                "set_expression",
                json!({"composition":id,"property":"X","source":"x".repeat(4097)}),
            ),
            (
                "set_composition",
                json!({"id":id,"time_remap":[{"time":1.,"value":0.},{"time":0.,"value":1.}]}),
            ),
            (
                "set_composition",
                json!({"id":id,"time_remap":[{"time":0.}]}),
            ),
            (
                "set_composition",
                json!({"id":id,"time_remap":[{"time":0.,"value":1.,"ease":{"CubicBezier":{"x1":-1.,"y1":0.,"x2":1.,"y2":1.}}}]}),
            ),
        ] {
            assert!(execute(&mut studio, name, &args).is_err(), "{name}: {args}");
            assert_eq!(studio.doc.motion, before);
            assert_eq!(studio.canvas_gen, revision);
            assert_eq!(studio.history.len(), 0);
        }
        let mut layer = crate::document::Layer::vector("Existing object");
        let shape = crate::layout::make_frame(crate::geom::Pt::ZERO, crate::geom::Pt::splat(8.));
        let shape_id = shape.id;
        layer.kind.shapes_mut().unwrap().push(shape);
        studio.doc.layers.push(layer);
        execute(
            &mut studio,
            "set_keyframes",
            &json!({"layer":0,"id":shape_id,"property":"X","keys":[{"time":0.,"value":6.}]}),
        )
        .unwrap();
        execute(
            &mut studio,
            "set_expression",
            &json!({"layer":0,"id":shape_id,"property":"X","source":"value + time"}),
        )
        .unwrap();
        assert_eq!(studio.doc.motion.pose(shape_id, 1.).dx, 7.);
    }
}
