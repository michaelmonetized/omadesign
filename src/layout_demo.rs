//! An editable, responsive two-screen design with linked component variants.
//! Every visible mark is a native vector or live text object.
use crate::color::Rgba;
use crate::document::{Artboard, Document, Fill, Layer, Shape, Style};
use crate::geom::{Geom, Pt, TypeRun};
use crate::layout::{
    AutoStack, Constraint, FrameLayout, Sizing, StackAlign, StackAxis, StackFlow, StackJustify,
};
use crate::layout_prototype::{Interaction, PrototypeAction, Transition, Trigger};

const INK: Rgba = Rgba {
    r: 31,
    g: 40,
    b: 43,
    a: 255,
};
const PAPER: Rgba = Rgba {
    r: 247,
    g: 244,
    b: 235,
    a: 255,
};
const ORANGE: Rgba = Rgba {
    r: 223,
    g: 87,
    b: 43,
    a: 255,
};
const MUTE: Rgba = Rgba {
    r: 100,
    g: 106,
    b: 101,
    a: 255,
};
const LINE: Rgba = Rgba {
    r: 216,
    g: 215,
    b: 203,
    a: 255,
};

fn push(doc: &mut Document, shape: Shape) -> u64 {
    let id = shape.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    id
}
fn solid(color: Rgba) -> Style {
    Style {
        fill: Fill::Solid(color),
        stroke: None,
    }
}
fn frame(
    name: &str,
    origin: Pt,
    size: Pt,
    parent: Option<u64>,
    color: Option<Rgba>,
    stack: Option<AutoStack>,
) -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin,
            size,
            radius: 0.,
        },
        color.map(solid).unwrap_or(Style {
            fill: Fill::None,
            stroke: None,
        }),
    );
    shape.name = name.into();
    shape.layout = FrameLayout::frame();
    shape.layout.parent = parent;
    shape.layout.stack = stack;
    shape
}
fn vertical(gap: f32, pad: f32) -> AutoStack {
    AutoStack {
        gap,
        padding: [pad; 4],
        ..Default::default()
    }
}
fn horizontal(gap: f32) -> AutoStack {
    AutoStack {
        direction: StackAxis::Horizontal,
        gap,
        padding: [0.; 4],
        align: StackAlign::Center,
        ..Default::default()
    }
}
fn flow(doc: &mut Document, id: u64, width: Sizing, height: Sizing) {
    let s = doc.find_shape_mut(0, id).unwrap();
    s.layout.width = width;
    s.layout.height = height;
}
fn font(serif: bool) -> String {
    let paths = if serif {
        [
            "/usr/share/fonts/gsfonts/NimbusRoman-Regular.otf",
            "/usr/share/fonts/noto/NotoSerif-Regular.ttf",
        ]
    } else {
        [
            "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ]
    };
    paths
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
        .map(String::from)
        .unwrap_or_default()
}
fn text(
    doc: &mut Document,
    parent: u64,
    name: &str,
    content: &str,
    px: f32,
    color: Rgba,
    serif: bool,
    width: Option<f32>,
) -> u64 {
    let mut run = TypeRun {
        origin: Pt::new(0., px),
        content: content.into(),
        px,
        leading: px * 1.18,
        font: font(serif),
        wrap_width: width,
        ..Default::default()
    };
    run.contours = crate::text::shape(&run);
    let mut shape = Shape::new(Geom::Text(run), solid(color));
    shape.name = name.into();
    shape.layout.parent = Some(parent);
    if width.is_some() {
        shape.layout.width = Sizing::Fill;
        shape.layout.height = Sizing::Hug;
    }
    push(doc, shape)
}
fn position(doc: &mut Document, id: u64, x: f32, y: f32) {
    let s = doc.find_shape_mut(0, id).unwrap();
    let old = s.geom.bbox().min;
    s.geom.translate(Pt::new(x, y) - old);
}
fn rect(
    doc: &mut Document,
    parent: u64,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Rgba,
    radius: f32,
) -> u64 {
    let mut shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(w, h),
            radius,
        },
        solid(color),
    );
    shape.name = name.into();
    shape.layout.parent = Some(parent);
    shape.layout.constraint_x = Constraint::Scale;
    shape.layout.constraint_y = Constraint::Scale;
    push(doc, shape)
}
fn ellipse(
    doc: &mut Document,
    parent: u64,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Rgba,
) -> u64 {
    let mut shape = Shape::new(
        Geom::Ellipse {
            center: Pt::new(x + w * 0.5, y + h * 0.5),
            radii: Pt::new(w * 0.5, h * 0.5),
        },
        solid(color),
    );
    shape.name = name.into();
    shape.layout.parent = Some(parent);
    shape.layout.constraint_x = Constraint::Scale;
    shape.layout.constraint_y = Constraint::Scale;
    push(doc, shape)
}
fn link(doc: &mut Document, id: u64, trigger: Trigger, action: PrototypeAction) {
    doc.find_shape_mut(0, id)
        .unwrap()
        .layout
        .interactions
        .push(Interaction {
            trigger,
            action,
            transition: Transition::Dissolve,
            duration_ms: 220,
        });
}

