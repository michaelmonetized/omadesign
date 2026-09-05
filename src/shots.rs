//! Feature scenes for product shots. Real chrome, real documents.

use crate::app::Studio;
use crate::color::Rgba;
use crate::document::{Cmd, Fill, Shape, Style};
use crate::geom::{Geom, Pt, TypeRun};
use crate::paint::Brush;
use crate::tools::{Persona, Tool};

pub struct Scene {
    pub id: &'static str,
    pub caption: &'static str,
}

pub const SCENES: &[Scene] = &[
    Scene {
        id: "welcome",
        caption: "New document. Your sizes. Your theme. Sit down and work.",
    },
    Scene {
        id: "design",
        caption: "Design — move, scale, rotate. Phosphor well. Omarchy chrome.",
    },
    Scene {
        id: "type",
        caption: "Type you type into. Character studio. Your fonts, live.",
    },
    Scene {
        id: "pixel",
        caption: "Pixel — brush, clone, wand. Same document, raster layer.",
    },
    Scene {
        id: "photo",
        caption: "Photo — develop, crop, Place in Design.",
    },
    Scene {
        id: "colour",
        caption: "Colour — real picker, palettes, fill and stroke.",
    },
    Scene {
        id: "boolean",
        caption: "Boolean and compound. Union, subtract, combine.",
    },
    Scene {
        id: "shapes",
        caption: "Shape library — Phosphor on the artboard in one click.",
    },
    Scene {
        id: "keys",
        caption: "F1. The keys already in your fingers.",
    },
    Scene {
        id: "poster",
        caption: "One document. Design, paint, photograph. Native Linux.",
    },
    Scene {
        id: "motion",
        caption: "Motion — timeline, keys, Lottie out. Space plays.",
    },
];

pub fn apply(studio: &mut Studio, id: &str) -> Result<(), String> {
    match id {
        "welcome" => welcome(studio),
        "design" => design(studio),
        "type" => type_scene(studio),
        "pixel" => pixel(studio),
        "photo" => photo(studio),
        "colour" => colour(studio),
        "boolean" => boolean(studio),
        "shapes" => shapes(studio),
        "keys" => keys(studio),
        "poster" => poster(studio),
        "motion" => motion(studio),
        other => return Err(format!("unknown scene: {other}")),
    }
    Ok(())
}

fn welcome(s: &mut Studio) {
    *s = Studio::new();
    s.show_welcome = true;
    s.new_doc_group = "Web".into();
}

fn design(s: &mut Studio) {
    s.seed_demo();
    s.persona = Persona::Design;
    s.tool = Tool::Select;
    s.show_welcome = false;
    s.show_rulers = true;
    if let Some(shapes) = s.doc.layers.get(1).and_then(|l| l.kind.shapes())
        && let Some(star) = shapes
            .iter()
            .find(|sh| matches!(sh.geom, Geom::Star { .. }))
    {
        s.selection = vec![(1, star.id)];
    }
    s.need_fit = true;
    s.status = "Move V · handles scale · the top handle rotates".into();
}

fn type_scene(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Type", 1440.0, 900.0, 72.0);
    s.persona = Persona::Design;
    s.tool = Tool::Text;
    let navy = Rgba::from_hex(0x11111B);
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, navy, 0.0);
    add_text(
        s,
        Pt::new(96.0, 280.0),
        "OMARCHY",
        128.0,
        Rgba::from_hex(0xCDD6F4),
        -2.0,
    );
    s.place_text(Pt::new(100.0, 420.0));
    s.type_insert("the linux your fingers already know");
    if let Some(e) = &mut s.type_edit {
        let n = s
            .doc
            .find_shape(e.layer, e.id)
            .and_then(|sh| match &sh.geom {
                Geom::Text(t) => Some(t.content.chars().count()),
                _ => None,
            })
            .unwrap_or(0);
        e.caret = n;
        e.anchor = n;
    }
    add_text(
        s,
        Pt::new(100.0, 620.0),
        "live caret  ·  tracking  ·  OpenType  ·  Google Fonts",
        22.0,
        Rgba::from_hex(0x89B4FA),
        1.5,
    );
    s.need_fit = true;
    s.status = "type — Esc finishes, Enter a new line".into();
}

