use crate::{app::Studio, document::Artboard, geom::Pt};
use eframe::egui::{self, Id};
const ID: &str = "new-artboard-size";
#[derive(Clone)]
struct Dialog {
    owner: String,
    origin: Pt,
    width: f32,
    height: f32,
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID)).is_some())
}
pub(super) fn open(ctx: &egui::Context, studio: &Studio, origin: Pt) {
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new(ID),
            Dialog {
                owner: studio.swap_id.clone(),
                origin,
                width: studio.custom_w,
                height: studio.custom_h,
            },
        )
    });
}
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut dialog) = ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))) else {
        return;
    };
    if dialog.owner != studio.swap_id {
        ctx.data_mut(|d| d.remove::<Dialog>(Id::new(ID)));
        return;
    }
    let mut create = false;
    let mut cancel = false;
    let response = egui::Modal::new(Id::new("artboard-size-modal")).show(ctx, |ui| {
        ui.heading("New artboard");
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut dialog.width)
                    .range(1.0..=65535.)
                    .prefix("Width  ")
                    .suffix(" px"),
            );
            ui.add(
                egui::DragValue::new(&mut dialog.height)
                    .range(1.0..=65535.)
                    .prefix("Height  ")
                    .suffix(" px"),
            );
        });
        ui.horizontal(|ui| {
            create = ui.button("Create artboard").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if create {
        let mut board = Artboard::new(
            studio.doc.artboards.len(),
            dialog.origin,
            Pt::new(dialog.width, dialog.height),
        );
        board.name = studio.doc.unique_artboard_name(&board.name);
        let id = board.id;
        let mut after = studio.doc.artboards.clone();
        after.push(board);
        studio.commit_artboards(after);
        studio.artboard_sel = vec![id];
    }
    let close = create || cancel || response.should_close();
    ctx.data_mut(|d| {
        if close {
            d.remove::<Dialog>(Id::new(ID));
        } else {
            d.insert_temp(Id::new(ID), dialog);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artboard_modal_can_render_and_close_without_nested_context_locks() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        open(&ctx, &studio, Pt::ZERO);
        let mut output = ctx.run_ui(Default::default(), |ui| show(ui.ctx(), &mut studio));
        output.textures_delta.clear();
        assert!(is_open(&ctx));
        let input = egui::RawInput { events: vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }], ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| show(ui.ctx(), &mut studio));
        output.textures_delta.clear();
        assert!(!is_open(&ctx));
    }
}
