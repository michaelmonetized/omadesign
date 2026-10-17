//! Saved alpha painting uses the existing dab engine and one ordinary channel command.
use super::{
    Op, Studio,
    masking::{self, SelectionSpace},
};
use crate::{
    channels::{Channel, Plane},
    deep_color::{storage, surface::Surface},
    document::Cmd,
    geom::Pt,
    paint,
    resources::Allocation,
    tools::Tool,
};
use std::sync::Arc;

pub struct Stroke {
    document: String,
    generation: u64,
    selection_generation: u64,
    before: Channel,
    quick: bool,
    working: Arc<Allocation<f32>>,
    coverage: Surface,
    selection: Option<Arc<Allocation<u8>>>,
    value: f32,
    opacity: f32,
    last: Option<Pt>,
}

fn valid_brush(b: &paint::Brush) -> bool {
    [
        b.size,
        b.angle,
        b.pressure,
        b.scatter,
        b.angle_jitter,
        b.size_jitter,
        b.opacity_jitter,
        b.hardness,
        b.opacity,
        b.flow,
        b.spacing,
    ]
    .iter()
    .all(|v| v.is_finite())
        && b.size > 0.
        && b.spacing > 0.
        && b.stroke_radius().is_finite()
        && b.spacing_distance().is_finite()
}
fn phase(remaining: f32, skipped: f64, spacing: f32) -> f32 {
    let value = (f64::from(remaining) - skipped).rem_euclid(f64::from(spacing));
    if value == 0. { spacing } else { value as f32 }
}
fn bounded_segment(
    from: Pt,
    to: Pt,
    width: u32,
    height: u32,
    brush: &paint::Brush,
    remaining: &mut f32,
) -> Option<(Pt, Pt, f64)> {
    let radius = f64::from(brush.stroke_radius());
    let x0 = f64::from(from.x);
    let y0 = f64::from(from.y);
    let dx = f64::from(to.x) - x0;
    let dy = f64::from(to.y) - y0;
    let distance = dx.hypot(dy);
    let mut first: f64 = 0.;
    let mut last: f64 = 1.;
    let mut enter = None;
    let mut leave = None;
    for (p, q, face) in [
        (-dx, x0 + radius, (true, -radius)),
        (
            dx,
            f64::from(width) + radius - x0,
            (true, f64::from(width) + radius),
        ),
        (-dy, y0 + radius, (false, -radius)),
        (
            dy,
            f64::from(height) + radius - y0,
            (false, f64::from(height) + radius),
        ),
    ] {
        if p == 0. {
            if q < 0. {
                *remaining = phase(*remaining, distance, brush.spacing_distance());
                return None;
            }
        } else {
            let t = q / p;
            if p < 0. {
                if t >= first {
                    first = t;
                    enter = Some(face)
                }
            } else if t <= last {
                last = t;
                leave = Some(face)
            }
        }
    }
    if first > last {
        *remaining = phase(*remaining, distance, brush.spacing_distance());
        return None;
    }
    if first == 0. && last == 1. {
        return Some((from, to, 0.));
    }
    if first > 0. {
        let offset = (f64::from(*remaining) - first * distance)
            .rem_euclid(f64::from(brush.spacing_distance()));
        *remaining = offset as f32;
    }
    let cross = f64::from(to.x) * y0 - f64::from(to.y) * x0;
    let point = |face: Option<(bool, f64)>, fallback| match face {
        Some((true, x)) => Pt::new(x as f32, ((dy * x + cross) / dx) as f32),
        Some((false, y)) => Pt::new(((dx * y - cross) / dy) as f32, y as f32),
        None => fallback,
    };
    Some((point(enter, from), point(leave, to), (1. - last) * distance))
}