fn pixel(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Paint", 1440.0, 900.0, 72.0);
    s.persona = Persona::Pixel;
    s.tool = Tool::Brush;
    s.active_layer = Some(0);
    if let Some(px) = s.doc.layers[0].kind.pixels_mut()
        && let Some(mut pm) = px.to_pixmap()
    {
        for (color, y) in [
            (0x89B4FAu32, 220.0),
            (0xF38BA8, 340.0),
            (0xA6E3A1, 460.0),
            (0xF9E2AF, 580.0),
            (0xCBA6F7, 700.0),
        ] {
            let b = Brush {
                size: 72.0,
                hardness: 0.25,
                opacity: 0.85,
                flow: 0.9,
                spacing: 0.12,
                color: Rgba::from_hex(color),
            };
            crate::paint::stroke_to(
                &mut pm,
                Pt::new(80.0, y),
                Pt::new(1360.0, y + 40.0),
                &b,
                false,
            );
        }
        *px = crate::document::Pixels::from_pixmap(&pm);
    }
    s.brush.size = 48.0;
    s.brush.hardness = 0.35;
    s.need_fit = true;
    s.status = "Paint on the pixel layer. [ ] size · Shift+[ ] hardness.".into();
}

fn photo(s: &mut Studio) {
    s.show_welcome = false;
    s.persona = Persona::Photo;
    s.tool = Tool::Hand;
    s.photo.import_samples();
    s.status = "Develop, then Place in Design".into();
}

fn colour(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Colour", 1440.0, 900.0, 72.0);
    s.persona = Persona::Design;
    s.tool = Tool::Select;
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, Rgba::from_hex(0x1E1E2E), 0.0);
    let swatches = [
        0xF38BA8u32,
        0xFAB387,
        0xF9E2AF,
        0xA6E3A1,
        0x94E2D5,
        0x89B4FA,
        0xB4BEFE,
        0xCBA6F7,
    ];
    for (i, hex) in swatches.iter().enumerate() {
        let x = 80.0 + (i as f32 % 4.0) * 220.0;
        let y = 120.0 + (i as f32 / 4.0).floor() * 280.0;
        add_rect(s, x, y, 200.0, 200.0, Rgba::from_hex(*hex), 24.0);
    }
    if let Some(shapes) = s.doc.layers.get(1).and_then(|l| l.kind.shapes())
        && let Some(last) = shapes.last()
    {
        s.selection = vec![(1, last.id)];
        s.style.fill = last.style.fill.clone();
    }
    s.fill_active = true;
    s.need_fit = true;
    s.status = "Click the fill well. Palettes live in ~/.config/omadesign.".into();
}

fn boolean(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Boolean", 1440.0, 900.0, 72.0);
    s.persona = Persona::Design;
    s.tool = Tool::Select;
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, Rgba::from_hex(0x11111B), 0.0);
    let a = Shape::new(
        Geom::Ellipse {
            center: Pt::new(620.0, 450.0),
            radii: Pt::splat(220.0),
        },
        Style {
            fill: Fill::Solid(Rgba::from_hex(0x89B4FA)),
            stroke: None,
        },
    );
    let b = Shape::new(
        Geom::Ellipse {
            center: Pt::new(820.0, 450.0),
            radii: Pt::splat(220.0),
        },
        Style {
            fill: Fill::Solid(Rgba::from_hex(0xF38BA8)),
            stroke: None,
        },
    );
    let ia = a.id;
    let ib = b.id;
    s.commit(Cmd::AddShape { layer: 1, shape: a });
    s.commit(Cmd::AddShape { layer: 1, shape: b });
    s.selection = vec![(1, ia), (1, ib)];
    s.need_fit = true;
    s.status = "Two shapes. Union, subtract, intersect, XOR. Combine Ctrl+G.".into();
}

