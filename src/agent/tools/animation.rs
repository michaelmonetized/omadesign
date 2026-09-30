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
            json!({"duration":studio.doc.motion.duration,"fps":studio.doc.motion.fps,"looped":studio.doc.motion.looped,"time":studio.playhead,"playing":studio.playing,"tracks":tracks,"next_offset":(next<studio.doc.motion.tracks.len()).then_some(next),"revision":studio.canvas_gen}),
        );
    }
    let mut after = studio.doc.motion.clone();
    match name {
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
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            editing::target(studio, li, id)?;
            let target = crate::motion::target(&studio.doc, li, id).ok_or("No animation target")?;
            let prop: Prop = serde_json::from_value(args["property"].clone())
                .map_err(|_| "Unknown property; use get_editor_capabilities")?;
            if id == crate::document::RASTER_ID
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
                        .find(|t| t.shape == target.id && t.prop == prop)
                    {
                        track.keys.retain(|k| (k.t - time).abs() > 0.0001);
                    }
                } else {
                    let ease: Ease = serde_json::from_value(
                        key.get("ease").cloned().unwrap_or(json!("EaseInOut")),
                    )
                    .map_err(|e| e.to_string())?;
                    if prop == Prop::Fill {
                        after.set_color_key(
                            target.id,
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
                        if matches!(prop, Prop::Scale | Prop::Width | Prop::Height) && value <= 0. {
                            return Err("Scale multipliers must be positive".into());
                        }
                        // API rotation is degrees; the native pose stores radians.
                        let value = if prop == Prop::Rotation {
                            value.to_radians()
                        } else {
                            value
                        };
                        after.set_key(target.id, prop, time, value, ease);
                    }
                    after.duration = after.duration.max(time);
                }
            }
            after.tracks.retain(|t| !t.keys.is_empty());
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
        json!({"revision":studio.canvas_gen,"duration":studio.doc.motion.duration,"time":studio.playhead}),
    )
}