fn artwork(doc: &mut Document, parent: u64) -> u64 {
    let mut art = frame(
        "A room of your own / vector illustration",
        Pt::ZERO,
        Pt::new(536., 520.),
        Some(parent),
        Some(Rgba::rgb(219, 202, 182)),
        None,
    );
    art.corners = [16.; 4];
    art.layout.width = Sizing::Fill;
    art.layout.min_width = Some(240.);
    art.layout.aspect_ratio = Some(536. / 520.);
    let id = push(doc, art);
    // A quiet architectural composition, built from native geometry.
    rect(
        doc,
        id,
        "Floor",
        0.,
        354.,
        536.,
        166.,
        Rgba::rgb(201, 174, 146),
        0.,
    );
    ellipse(doc, id, "Arched opening", 158., 74., 228., 258., INK);
    rect(
        doc,
        id,
        "Window lower opening",
        158.,
        198.,
        228.,
        202.,
        INK,
        0.,
    );
    ellipse(
        doc,
        id,
        "Late afternoon sun",
        207.,
        149.,
        118.,
        118.,
        ORANGE,
    );
    rect(
        doc,
        id,
        "Window sill",
        143.,
        369.,
        258.,
        20.,
        Rgba::rgb(243, 231, 209),
        0.,
    );
    rect(
        doc,
        id,
        "Window mullion",
        267.,
        74.,
        7.,
        310.,
        Rgba::rgb(231, 215, 193),
        0.,
    );
    let mut shadow = Shape::new(
        Geom::Poly {
            contours: vec![vec![
                Pt::new(164., 390.),
                Pt::new(381., 390.),
                Pt::new(523., 520.),
                Pt::new(280., 520.),
            ]],
            winding: false,
        },
        solid(Rgba::rgb(226, 198, 166)),
    );
    shadow.name = "Light falling across the floor".into();
    shadow.layout.parent = Some(id);
    shadow.layout.constraint_x = Constraint::Scale;
    shadow.layout.constraint_y = Constraint::Scale;
    push(doc, shadow);
    ellipse(doc, id, "Sculpture plinth top", 69., 344., 87., 24., PAPER);
    rect(doc, id, "Sculpture plinth", 69., 356., 87., 92., PAPER, 0.);
    ellipse(doc, id, "Sculpture plinth base", 69., 435., 87., 24., PAPER);
    ellipse(
        doc,
        id,
        "Sculpture lower sphere",
        89.,
        302.,
        49.,
        48.,
        ORANGE,
    );
    ellipse(
        doc,
        id,
        "Sculpture upper sphere",
        99.,
        268.,
        29.,
        35.,
        ORANGE,
    );
    let label = text(
        doc,
        id,
        "Artwork edition",
        "FIELD NOTES     /     002",
        11.,
        INK,
        false,
        None,
    );
    position(doc, label, 28., 28.);
    let mut caption = frame(
        "Artwork caption",
        Pt::new(307., 441.),
        Pt::new(201., 50.),
        Some(id),
        Some(PAPER),
        Some(AutoStack {
            padding: [16., 16., 16., 16.],
            gap: 0.,
            align: StackAlign::Center,
            justify: StackJustify::Center,
            ..horizontal(0.)
        }),
    );
    caption.corners = [25.; 4];
    caption.layout.constraint_x = Constraint::End;
    caption.layout.constraint_y = Constraint::End;
    let cap = push(doc, caption);
    text(
        doc,
        cap,
        "Caption",
        "ROOM FOR POSSIBILITY",
        10.,
        INK,
        false,
        None,
    );
    crate::layout::reflow(doc, 0, cap);
    id
}

