use super::*;
use crate::{
    app::Studio,
    color::Rgba,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "omadesign-type-test-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fonts() -> Vec<PathBuf> {
    crate::text::fonts()
        .iter()
        .filter(|face| face.path.extension().is_some_and(|ext| ext == "ttf"))
        .take(2)
        .map(|face| face.path.clone())
        .collect()
}

#[test]
fn portable_validation_rollback_and_refresh_preserve_original_font_bytes() {
    let temp = Temp::new();
    let fonts = fonts();
    assert_eq!(fonts.len(), 2, "two installed test fonts are required");
    let root = temp.0.join("kit");
    // Match size and mtime across an atomic replacement: file identity must
    // invalidate the cache, as it does when a sync tool preserves timestamps.
    let mut original = fs::read(&fonts[0]).unwrap();
    let mut replacement = fs::read(&fonts[1]).unwrap();
    let padded_size = original.len().max(replacement.len());
    original.resize(padded_size, 0);
    replacement.resize(padded_size, 0);
    let source_font = temp.0.join("source.ttf");
    fs::write(&source_font, original).unwrap();
    let first = add_fonts(&root, &[source_font], None).unwrap();
    register(&first).unwrap();
    assert_eq!(decode(&encode(&first.kit).unwrap()).unwrap(), first.kit);
    assert!(first.kit.roles[0].font.starts_with("fonts/"));
    for path in [
        "/tmp/font.ttf",
        "fonts/../outside.ttf",
        "fonts/linked/../../font.ttf",
        "fonts\\font.ttf",
    ] {
        let mut kit = first.kit.clone();
        kit.roles[0].font = path.into();
        assert!(encode(&kit).is_err(), "{path}");
    }
    let mut duplicate = first.kit.clone();
    duplicate.roles.push(duplicate.roles[0].clone());
    assert!(encode(&duplicate).is_err());
    let before = fs::read(root.join(FILE)).unwrap();
    let bad = temp.0.join("broken.ttf");
    fs::write(&bad, "not a font").unwrap();
    assert!(add_fonts(&root, &[fonts[1].clone(), bad], Some(first.stamp)).is_err());
    assert_eq!(fs::read(root.join(FILE)).unwrap(), before);
    assert_eq!(
        fs::read_dir(root.join(".omabrand/fonts")).unwrap().count(),
        1
    );

    let run = TypeRun {
        content: "Refresh Ω".into(),
        font: first.fonts[0].id.clone(),
        px: 30.0,
        ..Default::default()
    };
    let original_outline = crate::text::shape(&run);
    let file = root.join(".omabrand").join(&first.kit.roles[0].font);
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    let replacement_path = file.with_extension("replacement");
    fs::write(&replacement_path, replacement).unwrap();
    fs::File::open(&replacement_path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::rename(&replacement_path, &file).unwrap();
    let refreshed = load(&root).unwrap().unwrap();
    assert_ne!(first.fonts[0].id, refreshed.fonts[0].id);
    register(&refreshed).unwrap();
    assert_eq!(
        crate::text::shape(&run),
        original_outline,
        "an applied face stays immutable until explicitly reapplied"
    );
    let mut newer = run.clone();
    newer.font = refreshed.fonts[0].id.clone();
    assert_ne!(crate::text::shape(&newer), original_outline);
    let mut changed = first.kit.clone();
    changed.name = "Other writer".into();
    save(&root, &changed, Some(first.stamp)).unwrap();
    assert!(save(&root, &first.kit, Some(first.stamp)).is_err());
    assert_eq!(load(&root).unwrap().unwrap().kit.name, "Other writer");
}

#[test]
fn project_fonts_copy_reopen_and_edit_without_an_installed_font_path() {
    // A fresh test process ensures a cached font from the source kit cannot hide
    // a portability bug when the original project folder has been deleted.
    if let Some(path) = std::env::var_os("OMADESIGN_TYPE_REOPEN_TEST") {
        let path = PathBuf::from(path);
        let doc = crate::project::load_from(&path).unwrap();
        let mut studio = Studio::new();
        studio.doc = doc;
        let id = studio.doc.layers[0].kind.shapes().unwrap()[0].id;
        studio.selection = vec![(0, id)];
        let run = match &studio.doc.find_shape(0, id).unwrap().geom {
            Geom::Text(run) => run,
            _ => panic!(),
        };
        assert!(run.font.starts_with("omatype:") && crate::text::project_font_loaded(&run.font));
        let old = run.contours.clone();
        studio.patch_type(|run| run.content.push_str(" Ω123"));
        let run = match &studio.doc.find_shape(0, id).unwrap().geom {
            Geom::Text(run) => run,
            _ => panic!(),
        };
        assert!(!run.contours.is_empty() && run.contours != old);
        assert_eq!(run.contours, crate::text::shape(run));
        return;
    }
    let temp = Temp::new();
    let source = temp.0.join("source");
    let moved = temp.0.join("moved");
    let kit = add_fonts(&source, &fonts()[..1], None).unwrap();
    let mut studio = Studio::new();
    studio.doc = Document::new("Type travel", 1.0, 1.0, 72.0);
    studio.doc.layers = vec![Layer::vector("Type")];
    let shape = Shape::new(
        Geom::Text(TypeRun {
            content: "Portable".into(),
            origin: Pt::new(5.0, 35.0),
            px: 22.0,
            ..Default::default()
        }),
        Style {
            fill: crate::document::Fill::Solid(Rgba::BLACK),
            ..Default::default()
        },
    );
    let id = shape.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.selection = vec![(0, id)];
    studio.apply_project_font(&kit.fonts[0]).unwrap();
    studio.undo();
    assert!(
        matches!(&studio.doc.find_shape(0,id).unwrap().geom, Geom::Text(run) if run.font.is_empty())
    );
    studio.redo();
    let artwork = source.join("artwork.oma");
    crate::project::save_to(&studio.doc, &artwork).unwrap();
    let saved = fs::read_to_string(&artwork).unwrap();
    assert!(saved.contains(&kit.fonts[0].id));
    assert!(!saved.contains(&source.to_string_lossy().to_string()));
    // Removing the role keeps its archived font available to existing artwork.
    let mut without_roles = kit.kit.clone();
    without_roles.roles.clear();
    save(&source, &without_roles, Some(kit.stamp)).unwrap();
    export_copy(&source, &moved).unwrap();
    fs::copy(&artwork, moved.join("artwork.oma")).unwrap();
    fs::remove_dir_all(&source).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "typography::tests::project_fonts_copy_reopen_and_edit_without_an_installed_font_path",
            "--nocapture",
        ])
        .env("OMADESIGN_TYPE_REOPEN_TEST", moved.join("artwork.oma"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fresh process failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
