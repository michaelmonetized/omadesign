//! Attach inspector commands to the field and gesture that produced them.
use super::Studio;
use eframe::egui::{self, Id, Response};

const PENDING: &str = "inspector-field-change";

#[derive(Clone, Copy, Default)]
struct Session {
    serial: u64,
    pointer: bool,
    focused: bool,
}

/// Track an existing property widget without changing its editing behavior.
/// Args: the widget response. Returns whether its value changed this frame.
pub(crate) fn changed(response: Response) -> bool {
    let accepted = response.changed();
    record(response, accepted)
}

/// Observe a property response while respecting the field's accepted edit gate.
/// Args: native response and accepted change flag. Returns that accepted flag.
pub(crate) fn record(response: Response, accepted: bool) -> bool {
    let ctx = &response.ctx;
    let key = response.id.with("input-history");
    let mut session = ctx
        .data(|data| data.get_temp::<Session>(key))
        .unwrap_or_default();
    let pointer = response.is_pointer_button_down_on() || response.dragged();
    if (pointer && !session.pointer) || (response.has_focus() && !session.focused) {
        session.serial = ctx.cumulative_frame_nr();
    }
    if accepted
        && !pointer
        && !response.has_focus()
        && !response.lost_focus()
        && !response.drag_stopped()
    {
        session.serial = ctx.cumulative_frame_nr();
    }
    session.pointer = pointer;
    session.focused = response.has_focus();
    ctx.data_mut(|data| {
        data.insert_temp(key, session);
        if accepted {
            data.insert_temp(Id::new(PENDING), (response.id, session.serial));
        }
    });
    accepted
}

impl Studio {
    /// Keep widget text history and commands with their document and property owner.
    /// Args: property UI, stable property identity and drawing callback.
    /// Returns the callback result with field tagging limited to that operation.
    pub(crate) fn with_field_history<R>(
        &mut self,
        ui: &mut egui::Ui,
        property: impl std::hash::Hash + std::fmt::Debug,
        draw: impl FnOnce(&mut egui::Ui, &mut Self) -> R,
    ) -> R {
        let ctx = ui.ctx().clone();
        let owner = self.field_history_owner().with(property);
        let previous = self.field_history_context.replace(ctx.clone());
        let pending = ctx.data_mut(|data| {
            let pending = data.get_temp::<(Id, u64)>(Id::new(PENDING));
            data.remove::<(Id, u64)>(Id::new(PENDING));
            pending
        });
        let result = ui.push_id(owner, |ui| draw(ui, self)).inner;
        ctx.data_mut(|data| {
            data.remove::<(Id, u64)>(Id::new(PENDING));
            if let Some(pending) = pending {
                data.insert_temp(Id::new(PENDING), pending);
            }
        });
        self.field_history_context = previous;
        result
    }

    /// Identify the document and selection currently owning inspector input.
    /// Args: none. Returns a stable UI identity independent of property values.
    fn field_history_owner(&self) -> Id {
        Id::new((
            &self.swap_id,
            &self.selection,
            self.selected_layer,
            self.active_layer
                .and_then(|i| self.doc.layers.get(i).map(|l| l.id)),
            &self.artboard_sel,
            self.persona,
        ))
    }

    /// Consume the next field command's owner after checking document changes.
    /// Args: none. Returns its gesture identity, or none for ordinary commands.
    pub(super) fn take_field_history_key(&mut self) -> Option<u64> {
        let ctx = self.field_history_context.as_ref()?;
        let field = ctx.data_mut(|data| {
            let pending = data.get_temp::<(Id, u64)>(Id::new(PENDING));
            data.remove::<(Id, u64)>(Id::new(PENDING));
            pending
        })?;
        if self.field_history_generation != Some(self.canvas_gen) {
            self.history.finish_field();
        }
        Some(self.field_history_owner().with(field).value())
    }
}