pub fn build() -> Document {
    let mut doc = Document::new("Fieldwork · responsive studio", 1280., 1080., 72.);
    doc.layers = vec![Layer::vector("Fieldwork · design system")];
    doc.transparent = true;
    doc.artboards = vec![
        Artboard::new(0, Pt::ZERO, Pt::new(1280., 1080.)),
        Artboard::new(1, Pt::new(1400., 0.), Pt::new(1280., 1080.)),
    ];
    doc.artboards[0].name = "01 / Home".into();
    doc.artboards[1].name = "02 / Our approach".into();
    let home = push(
        &mut doc,
        frame(
            "01 / Home",
            Pt::ZERO,
            Pt::new(1280., 1080.),
            None,
            Some(PAPER),
            Some(vertical(32., 40.)),
        ),
    );
    flow(&mut doc, home, Sizing::Fixed, Sizing::Hug);
    doc.find_shape_mut(0, home)
        .unwrap()
        .layout
        .breakpoints
        .push(crate::layout::LayoutBreakpoint {
            max_width: 600.,
            padding: Some([24.; 4]),
            gap: Some(28.),
            ..Default::default()
        });
    let mut nav = frame(
        "Navigation",
        Pt::ZERO,
        Pt::new(1200., 44.),
        Some(home),
        None,
        Some(AutoStack {
            justify: StackJustify::SpaceBetween,
            flow: StackFlow::Wrap,
            cross_gap: 12.,
            ..horizontal(24.)
        }),
    );
    nav.layout.width = Sizing::Fill;
    nav.layout.height = Sizing::Hug;
    let nav = push(&mut doc, nav);
    text(
        &mut doc,
        nav,
        "Wordmark",
        "fieldwork.",
        30.,
        INK,
        true,
        None,
    );
    let contact = text(
        &mut doc,
        nav,
        "Contact navigation",
        "LET’S TALK  ↗",
        12.,
        INK,
        false,
        None,
    );
    let mut hero = frame(
        "Hero / responsive wrap",
        Pt::ZERO,
        Pt::new(1200., 520.),
        Some(home),
        None,
        Some(AutoStack {
            flow: StackFlow::Wrap,
            cross_gap: 40.,
            align: StackAlign::Start,
            ..horizontal(56.)
        }),
    );
    hero.layout.width = Sizing::Fill;
    hero.layout.height = Sizing::Hug;
    let hero = push(&mut doc, hero);
    let mut copy = frame(
        "Editorial copy",
        Pt::ZERO,
        Pt::new(550., 520.),
        Some(hero),
        None,
        Some(vertical(24., 0.)),
    );
    copy.layout.width = Sizing::Fill;
    copy.layout.height = Sizing::Hug;
    copy.layout.min_width = Some(260.);
    let copy = push(&mut doc, copy);
    let eyebrow = text(
        &mut doc,
        copy,
        "Eyebrow",
        "A NEW WAY TO FEEL AT HOME",
        11.,
        ORANGE,
        false,
        None,
    );
    if let Geom::Text(run) = &mut doc.find_shape_mut(0, eyebrow).unwrap().geom {
        run.tracking = 1.8;
        run.contours = crate::text::shape(run);
    }
    let title = text(
        &mut doc,
        copy,
        "Headline",
        "Make room\nfor possibility.",
        72.,
        INK,
        true,
        Some(550.),
    );
    if let Geom::Text(run) = &mut doc.find_shape_mut(0, title).unwrap().geom {
        run.leading = 76.;
        run.tracking = -1.2;
        run.contours = crate::text::shape(run);
    }
    {
        let s = doc.find_shape_mut(0, title).unwrap();
        s.layout.text_size = Some(72.);
        s.layout.breakpoints.push(crate::layout::LayoutBreakpoint {
            max_width: 600.,
            text_size: Some(44.),
            ..Default::default()
        });
    }
    text(
        &mut doc,
        copy,
        "Introduction",
        "Thoughtful places for curious people. We bring an independent spirit to the spaces where your next chapter begins.",
        17.,
        MUTE,
        false,
        Some(480.),
    );
    let actions = push(
        &mut doc,
        frame(
            "Primary action",
            Pt::ZERO,
            Pt::new(500., 56.),
            Some(copy),
            None,
            Some(horizontal(20.)),
        ),
    );
    flow(&mut doc, actions, Sizing::Fill, Sizing::Fixed);
    text(
        &mut doc,
        copy,
        "Availability",
        "A small studio. A few good projects.\nNow welcoming collaborations for 2026.",
        12.,
        MUTE,
        false,
        Some(450.),
    );
    artwork(&mut doc, hero);
    let mut principles = frame(
        "Principles / responsive cards",
        Pt::ZERO,
        Pt::new(1200., 150.),
        Some(home),
        None,
        Some(AutoStack {
            flow: StackFlow::Wrap,
            cross_gap: 24.,
            align: StackAlign::Start,
            ..horizontal(32.)
        }),
    );
    principles.layout.width = Sizing::Fill;
    principles.layout.height = Sizing::Hug;
    let principles = push(&mut doc, principles);
    for (number, title, description) in [
        (
            "01 / OBSERVE",
            "Start with the everyday.",
            "The morning light. A favorite corner. The details that make a place yours.",
        ),
        (
            "02 / EXPLORE",
            "Leave room for surprise.",
            "Good ideas rarely travel in straight lines. We follow the possibilities.",
        ),
        (
            "03 / MAKE",
            "Build something that lasts.",
            "Honest materials, useful spaces, and a point of view worth keeping.",
        ),
    ] {
        let mut card = frame(
            title,
            Pt::ZERO,
            Pt::new(378., 146.),
            Some(principles),
            None,
            Some(vertical(14., 0.)),
        );
        card.layout.width = Sizing::Fill;
        card.layout.height = Sizing::Hug;
        card.layout.min_width = Some(230.);
        let card = push(&mut doc, card);
        text(
            &mut doc,
            card,
            "Principle number",
            number,
            10.,
            ORANGE,
            false,
            None,
        );
        text(
            &mut doc,
            card,
            "Principle title",
            title,
            25.,
            INK,
            true,
            Some(370.),
        );
        text(
            &mut doc,
            card,
            "Principle description",
            description,
            13.,
            MUTE,
            false,
            Some(365.),
        );
    }
    let rule = rect(&mut doc, home, "Quiet divider", 0., 0., 1200., 1., LINE, 0.);
    flow(&mut doc, rule, Sizing::Fill, Sizing::Fixed);
    let footer = push(
        &mut doc,
        frame(
            "Footer",
            Pt::ZERO,
            Pt::new(1200., 25.),
            Some(home),
            None,
            Some(AutoStack {
                justify: StackJustify::SpaceBetween,
                ..horizontal(24.)
            }),
        ),
    );
    flow(&mut doc, footer, Sizing::Fill, Sizing::Hug);
    doc.find_shape_mut(0, footer)
        .unwrap()
        .layout
        .breakpoints
        .push(crate::layout::LayoutBreakpoint {
            max_width: 600.,
            direction: Some(StackAxis::Vertical),
            align: Some(StackAlign::Start),
            justify: Some(StackJustify::Start),
            gap: Some(12.),
            ..Default::default()
        });
    text(
        &mut doc,
        footer,
        "Copyright",
        "© FIELDWORK STUDIO",
        10.,
        MUTE,
        false,
        None,
    );
    text(
        &mut doc,
        footer,
        "Location",
        "BROOKLYN, NY     /     EVERYWHERE ELSE",
        10.,
        MUTE,
        false,
        None,
    );

    let detail = push(
        &mut doc,
        frame(
            "02 / Our approach",
            Pt::new(1400., 0.),
            Pt::new(1280., 1080.),
            None,
            Some(INK),
            Some(vertical(40., 64.)),
        ),
    );
    flow(&mut doc, detail, Sizing::Fixed, Sizing::Hug);
    {
        let s = doc.find_shape_mut(0, detail).unwrap();
        s.layout.min_height = Some(800.);
        s.layout.breakpoints.push(crate::layout::LayoutBreakpoint {
            max_width: 600.,
            padding: Some([24.; 4]),
            gap: Some(28.),
            ..Default::default()
        });
    }
    let mut detail_nav = frame(
        "Detail navigation",
        Pt::new(1400., 0.),
        Pt::new(1152., 44.),
        Some(detail),
        None,
        Some(AutoStack {
            justify: StackJustify::SpaceBetween,
            flow: StackFlow::Wrap,
            cross_gap: 12.,
            ..horizontal(24.)
        }),
    );
    detail_nav.layout.width = Sizing::Fill;
    detail_nav.layout.height = Sizing::Hug;
    let detail_nav = push(&mut doc, detail_nav);
    let back = text(
        &mut doc,
        detail_nav,
        "Back to home",
        "←  BACK TO FIELDWORK",
        12.,
        PAPER,
        false,
        None,
    );
    link(&mut doc, back, Trigger::Click, PrototypeAction::Back);
    text(
        &mut doc,
        detail_nav,
        "Chapter",
        "OUR APPROACH   /   01",
        11.,
        Rgba::rgb(178, 185, 174),
        false,
        None,
    );
    text(
        &mut doc,
        detail,
        "Detail eyebrow",
        "GOOD WORK BEGINS WITH A CONVERSATION",
        12.,
        Rgba::rgb(245, 149, 108),
        false,
        None,
    );
    let detail_title = text(
        &mut doc,
        detail,
        "Detail headline",
        "The best spaces\nstart with you.",
        86.,
        PAPER,
        true,
        Some(1080.),
    );
    let detail_description = text(
        &mut doc,
        detail,
        "Detail description",
        "Tell us what you’re imagining. We’ll bring an open mind, a fresh perspective, and the care to make it real.",
        25.,
        Rgba::rgb(186, 195, 188),
        false,
        Some(780.),
    );
    for (id, base, phone) in [(detail_title, 86., 44.), (detail_description, 25., 18.)] {
        let s = doc.find_shape_mut(0, id).unwrap();
        s.layout.text_size = Some(base);
        s.layout.breakpoints.push(crate::layout::LayoutBreakpoint {
            max_width: 600.,
            text_size: Some(phone),
            ..Default::default()
        });
    }
    let detail_actions = push(
        &mut doc,
        frame(
            "Detail actions",
            Pt::new(1400., 0.),
            Pt::new(500., 56.),
            Some(detail),
            None,
            Some(horizontal(20.)),
        ),
    );
    flow(&mut doc, detail_actions, Sizing::Fill, Sizing::Fixed);
    let note = text(
        &mut doc,
        detail,
        "Prototype note",
        "This is a working, editable prototype.\nUse the button or the back link to return to the first screen.",
        13.,
        Rgba::rgb(154, 166, 155),
        false,
        Some(700.),
    );
    let _ = note;

    // Components live beside the screens, with stable instances in the actual design.
    let mut button = frame(
        "Button / Primary",
        Pt::new(2800., 80.),
        Pt::new(222., 56.),
        None,
        Some(ORANGE),
        Some(AutoStack {
            padding: [18., 24., 18., 24.],
            align: StackAlign::Center,
            justify: StackJustify::Center,
            ..horizontal(12.)
        }),
    );
    button.corners = [28.; 4];
    let main = push(&mut doc, button);
    text(
        &mut doc,
        main,
        "Button label",
        "EXPLORE OUR APPROACH  ↗",
        11.,
        PAPER,
        false,
        None,
    );
    crate::layout::reflow(&mut doc, 0, main);
    crate::layout_components::make_component(&mut doc, 0, main).expect("demo button component");
    let hover =
        crate::layout_components::add_variant(&mut doc, main, "Hover", Pt::new(2800., 170.))
            .expect("demo hover variant");
    doc.find_shape_mut(0, hover).unwrap().style.fill = Fill::Solid(INK);
    link(
        &mut doc,
        main,
        Trigger::Hover,
        PrototypeAction::SetVariant { component: hover },
    );
    link(
        &mut doc,
        main,
        Trigger::Click,
        PrototypeAction::Navigate { target: detail },
    );
    link(
        &mut doc,
        hover,
        Trigger::Click,
        PrototypeAction::Navigate { target: detail },
    );
    let instance =
        crate::layout_components::insert_instance(&mut doc, main, 0, Pt::ZERO, Some(actions))
            .expect("demo button instance");
    doc.find_shape_mut(0, instance).unwrap().name = "Explore our approach / instance".into();
    let return_button = crate::layout_components::insert_instance(
        &mut doc,
        main,
        0,
        Pt::new(1400., 0.),
        Some(detail_actions),
    )
    .expect("demo return instance");
    let r = doc.find_shape_mut(0, return_button).unwrap();
    r.layout.interactions.clear();
    r.layout.interactions.push(Interaction {
        trigger: Trigger::Click,
        action: PrototypeAction::Navigate { target: home },
        transition: Transition::SlideRight,
        duration_ms: 250,
    });
    for id in crate::layout::descendants(&doc, 0, return_button) {
        if let Geom::Text(run) = &mut doc.find_shape_mut(0, id).unwrap().geom {
            run.content = "BACK TO THE BEGINNING  ↗".into();
            run.contours = crate::text::shape(run);
        }
    }
    link(
        &mut doc,
        contact,
        Trigger::Click,
        PrototypeAction::Navigate { target: detail },
    );
    crate::layout::reflow(&mut doc, 0, home);
    crate::layout::reflow(&mut doc, 0, detail);
    if let Some(root) = doc.find_shape(0, home) {
        doc.height = root.geom.bbox().height().max(1.);
    }
    doc.artboards[0].size = Pt::new(1280., doc.height);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn studio_demo_is_editable_valid_and_links_two_screens() {
        let doc = build();
        doc.validate_hierarchy().unwrap();
        let shapes = doc.layers[0].kind.shapes().unwrap();
        assert_eq!(doc.artboards.len(), 2);
        let decoded = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(decoded.layers[0].kind.shapes().unwrap().len(), shapes.len());
        assert!(shapes.iter().any(|s| matches!(
            s.layout.component,
            Some(crate::layout_components::ComponentBinding::Instance { .. })
        )));
        let root = shapes[0].id;
        let png = crate::compositor::export_frame_png(&doc, 0, root, 1).unwrap();
        assert!(png.len() > 10_000);
        let html = crate::layout_export::export_html(&doc, 0, root).unwrap();
        assert!(html.contains("Our approach") && html.contains("screen-"));
    }
}
