//! Layout persona starters: mobile screen, landing hero, dashboard.

use crate::color::Rgba;
use crate::document::{Artboard, Document, Fill, Layer, Shape, Style};
use crate::geom::{Geom, Pt, TypeRun};
use crate::layout::{
    AutoStack, Constraint, StackAlign, StackAxis, make_frame, make_placeholder, reflow,
};

#[derive(Clone, Copy)]
pub struct LayoutTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub const CATALOG: &[LayoutTemplate] = &[
    LayoutTemplate {
        id: "layout-mobile",
        name: "Mobile screen",
        description: "A phone frame with a stacked header, hero image, and action row.",
    },
    LayoutTemplate {
        id: "layout-hero",
        name: "Landing hero",
        description: "A wide hero with a stretching copy column and a pinned call to action.",
    },
    LayoutTemplate {
        id: "layout-dashboard",
        name: "Dashboard",
        description: "Sidebar navigation with a stretching content pane and metric cards.",
    },
    LayoutTemplate {
        id: "layout-card",
        name: "Card stack",
        description: "A vertical auto-layout of product cards inside a marketing frame.",
    },
];

pub fn find(id: &str) -> Option<&'static LayoutTemplate> {
    CATALOG.iter().find(|t| t.id == id)
}

pub fn is_layout_template(id: &str) -> bool {
    find(id).is_some()
}

pub fn build(id: &str, width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    match id {
        "layout-mobile" => mobile(width, height, dpi),
        "layout-hero" => hero(width, height, dpi),
        "layout-dashboard" => dashboard(width, height, dpi),
        "layout-card" => cards(width, height, dpi),
        _ => Err(format!("Unknown layout template: {id}")),
    }
}

fn blank(name: &str, width: f32, height: f32, dpi: f32) -> Document {
    let mut doc = Document::new(name, 1.0, 1.0, dpi);
    doc.width = width;
    doc.height = height;
    doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(width, height))];
    doc.artboards[0].name = name.into();
    doc.layers = vec![Layer::vector("Layout")];
    doc
}

fn push(doc: &mut Document, shape: Shape) -> u64 {
    let id = shape.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    id
}

fn label(origin: Pt, text: &str, px: f32, color: Rgba, parent: Option<u64>) -> Shape {
    let run = TypeRun {
        origin: Pt::new(origin.x, origin.y + px),
        content: text.into(),
        px,
        ..Default::default()
    };
    let mut geom = Geom::Text(run);
    crate::text::fill_contours(&mut geom);
    let mut shape = Shape::new(
        geom,
        Style {
            fill: Fill::Solid(color),
            stroke: None,
        },
    );
    shape.name = "Label".into();
    shape.layout.parent = parent;
    shape
}

fn fill_rect(origin: Pt, size: Pt, color: Rgba, parent: Option<u64>, name: &str) -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin,
            size,
            radius: 10.0,
        },
        Style {
            fill: Fill::Solid(color),
            stroke: None,
        },
    );
    shape.name = name.into();
    shape.layout.parent = parent;
    shape.corners = [10.0; 4];
    shape
}

fn mobile(width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    let mut doc = blank("Mobile screen", width.max(320.0), height.max(640.0), dpi);
    let w = doc.width;
    let h = doc.height;
    let ink = Rgba::rgb(0x17, 0x1C, 0x24);
    let paper = Rgba::rgb(0xF6, 0xF1, 0xE8);
    let accent = Rgba::rgb(0xE8, 0x6A, 0x3C);
    let mute = Rgba::rgb(0x5C, 0x65, 0x73);
    let screen = {
        let mut frame = make_frame(Pt::new(w * 0.18, h * 0.06), Pt::new(w * 0.64, h * 0.88));
        frame.name = "Phone".into();
        frame.style.fill = Fill::Solid(paper);
        frame.layout.stack = Some(AutoStack {
            direction: StackAxis::Vertical,
            gap: 16.0,
            padding: [28.0, 20.0, 24.0, 20.0],
            align: StackAlign::Stretch,
        });
        frame
    };
    let phone = push(&mut doc, screen);
    push(
        &mut doc,
        label(Pt::ZERO, "OMADESIGN TRANSIT", 14.0, mute, Some(phone)),
    );
    push(
        &mut doc,
        label(Pt::ZERO, "Night ferry", 36.0, ink, Some(phone)),
    );
    let hero = {
        let mut p = make_placeholder(Pt::ZERO, Pt::new(200.0, 160.0), Some(phone));
        p.name = "Hero photo".into();
        p
    };
    push(&mut doc, hero);
    push(
        &mut doc,
        label(Pt::ZERO, "Boarding 21:40 · Gate C", 16.0, mute, Some(phone)),
    );
    let mut cta = fill_rect(
        Pt::ZERO,
        Pt::new(200.0, 44.0),
        accent,
        Some(phone),
        "Button",
    );
    cta.layout.constraint_x = Constraint::Stretch;
    push(&mut doc, cta);
    reflow(&mut doc, 0, phone);
    Ok(doc)
}

