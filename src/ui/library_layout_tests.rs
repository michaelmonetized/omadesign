//! Repeated real frames catch panel expansion caused by overflowing children.
use super::*;

#[test]
fn populated_libraries_keep_their_width_and_canvas_space() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fieldwork");
    let palettes = crate::palette::load_file(root.join(".omacolors")).unwrap();
    let catalog = Arc::new(crate::brand::scan(&root.join(".omabrand")).unwrap());
    for size in [vec2(960., 640.), vec2(1600., 900.)] {
        for sidebar in [Sidebar::Brand, Sidebar::Palettes] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut studio = Studio::new();
            studio.libraries.sidebar = sidebar;
            studio.libraries.root = Some(root.clone());
            studio.libraries.catalog = Some(catalog.clone());
            studio.libraries.brand_name = catalog.name.clone();
            studio.libraries.project_scope = true;
            studio.libraries.draft().receive(palettes.clone(), Some(1));
            let mut textures = HashMap::new();
            for asset in &catalog.assets {
                let image = crate::brand::load_thumbnail(&catalog.root, asset, 64).unwrap();
                let key = format!("{:?}", asset.fingerprint());
                let texture = ctx.load_texture(
                    key.clone(),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.w as usize, image.h as usize],
                        &image.data,
                    ),
                    egui::TextureOptions::LINEAR,
                );
                textures.insert(key, Ok(texture));
            }
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("brand-preview-cache"),
                    Previews {
                        root: Some(catalog.root.clone()),
                        catalog: Some(catalog.clone()),
                        textures: Arc::new(textures),
                    },
                )
            });
            let mut first_canvas = None;
            for frame in 0..32 {
                if frame == 16 {
                    // Real names must scroll/truncate inside controls, never resize the panel.
                    studio.libraries.brand_name =
                        "Fieldwork / a thoughtfully named collection of reusable artwork".into();
                    let draft = studio.libraries.draft();
                    draft.palettes[0].name =
                        "Fieldwork / thoughtful colors for an unusually long project name".into();
                    draft.selected_name();
                }
                let mut canvas_width = 0.0;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(frame as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| {
                        crate::ui::studios::right_panel(ui, &mut studio);
                        canvas_width = ui.available_width();
                    },
                );
                output.textures_delta.clear();
                let width = size.x - canvas_width;
                assert!(
                    (287.0..=290.0).contains(&width),
                    "{sidebar:?} at {size:?}, frame {frame}: panel became {width}px"
                );
                let initial = *first_canvas.get_or_insert(canvas_width);
                assert!(
                    (initial - canvas_width).abs() <= 1.0,
                    "{sidebar:?} stole canvas width after fitting"
                );
            }
        }
    }
}