impl Studio {
    /// Fill the concrete scalar target without falling through to layer pixels.
    /// Args: actual document point. Returns true for a targeted alpha or Quick Mask, including refused edits.
    pub(crate) fn fill_channel(&mut self, world: Pt) -> bool {
        if self.channel_target.is_none() && !self.quick_mask_painting() {
            return false;
        }
        let result = (|| {
            self.end_pixel_stroke(false);
            self.capture_channels()?;
            let channel = self
                .channel_paint_target()
                .ok_or("That channel no longer exists")?
                .clone();
            let mut point = tiny_skia::Point::from_xy(world.x, world.y);
            channel
                .plane
                .placement()
                .invert()
                .ok_or("Channel placement is singular")?
                .map_point(&mut point);
            if !point.x.is_finite()
                || !point.y.is_finite()
                || point.x < 0.
                || point.y < 0.
                || point.x >= channel.plane.width as f32
                || point.y >= channel.plane.height as f32
            {
                return Err("Channel fill seed is outside its grid".into());
            }
            let selected = self.channel_selection(&channel.plane)?;
            let value = f32::from(self.mask_brush(false).color.r) / 255.;
            let plane = crate::channels::fill::apply(
                &channel.plane,
                (point.x.floor() as u32, point.y.floor() as u32),
                self.fill_tolerance,
                value,
                self.brush.opacity,
                selected.as_ref().map(|mask| mask.as_slice()),
                &storage::check,
            )?;
            self.replace_scalar_plane(channel.id, plane, "Channel filled")
        })();
        if let Err(error) = result {
            self.status = error;
        }
        true
    }
    fn scalar_channel_mut(&mut self, id: u64, quick: bool) -> Option<&mut Channel> {
        if quick {
            self.doc
                .quick_mask
                .as_mut()
                .map(|mask| &mut mask.channel)
                .filter(|channel| channel.id == id)
        } else {
            self.doc
                .channels
                .iter_mut()
                .find(|channel| channel.id == id)
        }
    }
    /// Read the current saved channel without falling back to layer pixels.
    /// Args: none. Returns the concrete channel while its persistent ID still exists.
    pub(crate) fn channel_paint_target(&self) -> Option<&Channel> {
        match self.channel_target {
            Some(id) => self.doc.channels.iter().find(|c| c.id == id),
            None if self.quick_mask_painting() => {
                self.doc.quick_mask.as_ref().map(|mask| &mask.channel)
            }
            None => None,
        }
    }

    /// Select a concrete saved alpha plane or return to the normal composite.
    /// Args: optional persistent ID. Returns an error before leaving a busy editor or selecting missing data.
    pub(crate) fn select_channel_target(&mut self, id: Option<u64>) -> Result<(), String> {
        self.end_pixel_stroke(true);
        self.capture_channels()?;
        if let Some(id) = id {
            if self.doc.collaboration.is_some() {
                return Err("Saved channel editing is unavailable in Live".into());
            }
            self.doc
                .channels
                .iter()
                .find(|c| c.id == id)
                .ok_or("That saved channel no longer exists")?
                .plane
                .validate()?;
            self.persona = crate::tools::Persona::Pixel;
            if !matches!(
                self.tool,
                Tool::Brush | Tool::Eraser | Tool::Hand | Tool::Zoom
            ) {
                self.set_tool(Tool::Brush);
            }
            self.paint_mask = false;
            self.selection.clear();
            self.selected_layer = None;
            self.node_sel.clear();
            self.artboard_sel.clear();
        }
        self.channel_target = id;
        self.paint_mask = false;
        self.mark_interaction();
        Ok(())
    }

    fn channel_selection(&self, plane: &Plane) -> Result<Option<Arc<Allocation<u8>>>, String> {
        let Some(values) = self.pixel_sel.as_ref() else {
            return Ok(None);
        };
        let source = self
            .pixel_selection_space()
            .ok_or("Clear the damaged selection before editing this channel")?;
        let target = SelectionSpace {
            w: plane.width,
            h: plane.height,
            transform: plane.placement(),
        };
        if source == target {
            return Ok(Some(values.clone()));
        }
        masking::resample_admitted(values, source, target).map(Some)
    }