fn hero(width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    let mut doc = blank("Landing hero", width.max(960.0), height.max(540.0), dpi);
    let w = doc.width;
    let h = doc.height;
    let ink = Rgba::rgb(0x12, 0x18, 0x22);
    let paper = Rgba::rgb(0xF4, 0xEE, 0xE4);
    let accent = Rgba::rgb(0x2F, 0x6B, 0xFF);
    let mut page = make_frame(Pt::ZERO, Pt::new(w, h));
    page.name = "Page".into();
    page.style.fill = Fill::Solid(paper);
    page.layout.stack = Some(AutoStack {
        direction: StackAxis::Horizontal,
        gap: 32.0,
        padding: [64.0, 64.0, 64.0, 64.0],
        align: StackAlign::Stretch,
    });
    let page_id = push(&mut doc, page);
    let mut copy = make_frame(Pt::ZERO, Pt::new(w * 0.42, h * 0.7));
    copy.name = "Copy".into();
    copy.layout.parent = Some(page_id);
    copy.layout.constraint_x = Constraint::Stretch;
    copy.layout.stack = Some(AutoStack {
        direction: StackAxis::Vertical,
        gap: 18.0,
        padding: [8.0, 8.0, 8.0, 8.0],
        align: StackAlign::Start,
    });
    let copy_id = push(&mut doc, copy);
    push(
        &mut doc,
        label(Pt::ZERO, "A native Linux studio", 18.0, ink, Some(copy_id)),
    );
    push(
        &mut doc,
        label(Pt::ZERO, "Layout, then ship it.", 48.0, ink, Some(copy_id)),
    );
    push(
        &mut doc,
        label(
            Pt::ZERO,
            "Frames, stacks and constraints for UI mockups that stay editable.",
            18.0,
            ink,
            Some(copy_id),
        ),
    );
    let mut cta = fill_rect(Pt::ZERO, Pt::new(160.0, 44.0), accent, Some(copy_id), "CTA");
    cta.layout.constraint_x = Constraint::Start;
    push(&mut doc, cta);
    let mut media = make_placeholder(Pt::ZERO, Pt::new(w * 0.4, h * 0.6), Some(page_id));
    media.layout.constraint_x = Constraint::End;
    media.layout.constraint_y = Constraint::Stretch;
    push(&mut doc, media);
    reflow(&mut doc, 0, copy_id);
    reflow(&mut doc, 0, page_id);
    Ok(doc)
}

