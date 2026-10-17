//! Quick Mask uses the existing scalar stroke and selection owners.
use super::{
    Studio,
    masking::{self, SelectionSpace},
};
use crate::{
    channels::{
        Channel, Plane,
        quick_mask::{Indicates, Options, QuickMask, Selection, SelectionChange},
    },
    color::Rgba,
    deep_color::storage,
    document::Cmd,
    tools::{Persona, Tool},
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub normal: Rgba,
    pub mask: Rgba,
}

impl Studio {
    /// Select the concrete temporary mask without saving it as an alpha channel.
    /// Args: none. Returns an error before changing a busy or missing target.
    pub(crate) fn select_quick_mask_target(&mut self) -> Result<(), String> {
        self.end_pixel_stroke(true);
        self.capture_channels()?;
        if self.doc.collaboration.is_some() {
            return Err("QuickMaskUnsupported: Quick Mask is unavailable in Live".into());
        }
        if self.doc.quick_mask.is_none() {
            return Err("Quick Mask is no longer open".into());
        }
        self.channel_target = None;
        self.paint_mask = false;
        self.persona = Persona::Pixel;
        self.sync_quick_mask_palette();
        self.set_tool(Tool::Brush);
        self.mark_interaction();
        Ok(())
    }
    /// Identify the tools that currently operate on mask coverage or its selection.
    /// Args: requested native tool. Returns false for unsupported layer-only operations.
    pub(crate) fn quick_mask_tool_allowed(tool: Tool) -> bool {
        matches!(
            tool,
            Tool::Brush
                | Tool::Eraser
                | Tool::Fill
                | Tool::Eyedropper
                | Tool::Hand
                | Tool::Zoom
                | Tool::Marquee
                | Tool::EllipseMarquee
                | Tool::Lasso
                | Tool::BezierLasso
        )
    }
    /// Read the temporary mask only while the composite owns scalar painting.
    /// Args: none. Returns true without treating a saved alpha or layer mask as Quick Mask.
    pub(crate) fn quick_mask_tools_limited(&self) -> bool {
        self.persona == Persona::Pixel && self.doc.quick_mask.is_some() && !self.paint_mask
    }
    pub(crate) fn quick_mask_painting(&self) -> bool {
        self.persona == Persona::Pixel
            && self.doc.quick_mask.is_some()
            && self.channel_target.is_none()
            && !self.paint_mask
    }
    /// Keep each document's normal and mask foreground colors across mode switches.
    /// Args: none. Returns nothing; it changes tool preferences, never artwork.
    pub(crate) fn sync_quick_mask_palette(&mut self) {
        if self.doc.quick_mask.is_some() && self.persona == Persona::Pixel {
            if self.quick_mask_palette.is_none() {
                self.quick_mask_defaults = self.doc.quick_mask.as_ref().unwrap().options;
                self.quick_mask_palette = Some(Palette {
                    normal: self.brush.color,
                    mask: Rgba::BLACK,
                });
                self.brush.color = Rgba::BLACK;
            }
        } else if let Some(palette) = self.quick_mask_palette.take() {
            self.brush.color = palette.normal;
        }
    }
    fn selection_snapshot(&self) -> Result<Option<Selection>, String> {
        let Some(values) = self.pixel_sel.as_ref() else {
            return Ok(None);
        };
        let space = self
            .pixel_selection_space()
            .ok_or("Clear the damaged selection before opening Quick Mask")?;
        let t = space.transform;
        Ok(Some(Selection {
            values: values.clone(),
            width: space.w,
            height: space.h,
            transform: [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty],
        }))
    }
    /// Enter or leave Quick Mask with one ordinary, reversible command.
    /// Args: requested mode. Returns false for an unchanged mode and errors before any mutation.
    pub(crate) fn set_quick_mask(&mut self, on: bool) -> Result<bool, String> {
        self.end_pixel_stroke(false);
        self.capture_channels()?;
        if self.doc.quick_mask.is_some() == on {
            return Ok(false);
        }
        if self.doc.collaboration.is_some() {
            return Err("QuickMaskUnsupported: Quick Mask is unavailable in Live".into());
        }
        let old_selection = self.selection_snapshot()?;
        let options = self.quick_mask_defaults;
        options.validate()?;
        let (after, selection) = if on {
            if !self.doc.width.is_finite()
                || !self.doc.height.is_finite()
                || self.doc.width <= 0.
                || self.doc.height <= 0.
                || self.doc.width.ceil() >= u32::MAX as f32
                || self.doc.height.ceil() >= u32::MAX as f32
            {
                return Err("Quick Mask needs finite, addressable canvas dimensions".into());
            }
            let width = self.doc.width.ceil() as u32;
            let height = self.doc.height.ceil() as u32;
            let space = SelectionSpace {
                w: width,
                h: height,
                transform: tiny_skia::Transform::identity(),
            };
            let plane = if let Some(selected) = &old_selection {
                let source = SelectionSpace {
                    w: selected.width,
                    h: selected.height,
                    transform: selected.placement(),
                };
                let values = if source == space {
                    selected.values.clone()
                } else {
                    masking::resample_admitted(&selected.values, source, space)?
                };
                Plane::from_coverage(values, width, height, space.transform)?
            } else {
                let count = storage::count(width, height, 1)?;
                let mut samples =
                    crate::resources::global().allocate_now::<f32, _>(count, &storage::check)?;
                samples
                    .as_mut_slice()
                    .fill(if options.indicates == Indicates::Masked {
                        1.
                    } else {
                        0.
                    });
                Plane {
                    width,
                    height,
                    transform: [1., 0., 0., 1., 0., 0.],
                    samples: Arc::new(samples),
                }
            };
            let mask = QuickMask {
                channel: Channel {
                    id: crate::document::next_id(),
                    name: "Quick Mask".into(),
                    plane,
                },
                unselected: old_selection.is_none(),
                options,
            };
            mask.validate(&self.doc.channels)?;
            (Some(mask), None)
        } else {
            let mask = self.doc.quick_mask.as_ref().unwrap();
            mask.validate(&self.doc.channels)?;
            let selected = if mask.leaves_unselected() {
                None
            } else {
                Some(Selection::from_plane(
                    &mask.channel.plane,
                    mask.channel.plane.samples.clone(),
                ))
            };
            (None, selected)
        };
        self.commit(Cmd::SetQuickMask {
            before: self.doc.quick_mask.clone(),
            after,
            selection: Some(Box::new(SelectionChange {
                before: old_selection,
                after: selection,
            })),
        });
        self.channel_target = None;
        self.paint_mask = false;
        if on {
            self.persona = Persona::Pixel;
        }
        self.sync_quick_mask_palette();
        if on {
            self.selection.clear();
            self.selected_layer = None;
            self.node_sel.clear();
            self.artboard_sel.clear();
            self.set_tool(Tool::Brush);
        }
        self.status = if on {
            "Quick Mask · paint black to mask, erase to select"
        } else {
            "Quick Mask returned to the selection"
        }
        .into();
        Ok(true)
    }
    /// Apply a validated replacement to the concrete scalar target.
    /// Args: captured target ID, completed plane and status. Returns before mutation if its identity changed.
    pub(crate) fn replace_scalar_plane(
        &mut self,
        id: u64,
        plane: Plane,
        message: &str,
    ) -> Result<bool, String> {
        let target = self.capture_channels()?;
        if self.doc.collaboration.is_some() {
            return Err("QuickMaskUnsupported: Quick Mask is unavailable in Live".into());
        }
        plane.validate()?;
        if self.quick_mask_painting() {
            let before = self
                .doc
                .quick_mask
                .clone()
                .ok_or("Quick Mask is no longer open")?;
            if before.channel.id != id {
                return Err("The mask target changed".into());
            }
            let mut after = before.clone();
            after.channel.plane = plane;
            if after == before {
                return Ok(false);
            }
            self.commit(Cmd::SetQuickMask {
                before: Some(before),
                after: Some(after),
                selection: None,
            });
            self.status = message.into();
            Ok(true)
        } else {
            let mut after = self.doc.channels.clone();
            after
                .iter_mut()
                .find(|channel| channel.id == id)
                .ok_or("That channel no longer exists")?
                .plane = plane;
            self.set_channels(&target, after, message)
        }
    }
    /// Change the actual mask overlay without changing the selection meaning.
    /// Args: accepted display options. Returns one reversible command or a validation error.
    pub(crate) fn set_quick_mask_options(&mut self, options: Options) -> Result<bool, String> {
        self.end_pixel_stroke(false);
        self.capture_channels()?;
        if self.doc.collaboration.is_some() {
            return Err("QuickMaskUnsupported: Quick Mask is unavailable in Live".into());
        }
        options.validate()?;
        let Some(before) = self.doc.quick_mask.clone() else {
            let changed = self.quick_mask_defaults != options;
            self.quick_mask_defaults = options;
            self.status = "Quick Mask defaults changed".into();
            return Ok(changed);
        };
        if before.options == options {
            return Ok(false);
        }
        let mut after = before.clone();
        if after.unselected && before.options.indicates != options.indicates {
            let old = if before.options.indicates == Indicates::Masked {
                1.
            } else {
                0.
            };
            if before
                .channel
                .plane
                .samples
                .iter()
                .all(|v| (*v - old).abs() <= 0.5 / 255.)
            {
                after.channel.plane = before.channel.plane.inverted(&storage::check)?;
            }
        }
        after.options = options;
        self.commit(Cmd::SetQuickMask {
            before: Some(before),
            after: Some(after),
            selection: None,
        });
        self.quick_mask_defaults = options;
        self.status = "Quick Mask options changed".into();
        Ok(true)
    }
    /// Restore the exact selection side of a mode command, including nested Undo batches.
    /// Args: applied command. Returns nothing; all byte allocations were admitted before commit.
    pub(super) fn apply_quick_mask_selection(&mut self, command: &Cmd) {
        match command {
            Cmd::SetQuickMask {
                selection: Some(change),
                ..
            } => {
                if let Some(selected) = &change.after {
                    self.replace_pixel_selection_admitted(
                        selected.values.clone(),
                        SelectionSpace {
                            w: selected.width,
                            h: selected.height,
                            transform: selected.placement(),
                        },
                    );
                } else {
                    self.set_pixel_sel(None);
                }
                self.pixel_edit = None;
                self.pixel_path = None;
            }
            Cmd::Batch(commands) => {
                for command in commands {
                    self.apply_quick_mask_selection(command);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "quick_mask_tests.rs"]
pub(crate) mod tests;
