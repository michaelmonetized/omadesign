//! Channel actions retain their captured document, selection and allocation owners.
use super::{
    Studio,
    masking::{self, SelectionSpace},
};
use crate::{
    channels::{self, Channel, Plane},
    document::{self, Cmd},
    resources::{Allocation, CancellationCheck},
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Target {
    document: String,
    generation: u64,
    selection: u64,
}
fn available(s: &Studio) -> Result<(), String> {
    if s.op.is_some()
        || s.type_edit.is_some()
        || s.free_transform.is_some()
        || s.deformation.is_some()
        || s.pending_nav.is_some()
        || s.file_dialog_pending()
        || s.updates.freezing
    {
        return Err("Finish the current edit before changing channels".into());
    }
    Ok(())
}
impl Studio {
    /// Capture the real document and selection before any channel worker starts.
    /// Args: none. Returns an owner guard or a busy error; focus never mutates data.
    pub(crate) fn capture_channels(&self) -> Result<Target, String> {
        available(self)?;
        Ok(Target {
            document: self.swap_id.clone(),
            generation: self.canvas_gen,
            selection: self.pixel_sel_gen,
        })
    }
    /// Check the captured document and selection before installing a worker result.
    /// Args: original owner guard. Returns true only while both sources remain current.
    pub(crate) fn channels_current(&self, target: &Target) -> bool {
        available(self).is_ok()
            && target.document == self.swap_id
            && target.generation == self.canvas_gen
            && target.selection == self.pixel_sel_gen
    }
    /// Commit complete validated channel changes in one ordinary Undo transaction.
    /// Args: captured owner, immutable replacement and success message. Returns before mutation on stale or unsupported sources.
    pub(crate) fn set_channels(
        &mut self,
        target: &Target,
        after: Vec<Channel>,
        message: &str,
    ) -> Result<bool, String> {
        if !self.channels_current(target) {
            return Err("The document or selection changed; use the current channels".into());
        }
        if self.doc.collaboration.is_some() {
            return Err("Saved channel editing is unavailable in Live".into());
        }
        channels::validate(&after)?;
        if let Some(mask)=&self.doc.quick_mask {mask.validate_metadata(&after)?;}
        if self.doc.channels == after {
            return Ok(false);
        }
        self.commit(Cmd::SetChannels {
            before: self.doc.channels.clone(),
            after,
        });
        self.status = message.into();
        Ok(true)
    }
    /// Save a full scalar plane as a named alpha channel without changing artwork.
    /// Args: captured owner, produced plane and desired name. Returns the persistent channel ID after one command.
    pub(crate) fn save_channel(
        &mut self,
        target: &Target,
        plane: Plane,
        name: &str,
    ) -> Result<u64, String> {
        let name = if name.trim().is_empty() {
            channels::next_name(&self.doc.channels, "Alpha")
        } else {
            name.trim().into()
        };
        let id = document::next_id();
        let mut after = self.doc.channels.clone();
        after.push(Channel { id, name, plane });
        self.set_channels(target, after, "Selection saved as an alpha channel")?;
        Ok(id)
    }
    /// Rename only the current channel name rather than overwriting a concurrent edit.
    /// Args: captured owner, channel ID, expected name and accepted replacement. Returns whether one command was needed.
    pub(crate) fn rename_channel(
        &mut self,
        target: &Target,
        id: u64,
        expected: &str,
        name: &str,
    ) -> Result<bool, String> {
        let mut after = self.doc.channels.clone();
        let channel = after
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("That channel no longer exists")?;
        if channel.name != expected {
            return Err("The channel name changed; edit its current name".into());
        }
        channel.name = name.trim().into();
        self.set_channels(target, after, "Channel renamed")
    }
    /// Duplicate a channel with a new identity while sharing immutable admitted samples.
    /// Args: captured owner and current channel ID. Returns the new ID after one Undo step.
    pub(crate) fn duplicate_channel(&mut self, target: &Target, id: u64) -> Result<u64, String> {
        let mut channel = self
            .doc
            .channels
            .iter()
            .find(|c| c.id == id)
            .ok_or("That channel no longer exists")?
            .clone();
        channel.id = document::next_id();
        channel.name = channels::next_name(&self.doc.channels, "Alpha copy");
        let id = channel.id;
        let mut after = self.doc.channels.clone();
        after.push(channel);
        self.set_channels(target, after, "Channel duplicated")?;
        Ok(id)
    }
    /// Remove one saved channel without deleting any layer pixels or selection.
    /// Args: captured owner and channel ID. Returns before mutation if it is stale or missing.
    pub(crate) fn remove_channel(&mut self, target: &Target, id: u64) -> Result<bool, String> {
        let mut after = self.doc.channels.clone();
        let before = after.len();
        after.retain(|c| c.id != id);
        if after.len() == before {
            return Err("That channel no longer exists".into());
        }
        self.set_channels(target, after, "Channel removed")
    }
    /// Install an admitted channel selection without copying it or releasing its lease.
    /// Args: captured source, completed selection and coordinate system. Returns whether the source is still current.
    pub(crate) fn load_channel_selection(
        &mut self,
        target: &Target,
        values: Arc<Allocation<u8>>,
        space: SelectionSpace,
    ) -> Result<(), String> {
        if !self.channels_current(target) {
            return Err("The document or selection changed; load the current channel".into());
        }
        if !self.replace_pixel_selection_admitted(values, space) {
            return Err(self.status.clone());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Combine {
    Replace,
    Add,
    Subtract,
    Intersect,
}
impl Combine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Replace => "Replace",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
        }
    }
}
/// Combine soft channel coverage in the existing selection's actual coordinate system.
/// Args: source plane, optional original selection, accepted operation and cancellation. Returns admitted bytes and their placement.
pub(crate) fn selection(
    plane: &Plane,
    existing: Option<(Arc<Allocation<u8>>, SelectionSpace)>,
    operation: Combine,
    cancel: &dyn CancellationCheck,
) -> Result<(Arc<Allocation<u8>>, SelectionSpace), String> {
    let next = plane.mask(cancel)?;
    let source = SelectionSpace {
        w: plane.width,
        h: plane.height,
        transform: plane.placement(),
    };
    if operation == Combine::Replace {
        return Ok((next, source));
    }
    let Some((before, target)) = existing else {
        return if operation == Combine::Add {
            Ok((next, source))
        } else {
            Err("Make a selection before subtracting or intersecting a channel".into())
        };
    };
    if !channels::valid_placement(target.transform)
        || before.len() != crate::deep_color::storage::count(target.w, target.h, 1)?
    {
        return Err("Selection dimensions changed".into());
    }
    let next = masking::resample_admitted(&next, source, target)?;
    let mut out = crate::resources::global().allocate_now::<u8, _>(before.len(), cancel)?;
    for (i, ((a, b), dst)) in before
        .iter()
        .zip(next.iter())
        .zip(out.as_mut_slice())
        .enumerate()
    {
        if i % 16384 == 0 {
            cancel.check()?;
        }
        *dst = match operation {
            Combine::Add => (*a).max(*b),
            Combine::Subtract => a.saturating_sub(*b),
            Combine::Intersect => (*a).min(*b),
            Combine::Replace => *b,
        };
    }
    Ok((Arc::new(out), target))
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
