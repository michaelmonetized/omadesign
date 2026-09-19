//! Render the editable website Layout example using Omadesign's native engines.
extern crate omadesign;
use omadesign::{
    color::Rgba,
    document::{Document, Fill, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
    layout, text,
};
fn color(hex: u32) -> Rgba {
    Rgba::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
fn frame(
    doc: &mut Document,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hex: u32,
    parent: Option<u64>,
) -> u64 {
    let mut s = layout::make_frame(Pt::new(x, y), Pt::new(w, h));
    s.name = name.into();
    s.style = Style {
        fill: Fill::Solid(color(hex)),
        stroke: None,
    };
    s.layout.parent = parent;
    let id = s.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(s);
    id
}
fn label(doc: &mut Document, value: &str, x: f32, y: f32, px: f32, hex: u32, parent: u64) {
    let mut g = Geom::Text(TypeRun {
        origin: Pt::new(x, y + px),
        content: value.into(),
        px,
        font: "/usr/share/fonts/gsfonts/NimbusSans-Regular.otf".into(),
        ..Default::default()
    });
    text::fill_contours(&mut g);
    let mut s = Shape::new(
        g,
        Style {
            fill: Fill::Solid(color(hex)),
            stroke: None,
        },
    );
    s.name = value.into();
    s.layout.parent = Some(parent);
    doc.layers[0].kind.shapes_mut().unwrap().push(s);
}
fn main() {
    let mut d = Document::new("Form — studio dashboard", 1600., 1000., 72.);
    d.layers = vec![Layer::vector("Layout")];
    let page = frame(&mut d, "Dashboard", 0., 0., 1600., 1000., 0xf3f0e9, None);
    let side = frame(
        &mut d,
        "Navigation",
        0.,
        0.,
        280.,
        1000.,
        0x242329,
        Some(page),
    );
    label(&mut d, "form.", 40., 42., 58., 0xe6edaa, side);
    for (i, v) in ["Overview", "Projects", "Assets", "Archive"]
        .iter()
        .enumerate()
    {
        label(
            &mut d,
            v,
            42.,
            220. + i as f32 * 62.,
            23.,
            if i == 0 { 0xe6edaa } else { 0xa9a7b2 },
            side,
        );
    }
    label(&mut d, "INDEPENDENT STUDIO", 40., 874., 14., 0xa9a7b2, side);
    label(
        &mut d,
        "Make room for ideas.",
        40.,
        910.,
        18.,
        0xf3f0e9,
        side,
    );
    label(
        &mut d,
        "WORKSPACE / OVERVIEW",
        340.,
        48.,
        16.,
        0x696574,
        page,
    );
    label(
        &mut d,
        "A little space to create.",
        340.,
        104.,
        54.,
        0x242329,
        page,
    );
    label(
        &mut d,
        "Your projects, taking shape.",
        340.,
        179.,
        23.,
        0x696574,
        page,
    );
    let metrics = frame(
        &mut d,
        "Metrics",
        340.,
        252.,
        1200.,
        148.,
        0xf3f0e9,
        Some(page),
    );
    for (i, (title, n)) in [
        ("ACTIVE PROJECTS", "08"),
        ("READY TO SHARE", "03"),
        ("IDEAS IN MOTION", "12"),
    ]
    .iter()
    .enumerate()
    {
        let x = 340. + i as f32 * 408.;
        let f = frame(
            &mut d,
            title,
            x,
            252.,
            384.,
            148.,
            if i == 0 { 0xe6edaa } else { 0xe5e1ed },
            Some(metrics),
        );
        label(&mut d, title, x + 26., 272., 14., 0x57515f, f);
        label(&mut d, n, x + 26., 303., 52., 0x242329, f);
    }
    label(
        &mut d,
        "On the drawing board",
        340.,
        448.,
        30.,
        0x242329,
        page,
    );
    label(&mut d, "VIEW ALL  →", 1400., 458., 14., 0x696574, page);
    let first = frame(
        &mut d,
        "Identity project",
        340.,
        514.,
        580.,
        340.,
        0xb9acd1,
        Some(page),
    );
    label(
        &mut d,
        "01 / BRAND IDENTITY",
        370.,
        543.,
        15.,
        0x393044,
        first,
    );
    label(&mut d, "Less noise.", 374., 615., 62., 0x393044, first);
    label(&mut d, "More meaning.", 374., 689., 62., 0x393044, first);
    let second = frame(
        &mut d,
        "Editorial project",
        944.,
        514.,
        596.,
        340.,
        0x30372d,
        Some(page),
    );
    label(
        &mut d,
        "02 / DIGITAL EDITORIAL",
        974.,
        543.,
        15.,
        0xdce4bf,
        second,
    );
    label(&mut d, "FIELD", 980., 605., 94., 0xe6edaa, second);
    label(&mut d, "NOTES", 980., 702., 94., 0xe6edaa, second);
    label(&mut d, "Common Ground", 340., 878., 23., 0x242329, page);
    label(
        &mut d,
        "Identity system · In progress",
        340.,
        916.,
        17.,
        0x696574,
        page,
    );
    label(&mut d, "Field Notes", 944., 878., 23., 0x242329, page);
    label(
        &mut d,
        "Editorial website · Ready for review",
        944.,
        916.,
        17.,
        0x696574,
        page,
    );
    std::fs::write(
        "/tmp/layout.png",
        omadesign::compositor::export_png(&d, 1).unwrap(),
    )
    .unwrap();
    omadesign::project::save_to(
        &d,
        std::path::Path::new("site/public/media/showcase/layout.oma"),
    )
    .unwrap();
}