fn shapes(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Shapes", 1440.0, 900.0, 72.0);
    s.persona = Persona::Design;
    s.tool = Tool::Select;
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, Rgba::from_hex(0x181825), 0.0);
    s.style.fill = Fill::Solid(Rgba::from_hex(0xCDD6F4));
    s.style.stroke = None;
    let icons = ["heart", "star", "camera", "globe", "lightning", "cube"];
    for (i, name) in icons.iter().enumerate() {
        let icon = crate::shape_browser::Icon {
            name,
            lib: "Phosphor",
        };
        let x = 140.0 + (i as f32 % 3.0) * 280.0;
        let y = 160.0 + (i as f32 / 3.0).floor() * 300.0;
        if let Ok(mut geom) = crate::shape_browser::icon_to_geom(&icon, 160.0) {
            let off = Pt::new(x, y) - geom.bbox().min;
            geom.translate(off);
            s.commit(Cmd::AddShape {
                layer: 1,
                shape: Shape::new(geom, s.style.clone()),
            });
        } else {
            add_rect(s, x, y, 160.0, 160.0, Rgba::from_hex(0x89B4FA), 16.0);
        }
    }
    s.show_shape_browser = false;
    s.shape_lib = "Phosphor".into();
    s.need_fit = true;
    s.status = "Phosphor, LineIcons, Heroicons, Feather.".into();
}

fn keys(s: &mut Studio) {
    design(s);
    s.show_shortcuts = true;
}

fn poster(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("omadesign", 1440.0, 900.0, 72.0);
    s.persona = Persona::Design;
    s.tool = Tool::Select;
    let navy = Rgba::from_hex(0x073B4C);
    let orange = Rgba::from_hex(0xF47C2E);
    let cream = Rgba::from_hex(0xF4EDE4);
    let teal = Rgba::from_hex(0x2EC4B6);
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, navy, 0.0);
    let mark = Shape::new(
        Geom::Star {
            center: Pt::new(320.0, 420.0),
            outer: Pt::splat(150.0),
            inner: 0.42,
            points: 5,
        },
        Style {
            fill: Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 1.0],
                c0: orange,
                c1: Rgba::from_hex(0xE5484D),
            },
            stroke: None,
        },
    );
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: mark,
    });
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: Shape::new(
            Geom::Ellipse {
                center: Pt::new(320.0, 420.0),
                radii: Pt::splat(54.0),
            },
            Style {
                fill: Fill::Solid(cream),
                stroke: None,
            },
        ),
    });
    add_text(s, Pt::new(520.0, 400.0), "omadesign", 92.0, cream, -1.0);
    add_text(
        s,
        Pt::new(524.0, 470.0),
        "design  ·  paint  ·  photograph",
        26.0,
        teal,
        1.2,
    );
    add_text(
        s,
        Pt::new(524.0, 560.0),
        "Native Linux. Your theme. Your keys.",
        22.0,
        Rgba::from_hex(0x8EBBD4),
        0.4,
    );
    if let Some(px) = s.doc.layers[0].kind.pixels_mut()
        && let Some(mut pm) = px.to_pixmap()
    {
        let b = Brush {
            size: 80.0,
            hardness: 0.15,
            opacity: 0.28,
            flow: 0.8,
            spacing: 0.2,
            color: orange,
        };
        crate::paint::stroke_to(
            &mut pm,
            Pt::new(980.0, 80.0),
            Pt::new(1380.0, 820.0),
            &b,
            false,
        );
        *px = crate::document::Pixels::from_pixmap(&pm);
    }
    if let Some(shapes) = s.doc.layers.get(1).and_then(|l| l.kind.shapes())
        && let Some(word) = shapes.iter().find(|sh| match &sh.geom {
            Geom::Text(t) => t.content == "omadesign",
            _ => false,
        })
    {
        s.selection = vec![(1, word.id)];
    }
    s.need_fit = true;
    s.status = "One document. One layer stack. No Electron.".into();
}