    /// Start a saved-channel stroke using the existing round or library tip coverage.
    /// Args: actual document point and eraser flag. Returns true whenever a channel owns the input, including refused edits.
    pub(crate) fn start_channel_stroke(&mut self, world: Pt, erase: bool) -> bool {
        if self.channel_target.is_none() && !self.quick_mask_painting() {
            return false;
        }
        let result = (|| {
            self.capture_channels()?;
            if self.doc.collaboration.is_some() {
                return Err("Saved channel editing is unavailable in Live".into());
            }
            let before = self
                .channel_paint_target()
                .ok_or("That saved channel no longer exists")?
                .clone();
            before.plane.validate()?;
            if !valid_brush(&self.brush) {
                return Err("Channel brush extent must be finite".into());
            }
            if !world.x.is_finite() || !world.y.is_finite() {
                return Err("Channel brush position must be finite".into());
            }
            let selection = self.channel_selection(&before.plane)?;
            let coverage = Surface::new(before.plane.width, before.plane.height)?;
            let working = Arc::new(
                crate::resources::global().copy_now(&before.plane.samples, &storage::check)?,
            );
            let quick = self.quick_mask_painting();
            let value = if quick && erase {
                1.
            } else {
                f32::from(self.mask_brush(erase).color.r) / 255.
            };
            self.brush_distance = self.brush.spacing_distance();
            self.brush_heading = 0.;
            self.brush.seed = crate::document::next_id();
            self.op = Some(Op::ChannelStroke(Box::new(Stroke {
                document: self.swap_id.clone(),
                generation: self.canvas_gen,
                selection_generation: self.pixel_sel_gen,
                before,
                quick,
                working,
                coverage,
                selection,
                value,
                opacity: self.brush.opacity.clamp(0., 1.),
                last: None,
            })));
            Ok::<_, String>(())
        })();
        match result {
            Ok(()) => self.drag_channel_stroke(world),
            Err(error) => self.status = error,
        }
        true
    }

    /// Apply an ordered pointer sample without quantizing original scalar values.
    /// Args: actual document point. Returns nothing; failed allocation retains the last valid plane.
    pub(crate) fn drag_channel_stroke(&mut self, world: Pt) {
        if !matches!(self.op, Some(Op::ChannelStroke(_))) {
            return;
        }
        let Some(Op::ChannelStroke(mut stroke)) = self.op.take() else {
            unreachable!()
        };
        let current = stroke.document == self.swap_id
            && stroke.generation == self.canvas_gen
            && stroke.selection_generation == self.pixel_sel_gen
            && self.quick_mask_painting() == stroke.quick
            && self.channel_paint_target().is_some_and(|c| {
                c.id == stroke.before.id
                    && (Arc::ptr_eq(&c.plane.samples, &stroke.working)
                        || (stroke.last.is_none() && c == &stroke.before))
            });
        if !current || !valid_brush(&self.brush) || !world.x.is_finite() || !world.y.is_finite() {
            self.op = Some(Op::ChannelStroke(stroke));
            self.end_channel_stroke(true);
            self.status = "Channel stroke cancelled because its owner changed".into();
            return;
        }
        let mut point = tiny_skia::Point::from_xy(world.x, world.y);
        stroke
            .before
            .plane
            .placement()
            .invert()
            .unwrap()
            .map_point(&mut point);
        let to = Pt::new(point.x, point.y);
        if !to.x.is_finite() || !to.y.is_finite() {
            self.op = Some(Op::ChannelStroke(stroke));
            self.end_channel_stroke(true);
            self.status = "Channel brush position cannot be represented on this grid".into();
            return;
        }
        let from = stroke.last.unwrap_or(to);
        if stroke.last.is_some() && (to - from).length_sq() < 0.0001 {
            self.op = Some(Op::ChannelStroke(stroke));
            return;
        }
        let brush = paint::Brush {
            color: crate::color::Rgba::WHITE,
            opacity: 1.,
            ..self.brush
        };
        let segment = bounded_segment(
            from,
            to,
            stroke.before.plane.width,
            stroke.before.plane.height,
            &brush,
            &mut self.brush_distance,
        );
        let region = if let Some((start, end, tail)) = segment {
            if !(end - start).length().is_finite() {
                self.op = Some(Op::ChannelStroke(stroke));
                self.end_channel_stroke(true);
                self.status = "Channel brush extent cannot be represented on this grid".into();
                return;
            }
            if let Some(tip) = self.brushes.tip() {
                if stroke.last.is_some() {
                    crate::brushes::stroke_spaced(
                        &mut stroke.coverage,
                        start,
                        end,
                        &brush,
                        tip,
                        false,
                        &mut self.brush_distance,
                    );
                } else {
                    crate::brushes::stamp(&mut stroke.coverage, end, &brush, tip, false);
                }
            } else if stroke.last.is_some() {
                paint::stroke_spaced(
                    &mut stroke.coverage,
                    start,
                    end,
                    &brush,
                    false,
                    &mut self.brush_distance,
                );
            } else {
                paint::stamp(&mut stroke.coverage, end, &brush, false);
            }
            if tail > 0. {
                self.brush_distance = phase(self.brush_distance, tail, brush.spacing_distance());
            }
            Some(paint::PixelRegion::stroke(
                stroke.before.plane.width,
                stroke.before.plane.height,
                start,
                end,
                &brush,
            ))
        } else {
            None
        };
        self.scalar_channel_mut(stroke.before.id, stroke.quick)
            .unwrap()
            .plane
            .samples = stroke.before.plane.samples.clone();
        match storage::make_mut(&mut stroke.working, &storage::check) {
            Ok(values) => {
                for row in region
                    .into_iter()
                    .flat_map(|r| r.rows(stroke.before.plane.width))
                {
                    for i in row {
                        let amount = f32::from(stroke.coverage.pixels()[i].alpha()) / 255.
                            * stroke.opacity
                            * stroke
                                .selection
                                .as_ref()
                                .map_or(1., |m| f32::from(m[i]) / 255.);
                        if amount > 0. {
                            let before = stroke.before.plane.samples[i];
                            values[i] = (before + (stroke.value - before) * amount).clamp(0., 1.);
                        }
                    }
                }
                stroke.last = Some(to);
            }
            Err(error) => self.status = error,
        }
        self.scalar_channel_mut(stroke.before.id, stroke.quick)
            .unwrap()
            .plane
            .samples = stroke.working.clone();
        self.mark_interaction();
        stroke.generation = self.canvas_gen;
        self.op = Some(Op::ChannelStroke(stroke));
    }