fn dashboard(width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    let mut doc = blank("Dashboard", width.max(1100.0), height.max(700.0), dpi);
    let w = doc.width;
    let h = doc.height;
    let ink = Rgba::rgb(0xE8, 0xEC, 0xF4);
    let panel = Rgba::rgb(0x1B, 0x22, 0x30);
    let card = Rgba::rgb(0x27, 0x31, 0x43);
    let accent = Rgba::rgb(0x7C, 0x9C, 0xFF);
    let mut page = make_frame(Pt::ZERO, Pt::new(w, h));
    page.name = "App".into();
    page.style.fill = Fill::Solid(panel);
    page.layout.stack = Some(AutoStack {
        direction: StackAxis::Horizontal,
        gap: 0.0,
        padding: [0.0; 4],
        align: StackAlign::Stretch,
    });
    let page_id = push(&mut doc, page);
    let mut side = make_frame(Pt::ZERO, Pt::new(220.0, h));
    side.name = "Sidebar".into();
    side.style.fill = Fill::Solid(Rgba::rgb(0x14, 0x19, 0x24));
    side.layout.parent = Some(page_id);
    side.layout.constraint_x = Constraint::Start;
    side.layout.stack = Some(AutoStack {
        direction: StackAxis::Vertical,
        gap: 12.0,
        padding: [24.0, 18.0, 24.0, 18.0],
        align: StackAlign::Stretch,
    });
    let side_id = push(&mut doc, side);
    push(
        &mut doc,
        label(Pt::ZERO, "omadesign", 20.0, ink, Some(side_id)),
    );
    for item in ["Overview", "Projects", "Showcase", "Settings"] {
        push(&mut doc, label(Pt::ZERO, item, 16.0, ink, Some(side_id)));
    }
    let mut main = make_frame(Pt::ZERO, Pt::new(w - 220.0, h));
    main.name = "Content".into();
    main.layout.parent = Some(page_id);
    main.layout.constraint_x = Constraint::Stretch;
    main.layout.stack = Some(AutoStack {
        direction: StackAxis::Vertical,
        gap: 20.0,
        padding: [32.0, 32.0, 32.0, 32.0],
        align: StackAlign::Stretch,
    });
    let main_id = push(&mut doc, main);
    push(
        &mut doc,
        label(Pt::ZERO, "This week", 28.0, ink, Some(main_id)),
    );
    let mut row = make_frame(Pt::ZERO, Pt::new(w - 284.0, 120.0));
    row.name = "Metrics".into();
    row.layout.parent = Some(main_id);
    row.layout.stack = Some(AutoStack {
        direction: StackAxis::Horizontal,
        gap: 16.0,
        padding: [0.0; 4],
        align: StackAlign::Stretch,
    });
    let row_id = push(&mut doc, row);
    for (title, value) in [("Frames", "24"), ("Comments", "7"), ("Published", "3")] {
        let mut metric = make_frame(Pt::ZERO, Pt::new(180.0, 100.0));
        metric.name = title.into();
        metric.style.fill = Fill::Solid(card);
        metric.layout.parent = Some(row_id);
        metric.layout.constraint_x = Constraint::Stretch;
        metric.layout.stack = Some(AutoStack {
            direction: StackAxis::Vertical,
            gap: 6.0,
            padding: [16.0, 16.0, 16.0, 16.0],
            align: StackAlign::Start,
        });
        let metric_id = push(&mut doc, metric);
        push(&mut doc, label(Pt::ZERO, title, 13.0, ink, Some(metric_id)));
        push(
            &mut doc,
            label(Pt::ZERO, value, 28.0, accent, Some(metric_id)),
        );
        reflow(&mut doc, 0, metric_id);
    }
    reflow(&mut doc, 0, side_id);
    reflow(&mut doc, 0, row_id);
    reflow(&mut doc, 0, main_id);
    reflow(&mut doc, 0, page_id);
    Ok(doc)
}

fn cards(width: f32, height: f32, dpi: f32) -> Result<Document, String> {
    let mut doc = blank("Card stack", width.max(420.0), height.max(720.0), dpi);
    let w = doc.width;
    let h = doc.height;
    let ink = Rgba::rgb(0x1A, 0x1F, 0x28);
    let paper = Rgba::rgb(0xFB, 0xF7, 0xF0);
    let card = Rgba::rgb(0xFF, 0xFF, 0xFF);
    let mut page = make_frame(Pt::new(w * 0.12, h * 0.06), Pt::new(w * 0.76, h * 0.88));
    page.name = "Feed".into();
    page.style.fill = Fill::Solid(paper);
    page.layout.stack = Some(AutoStack {
        direction: StackAxis::Vertical,
        gap: 16.0,
        padding: [24.0, 24.0, 24.0, 24.0],
        align: StackAlign::Stretch,
    });
    let page_id = push(&mut doc, page);
    push(
        &mut doc,
        label(Pt::ZERO, "New this week", 22.0, ink, Some(page_id)),
    );
    for title in ["Harbor mark", "Night ferry", "Studio dashboard"] {
        let mut card_shape = make_frame(Pt::ZERO, Pt::new(280.0, 88.0));
        card_shape.name = title.into();
        card_shape.style.fill = Fill::Solid(card);
        card_shape.layout.parent = Some(page_id);
        card_shape.layout.constraint_x = Constraint::Stretch;
        card_shape.layout.stack = Some(AutoStack {
            direction: StackAxis::Vertical,
            gap: 4.0,
            padding: [18.0, 18.0, 18.0, 18.0],
            align: StackAlign::Start,
        });
        let id = push(&mut doc, card_shape);
        push(&mut doc, label(Pt::ZERO, title, 18.0, ink, Some(id)));
        reflow(&mut doc, 0, id);
    }
    reflow(&mut doc, 0, page_id);
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_template_builds_nested_frames() {
        for template in CATALOG {
            let doc = build(template.id, 1280.0, 800.0, 72.0).unwrap();
            let frames: Vec<_> = doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .filter(|s| s.layout.frame)
                .collect();
            assert!(!frames.is_empty(), "{} must create a frame", template.id);
            let nested = doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .any(|s| s.layout.parent.is_some());
            assert!(nested, "{} must nest children", template.id);
            assert!(
                frames.iter().any(|s| s.layout.stack.is_some()),
                "{} must ship an auto-layout stack",
                template.id
            );
        }
    }
}