fn motion(s: &mut Studio) {
    s.show_welcome = false;
    s.doc = crate::document::Document::new("Motion", 1440.0, 900.0, 72.0);
    s.persona = Persona::Motion;
    s.tool = Tool::Select;
    add_rect(s, 0.0, 0.0, 1440.0, 900.0, Rgba::from_hex(0x11111B), 0.0);
    let cream = Rgba::from_hex(0xF4EDE4);
    let orange = Rgba::from_hex(0xF47C2E);
    let teal = Rgba::from_hex(0x2EC4B6);
    let star = Shape::new(
        Geom::Star {
            center: Pt::new(520.0, 380.0),
            outer: Pt::splat(120.0),
            inner: 0.42,
            points: 5,
        },
        Style {
            fill: Fill::Solid(orange),
            stroke: None,
        },
    );
    let star_id = star.id;
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: star,
    });
    let disc = Shape::new(
        Geom::Ellipse {
            center: Pt::new(920.0, 420.0),
            radii: Pt::splat(90.0),
        },
        Style {
            fill: Fill::Solid(teal),
            stroke: None,
        },
    );
    let disc_id = disc.id;
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: disc,
    });
    add_text(s, Pt::new(480.0, 640.0), "omadesign", 56.0, cream, -1.0);
    let mut clip = crate::motion::Motion {
        duration: 2.0,
        fps: 30.0,
        looped: true,
        tracks: vec![],
    };
    use crate::motion::{Ease, Prop};
    clip.set_key(star_id, Prop::X, 0.0, 0.0, Ease::EaseInOut);
    clip.set_key(star_id, Prop::X, 1.0, 80.0, Ease::EaseInOut);
    clip.set_key(star_id, Prop::X, 2.0, 0.0, Ease::EaseInOut);
    clip.set_key(star_id, Prop::Rotation, 0.0, 0.0, Ease::EaseInOut);
    clip.set_key(
        star_id,
        Prop::Rotation,
        2.0,
        std::f32::consts::FRAC_PI_2,
        Ease::EaseInOut,
    );
    clip.set_key(star_id, Prop::Scale, 0.0, 1.0, Ease::EaseInOut);
    clip.set_key(star_id, Prop::Scale, 1.0, 1.18, Ease::EaseInOut);
    clip.set_key(star_id, Prop::Scale, 2.0, 1.0, Ease::EaseInOut);
    clip.set_key(disc_id, Prop::Y, 0.0, 0.0, Ease::EaseOut);
    clip.set_key(disc_id, Prop::Y, 1.0, -70.0, Ease::EaseOut);
    clip.set_key(disc_id, Prop::Y, 2.0, 0.0, Ease::EaseIn);
    clip.set_key(disc_id, Prop::Opacity, 0.0, 1.0, Ease::Linear);
    clip.set_key(disc_id, Prop::Opacity, 1.0, 0.45, Ease::Linear);
    clip.set_key(disc_id, Prop::Opacity, 2.0, 1.0, Ease::Linear);
    s.commit(Cmd::SetMotion {
        before: crate::motion::Motion::default(),
        after: clip,
    });
    s.selection = vec![(1, star_id)];
    s.playhead = 0.72;
    s.playing = false;
    s.show_rulers = false;
    s.need_fit = true;
    s.status = "Space plays · K keys · File → Export Lottie".into();
}

fn add_rect(s: &mut Studio, x: f32, y: f32, w: f32, h: f32, c: Rgba, radius: f32) {
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(w, h),
                radius,
            },
            Style {
                fill: Fill::Solid(c),
                stroke: None,
            },
        ),
    });
}

fn add_text(s: &mut Studio, origin: Pt, content: &str, px: f32, color: Rgba, tracking: f32) {
    let run = TypeRun {
        origin,
        content: content.into(),
        px,
        tracking,
        font: s.text_font.clone(),
        kern: true,
        liga: true,
        ..TypeRun::default()
    };
    let mut g = Geom::Text(run);
    crate::text::fill_contours(&mut g);
    s.commit(Cmd::AddShape {
        layer: 1,
        shape: Shape::new(
            g,
            Style {
                fill: Fill::Solid(color),
                stroke: None,
            },
        ),
    });
}