    /// Commit one stroke or restore its original plane on Escape, Undo or owner change.
    /// Args: cancel flag. Returns true only when a saved-channel gesture was active.
    pub(crate) fn end_channel_stroke(&mut self, cancel: bool) -> bool {
        if !matches!(self.op, Some(Op::ChannelStroke(_))) {
            return false;
        }
        let Some(Op::ChannelStroke(stroke)) = self.op.take() else {
            unreachable!()
        };
        self.paint_contact = None;
        self.brush.pressure = 1.;
        let current = stroke.document == self.swap_id;
        if !current {
            return true;
        }
        let Some(channel) = self.scalar_channel_mut(stroke.before.id, stroke.quick) else {
            return true;
        };
        if !Arc::ptr_eq(&channel.plane.samples, &stroke.working) {
            return true;
        }
        channel.plane = stroke.before.plane.clone();
        if !cancel && stroke.before.plane.samples.as_slice() != stroke.working.as_slice() {
            let plane = Plane {
                samples: stroke.working.clone(),
                ..stroke.before.plane.clone()
            };
            if let Err(error) =
                self.replace_scalar_plane(stroke.before.id, plane, "Channel painted")
            {
                self.status = error;
            }
        } else {
            self.mark_interaction();
        }
        true
    }

    /// Clear coverage only in the targeted alpha channel, respecting soft selection.
    /// Args: none. Returns nothing and never falls back to layer pixels on an error.
    pub(crate) fn clear_channel_selection(&mut self) {
        let result = (|| {
            self.end_pixel_stroke(false);
            let target = self.capture_channels()?;
            let channel = self
                .channel_paint_target()
                .ok_or("That saved channel no longer exists")?;
            let selection = self.channel_selection(&channel.plane)?;
            let mut plane = channel.plane.clone();
            plane.samples = storage::collect(plane.samples.iter().enumerate().map(|(i, v)| {
                if let Some(mask) = selection.as_ref() {
                    *v * (1. - f32::from(mask[i]) / 255.)
                } else {
                    0.
                }
            }))?;
            let id = channel.id;
            let _ = target;
            self.replace_scalar_plane(id, plane, "Channel cleared")
        })();
        if let Err(error) = result {
            self.status = error;
        }
    }
}

#[cfg(test)]
#[path = "channel_paint_tests.rs"]
mod tests;
