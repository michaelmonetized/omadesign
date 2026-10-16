use eframe::egui::{self, Id, InnerResponse, Ui};

/// Draw native controls with an identity independent of earlier conditional rows.
/// Args: parent UI, unique absolute owner ID and native control body.
/// Returns the body result and allocated response, retaining the parent's presentation.
pub fn native_scope<R>(ui: &mut Ui, id: Id, body: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let id = ui
        .data(|data| data.get_temp::<Id>(Id::new("oma-native-view")))
        .map_or(id, |view| id.with(view));
    #[cfg(not(feature = "oma-ui-egui032"))]
    {
        ui.scope_builder(egui::UiBuilder::new().id(id), body)
    }
    #[cfg(feature = "oma-ui-egui032")]
    {
        let mut builder = egui::UiBuilder::new()
            .layer_id(ui.layer_id())
            .max_rect(ui.available_rect_before_wrap())
            .layout(*ui.layout())
            .style(ui.style().clone());
        if !ui.is_enabled() {
            builder = builder.disabled();
        }
        if !ui.is_visible() {
            builder = builder.invisible();
        }
        if ui.is_sizing_pass() {
            builder = builder.sizing_pass();
        }
        let mut child = Ui::new(ui.ctx().clone(), id, builder);
        child.set_clip_rect(ui.clip_rect());
        child.set_opacity(ui.opacity());
        let inner = body(&mut child);
        let rect = child.min_rect();
        drop(child);
        InnerResponse::new(inner, ui.allocate_rect(rect, egui::Sense::hover()))
    }
}

/// Keep simultaneous views of the same property independent.
/// Args: UI, document/view identity and native content. Returns the content result.
pub fn with_native_view<R>(ui: &mut Ui, view: Id, body: impl FnOnce(&mut Ui) -> R) -> R {
    let key = Id::new("oma-native-view");
    let previous = ui.data(|data| data.get_temp::<Id>(key));
    ui.data_mut(|data| data.insert_temp(key, view));
    let result = body(ui);
    ui.data_mut(|data| {
        if let Some(previous) = previous {
            data.insert_temp(key, previous);
        } else {
            data.remove::<Id>(key);
        }
    });
    result
}
