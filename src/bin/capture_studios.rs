//! Isolated native WGPU recordings driven by real egui pointer/key events.
//! Run from the repository root: capture_studios SCENE OUTPUT_DIR [--probe] [--fps 30|60].
//! Only the initial document is seeded; recorded edits go through the actual UI.
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, ViewportBuilder};
use omadesign::{
    app::Studio,
    color::Rgba,
    document::{Document, Layer, Pixels},
    geom::{Geom, Pt},
    tools::{Persona, Tool},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant, UNIX_EPOCH},
};

const FPS: u32 = 30;
const SIZE: [u32; 2] = [1600, 900];
#[derive(Clone, Debug)]
enum Target {
    Text(&'static str),
    Any(&'static str),
    Left(&'static str),
    Field(&'static str),
    Menu(&'static str),
    Slider(&'static str, f32),
    World(f32, f32),
    At(f32, f32),
    Swatch(&'static str),
    Asset(&'static str),
    Handle(usize),
    RoleApply(&'static str),
    Offset(&'static str, f32, f32),
}
#[derive(Debug)]
enum ActionKind {
    Click(Target),
    ModifiedClick(Target, Modifiers),
    Drag(Target, Target),
    Delta(Target, f32),
    Key(Key, Modifiers),
    Scroll(f32),
    Hover(Target),
    Move(Target),
    ScrollAt(Target, f32),
    Type(&'static str),
    Expect(&'static str),
}
struct Action {
    start: u32,
    end: u32,
    kind: ActionKind,
    points: Option<(Pos2, Pos2)>,
}
fn event(at: f32, kind: ActionKind) -> Action {
    Action {
        start: (at * FPS as f32).round() as u32,
        end: (at * FPS as f32).round() as u32,
        kind,
        points: None,
    }
}
fn drag(at: f32, seconds: f32, kind: ActionKind) -> Action {
    let mut a = event(at, kind);
    a.end = a.start + (seconds * FPS as f32) as u32;
    a
}
fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    }
}
fn schedule(scene: &str) -> Vec<Action> {
    use ActionKind::*;
    use Target::*;
    match scene {
        "welcome-browse" => vec![
            event(0.5, Expect("Your Work")),
            drag(0.8, 0.7, Move(At(105., 180.))),
            event(1.7, Expect("After Hours")),
            drag(2.2, 0.7, Move(Left("\u{E266}"))),
            event(3., Click(Left("\u{E266}"))),
            event(3.4, Expect("All modes")),
            drag(3.6, 0.5, Move(Left("Raster"))),
            event(4.2, Click(Left("Raster"))),
            drag(4.8, 0.6, Move(At(105., 150.))),
            event(5.6, Expect("Iris study")),
            drag(6.1, 0.5, Move(Left("\u{E266}"))),
            event(6.7, Click(Left("\u{E266}"))),
            event(7.1, Click(Left("All modes"))),
            drag(7.6, 0.6, Move(At(105., 180.))),
            event(
                8.3,
                ModifiedClick(
                    At(105., 180.),
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                ),
            ),
            drag(8.8, 0.6, Move(At(300., 150.))),
            event(9.5, Click(At(300., 150.))),
            event(10., Expect("2 selected")),
            drag(10.5, 0.5, Move(Left("Clear"))),
            event(11.1, Click(Left("Clear"))),
            drag(11.6, 0.8, Move(At(1080., 130.))),
            event(12.5, Expect("Fieldwork")),
            event(13., Click(At(1080., 130.))),
            event(13.5, Expect("← Projects")),
            drag(14., 0.6, Move(At(1080., 180.))),
            event(14.8, Expect("Spring launch")),
            event(15.4, Click(At(1080., 180.))),
            event(16., Expect("↑ Fieldwork")),
            drag(16.5, 0.6, Move(At(1080., 250.))),
            drag(18., 0.6, Move(Any("↑ Fieldwork"))),
            event(18.7, Click(Any("↑ Fieldwork"))),
            drag(19.6, 0.6, Move(Any("← Projects"))),
            event(20.3, Click(Any("← Projects"))),
            drag(21., 0.8, Move(At(850., 80.))),
        ],
        "welcome-vector" => vec![
            drag(1., 0.8, Move(Any("Vector"))),
            event(2., Click(Any("Vector"))),
            event(2.6, Expect("Vector templates")),
            drag(3.2, 1.2, Move(At(730., 500.))),
            drag(5., 1., Move(At(1080., 500.))),
            drag(6.5, 0.8, Move(Any("Use this template"))),
        ],
        "welcome-layout" => vec![
            drag(1., 0.8, Move(Any("Layout"))),
            event(2., Click(Any("Layout"))),
            event(2.6, Expect("Layout templates")),
            drag(3.2, 1.2, Move(At(730., 500.))),
            drag(5., 1., Move(At(1080., 500.))),
            drag(6.5, 0.8, Move(Any("Use this template"))),
        ],
        "welcome-raster" => vec![
            drag(1., 0.8, Move(Any("Raster"))),
            event(2., Click(Any("Raster"))),
            event(2.6, Expect("New Raster document")),
            drag(3.2, 1., Move(Any("Custom size"))),
            drag(4.6, 0.7, Move(Any("Transparent"))),
            event(5.5, Click(Any("Transparent"))),
            drag(6., 0.7, Move(Any("Create document"))),
        ],
        "welcome-agents" => vec![
            event(0.3, Click(At(1080., 130.))),
            drag(0.8, 0.6, Move(At(810., 720.))),
            event(1.6, ScrollAt(At(810., 720.), -380.)),
            drag(2.4, 0.8, Move(Any("Learn with AI"))),
            event(3.3, Click(Any("Learn with AI"))),
            event(3.8, Expect("Ask anything…")),
            drag(4.2, 3., Type("How do I make a responsive landing page?")),
            event(7.6, Expect("How do I make a responsive landing page?")),
            drag(8.1, 0.7, Move(Any("Cancel"))),
            event(9., Click(Any("Cancel"))),
            drag(9.6, 0.7, Move(Any("Create with agent"))),
            event(10.5, Click(Any("Create with agent"))),
            drag(
                11.3,
                4.5,
                Type("Create an editable poster for a late-night listening party."),
            ),
            event(
                16.2,
                Expect("Create an editable poster for a late-night listening party."),
            ),
            drag(17., 0.8, Move(Any("Cancel"))),
            event(18., Click(Any("Cancel"))),
            drag(18.8, 0.8, Move(At(850., 80.))),
        ],
        "layout" => vec![
            event(2., Click(Text("Present"))),
            event(5., Key(egui::Key::Escape, Modifiers::NONE)),
            event(7., Key(egui::Key::F, Modifiers::NONE)),
            drag(8., 2., Drag(At(650., 350.), At(1000., 650.))),
            event(12., Key(egui::Key::Z, ctrl())),
        ],
        "chroma" => vec![
            event(1., Click(Text("Chroma key…"))),
            event(2., Click(Any("Original"))),
            event(3., Click(Any("Preview"))),
            event(4., Click(Any("Sample image"))),
            event(5., Click(At(390., 320.))),
            event(6., Click(Any("Apply"))),
            event(8., Key(egui::Key::Z, ctrl())),
            event(
                9.,
                Key(
                    egui::Key::Z,
                    Modifiers {
                        shift: true,
                        ..ctrl()
                    },
                ),
            ),
            event(11., Click(Text("Chroma key…"))),
            event(13., Click(Any("Cancel"))),
        ],
        "graphics" => vec![
            drag(1., 2., Drag(World(100., 90.), World(350., 230.))),
            drag(4., 1.5, Drag(World(350., 230.), World(200., 270.))),
            drag(7., 2., Drag(World(110., 250.), World(340., 100.))),
            event(10., Key(egui::Key::V, Modifiers::NONE)),
            event(11., Key(egui::Key::A, ctrl())),
            event(12., Key(egui::Key::G, ctrl())),
            event(
                13.,
                Key(
                    egui::Key::G,
                    Modifiers {
                        shift: true,
                        ..ctrl()
                    },
                ),
            ),
            event(14., Key(egui::Key::Z, ctrl())),
            event(
                15.,
                Key(
                    egui::Key::Z,
                    Modifiers {
                        shift: true,
                        ..ctrl()
                    },
                ),
            ),
        ],
        "design" => vec![
            drag(1., 1.4, Delta(Field("Rotate"), 44.)),
            drag(3.4, 1.3, Delta(Field("Corner radius"), 56.)),
            event(6.1, Click(Text("Add stroke"))),
            event(8.8, Key(egui::Key::Escape, Modifiers::NONE)),
            event(9.2, Click(Menu("Appearance"))),
            event(9.7, Click(Any("Linear gradient"))),
            event(11.5, Click(World(240., 270.))),
            drag(13., 1.5, Delta(Field("Letter spacing"), 20.)),
            event(15., Click(Text("OpenType features"))),
            event(16., Click(Text("Small caps"))),
            event(17.6, Click(World(990., 303.))),
            event(18., Click(Any("Object"))),
            event(18.5, Hover(Any("Reshape"))),
            event(19.2, Click(Any("Warp mesh"))),
            drag(20., 1.7, Delta(Handle(4), 50.)),
            event(22.5, Click(Text("Done"))),
            event(23.8, Click(Text("Effects"))),
            event(24.3, Scroll(-210.)),
            event(25., Click(Text("Add"))),
            event(25.5, Click(Any("Drop shadow"))),
            event(28., Scroll(-180.)),
            event(29.5, Key(egui::Key::U, Modifiers::NONE)),
            event(30., Scroll(-160.)),
            event(31., Click(Text("Ignore white"))),
            event(32.2, Hover(Text("Trace to vector"))),
            event(35.5, Key(egui::Key::V, Modifiers::NONE)),
            event(35.7, Click(Text("Effects"))),
            event(36., Click(Text("Normal"))),
            event(37., Click(Any("Multiply"))),
            event(38.6, Click(Text("Multiply"))),
            event(39.2, Click(Any("Normal"))),
        ],
        "pixel" => vec![
            drag(1., 1.2, Delta(Field("Size"), 14.)),
            drag(3., 2.2, Drag(World(500., 655.), World(630., 730.))),
            event(6., Click(Text("Iris original"))),
            event(
                6.4,
                Key(
                    egui::Key::J,
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                ),
            ),
            event(
                7.2,
                ModifiedClick(
                    World(360., 700.),
                    Modifiers {
                        alt: true,
                        ..Modifiers::NONE
                    },
                ),
            ),
            drag(8., 2.2, Drag(World(380., 720.), World(430., 760.))),
            event(11.2, Click(Text("Paint accents"))),
            event(11.5, Key(egui::Key::B, Modifiers::NONE)),
            event(12., Click(Text("Add layer mask"))),
            event(12.7, Click(Text("Mask"))),
            event(13.3, Click(Text("Hide"))),
            drag(14., 2.3, Drag(World(545., 675.), World(565., 718.))),
            event(17.5, Click(Text("Pixels"))),
            event(18., Click(Offset("Color", 12., 27.))),
            event(19., Key(egui::Key::Escape, Modifiers::NONE)),
            event(19.5, Click(Text("Palettes"))),
            event(20., Click(Text("Project"))),
            event(21., Click(Swatch("#CBA6F7"))),
            event(22.5, Click(Text("Inspect"))),
            drag(22.8, 1.0, Drag(World(880., 805.), World(1000., 765.))),
            event(24.5, Click(Text("Normal"))),
            event(25.4, Click(Any("Screen"))),
            event(27., Click(Text("Screen"))),
            event(27.6, Click(Any("Normal"))),
            event(28.5, Key(egui::Key::Z, ctrl())),
        ],
        "photo" => vec![
            drag(
                0.9,
                1.4,
                Drag(Slider("Exposure", 0.5), Slider("Exposure", 0.475)),
            ),
            drag(
                3.0,
                1.1,
                Drag(Slider("Shadows", 0.5), Slider("Shadows", 0.57)),
            ),
            event(4.5, Click(Text("Before"))),
            event(5.3, Click(Text("Before"))),
            event(6., Click(Text("Color"))),
            drag(
                6.9,
                1.3,
                Drag(Slider("Temperature", 0.5), Slider("Temperature", 0.52)),
            ),
            drag(
                8.7,
                1.2,
                Drag(Slider("Vibrance", 0.5), Slider("Vibrance", 0.52)),
            ),
            event(10.4, Hover(Text("Color"))),
            event(12., Click(Text("Detail"))),
            drag(
                13.,
                1.3,
                Drag(Slider("Clarity", 0.5), Slider("Clarity", 0.54)),
            ),
            drag(
                15.,
                1.3,
                Drag(Slider("Vignette", 0.5), Slider("Vignette", 0.53)),
            ),
            event(18.2, Click(Text("Light"))),
            event(21.2, Click(Text("Before"))),
            event(22.2, Click(Text("Before"))),
        ],
        "motion" => vec![
            event(0.7, Click(Text("Draw stroke"))),
            event(1.5, Key(egui::Key::Space, Modifiers::NONE)),
            event(4.5, Key(egui::Key::Space, Modifiers::NONE)),
            event(6., Key(egui::Key::Home, Modifiers::NONE)),
            event(7., Key(egui::Key::K, Modifiers::NONE)),
            event(9., Key(egui::Key::Z, ctrl())),
            event(12., Click(Text("Pop in"))),
            event(13., Key(egui::Key::Home, Modifiers::NONE)),
            event(13.5, Key(egui::Key::Space, Modifiers::NONE)),
            event(16., Key(egui::Key::Space, Modifiers::NONE)),
            event(18., Click(Text("Slide up"))),
            event(19., Key(egui::Key::Home, Modifiers::NONE)),
            event(19.5, Key(egui::Key::Space, Modifiers::NONE)),
            event(22., Key(egui::Key::Space, Modifiers::NONE)),
            event(24., Click(Text("Normal"))),
            event(25., Click(Any("Screen"))),
            event(26.3, Click(Text("Screen"))),
            event(27., Click(Any("Normal"))),
            event(28., Key(egui::Key::Space, Modifiers::NONE)),
        ],
        "brand-kit" => vec![
            event(0.7, Click(Text("Project"))),
            event(1.6, Click(Swatch("#CBA6F7"))),
            event(3.2, Click(Swatch("#89B4FA"))),
            event(5.0, Click(Text("Warm accents"))),
            event(6.4, Click(Swatch("#F5C2E7"))),
            event(8.4, Click(Text("Studio colors"))),
            event(10., Click(Text("Brand"))),
            event(10.8, Click(Text("Typography"))),
            event(11.7, Click(Text("SVG"))),
            drag(13., 2.0, Drag(Asset("landscape"), World(360., 800.))),
            event(16.0, Key(egui::Key::Z, ctrl())),
            event(16.4, Click(Text("All"))),
            event(17.0, Hover(Asset("rings"))),
            event(19.6, Click(World(240., 270.))),
            event(20., Click(Text("Typography"))),
            event(21.2, Click(RoleApply("Heading · NimbusSans-Bold"))),
            event(23.0, Click(RoleApply("Body · NimbusRoman-Regular"))),
            event(25.0, Click(RoleApply("Heading · NimbusSans-Bold"))),
            event(26.0, Click(Text("Typography"))),
            event(27., Click(Text("Inspect"))),
        ],
        _ => panic!("unknown scene"),
    }
}

struct Encoder {
    child: Child,
    pipe: ChildStdin,
}
impl Encoder {
    fn new(path: &Path, fps: u32) -> Self {
        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "1600x900",
                "-framerate",
                &fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let pipe = child.stdin.take().unwrap();
        Self { child, pipe }
    }
    fn finish(mut self) {
        drop(self.pipe);
        assert!(self.child.wait().unwrap().success());
    }
}
struct Capture {
    studio: Studio,
    scene: String,
    directory: PathBuf,
    actions: Vec<Action>,
    labels: Vec<(String, Rect)>,
    cursor: Pos2,
    pressed: bool,
    frame: u32,
    warm: u32,
    pending: bool,
    stepped: bool,
    encoder: Option<Encoder>,
    probe: bool,
    events: Vec<Event>,
    total: u32,
    errors: Vec<String>,
    fps: u32,
    ready_since: Instant,
}

fn prepare_kit() -> PathBuf {
    let root = std::env::temp_dir()
        .join(format!("omadesign-recording-{}", std::process::id()))
        .join("Studio kit");
    fs::create_dir_all(&root).unwrap();
    omadesign::brand::create(&root, "Studio kit").unwrap();
    for (from, to) in [
        ("media/logo.svg", "wordmark.svg"),
        ("examples/site-showcase/motion.oma", "rings.oma"),
        ("examples/site-showcase/design.oma", "poster.oma"),
        (
            "examples/fieldwork/.omabrand/illustrations/rolling-hills.svg",
            "landscape.svg",
        ),
    ] {
        fs::copy(from, root.join(".omabrand").join(to)).unwrap();
    }
    let colors = |v: &[u32]| v.iter().map(|&c| Rgba::from_hex(c)).collect();
    omadesign::palette::save_file(
        root.join(".omacolors"),
        &[
            omadesign::palette::Palette::new(
                "Studio colors",
                colors(&[0xCBA6F7, 0x89B4FA, 0xA6E3A1, 0x1E1E2E, 0xCDD6F4, 0xF5E0DC]),
            ),
            omadesign::palette::Palette::new(
                "Warm accents",
                colors(&[0xF5C2E7, 0xF38BA8, 0xFAB387, 0xF9E2AF, 0xB4BEFE, 0x94E2D5]),
            ),
        ],
    )
    .unwrap();
    let loaded = omadesign::typography::add_fonts(
        &root,
        &[
            PathBuf::from("/usr/share/fonts/gsfonts/NimbusSans-Bold.otf"),
            PathBuf::from("/usr/share/fonts/gsfonts/NimbusRoman-Regular.otf"),
        ],
        None,
    )
    .unwrap();
    let mut kit = loaded.kit;
    kit.name = "Studio typography".into();
    kit.roles[0].name = "Heading".into();
    kit.roles[1].name = "Body".into();
    omadesign::typography::save(&root, &kit, Some(loaded.stamp)).unwrap();
    root
}
/// Disposable native documents and real brand assets. The production scanner
/// indexes this directory; no catalog entries or rendered UI are substituted.
fn prepare_welcome_catalog(profile: &Path) -> PathBuf {
    let root = profile.join("catalog");
    let project = root.join("Fieldwork");
    let nested = project.join("Campaigns/Spring launch");
    let kit = root.join("Studio kit");
    for (path, name) in [
        (&project, "Fieldwork"),
        (&nested, "Spring launch"),
        (&kit, "Studio kit"),
    ] {
        fs::create_dir_all(path).unwrap();
        omadesign::brand::create(path, name).unwrap();
        for (from, to) in [
            ("media/logo.svg", "wordmark.svg"),
            (
                "examples/fieldwork/.omabrand/illustrations/rolling-hills.svg",
                "landscape.svg",
            ),
            ("site/public/media/ai/poster.svg", "poster.svg"),
        ] {
            fs::copy(from, path.join(".omabrand").join(to)).unwrap();
        }
    }
    let mut documents = vec![
        (
            project.join("After Hours.oma"),
            omadesign::templates::build("after-hours", 900., 1200., 72.).unwrap(),
            Persona::Design,
        ),
        (
            nested.join("Wide violet.oma"),
            omadesign::project::load_from(Path::new("site/public/media/ai/wide-violet.oma"))
                .unwrap(),
            Persona::Design,
        ),
        (
            nested.join("Solar Social.oma"),
            omadesign::templates::build("solar-social", 1000., 1000., 72.).unwrap(),
            Persona::Design,
        ),
        (
            project.join("Landing page.oma"),
            omadesign::layout_templates::build("layout-hero", 1280., 800., 96.).unwrap(),
            Persona::Layout,
        ),
        (
            kit.join("Motion rings.oma"),
            omadesign::project::load_from(Path::new("examples/site-showcase/motion.oma")).unwrap(),
            Persona::Motion,
        ),
        (
            root.join("Poster study.oma"),
            omadesign::project::load_from(Path::new("site/public/media/ai/poster.oma")).unwrap(),
            Persona::Design,
        ),
        (
            kit.join("Square violet.oma"),
            omadesign::project::load_from(Path::new("site/public/media/ai/square-violet.oma"))
                .unwrap(),
            Persona::Design,
        ),
    ];
    let image = omadesign::photo::load_file(Path::new("examples/site-showcase/pixel-original.png"))
        .unwrap();
    let pixels = Pixels::from_rgba(image.w, image.h, image.data).unwrap();
    let mut raster = Document::new("Iris study", pixels.w as f32, pixels.h as f32, 72.);
    raster.layers = vec![Layer::placed_raster(
        "Iris original",
        pixels,
        Pt::ZERO,
        Pt::new(raster.width, raster.height),
    )];
    documents.insert(3, (project.join("Iris study.oma"), raster, Persona::Pixel));
    // Stable ordering, independent of asynchronous scan order and wall clock.
    for (index, (path, mut document, mode)) in documents.into_iter().enumerate() {
        document.workspace = Some(mode);
        document.name = path.file_stem().unwrap().to_string_lossy().into_owned();
        omadesign::project::save_to(&document, &path).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + Duration::from_secs(1_780_000_000 - index as u64)),
            )
            .unwrap();
    }
    for (index, path) in [&project, &nested, &kit].into_iter().enumerate() {
        fs::File::open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + Duration::from_secs(1_780_000_010 - index as u64)),
            )
            .unwrap();
    }
    root
}

fn seed(scene: &str) -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.show_key_hud = true;
    s.recents.clear();
    s.path = None;
    match scene {
        scene if scene.starts_with("welcome-") => {
            s.show_welcome = true;
            s.welcome_page = omadesign::app::WelcomePage::Recents;
        }
        "layout" => {
            omadesign::shots::apply(&mut s, "layout").unwrap();
        }
        "chroma" => {
            omadesign::shots::apply(&mut s, "pixel-effects").unwrap();
        }
        "graphics" => {
            omadesign::shots::apply(&mut s, "design-tools").unwrap();
        }
        "photo" => {
            s.doc = Document::new("Coast / photo study", 1586., 992., 72.);
            s.persona = Persona::Photo;
            s.tool = Tool::Hand;
            for (name, file) in [
                ("Coast at golden hour.png", "photo"),
                ("Iris study.png", "pixel"),
            ] {
                s.photo.import_bytes(
                    name.into(),
                    &fs::read(format!("examples/site-showcase/{file}-original.png")).unwrap(),
                );
            }
            s.photo.select_image(0);
        }
        "pixel" => {
            let img =
                omadesign::photo::load_file(Path::new("examples/site-showcase/pixel-original.png"))
                    .unwrap();
            s.doc = Document::new("Iris / paint study", img.w as f32, img.h as f32, 72.);
            s.doc.layers = vec![
                Layer::placed_raster(
                    "Iris original",
                    Pixels::from_rgba(img.w, img.h, img.data).unwrap(),
                    Pt::ZERO,
                    Pt::new(img.w as f32, img.h as f32),
                ),
                Layer::raster("Paint accents", img.w, img.h),
            ];
            s.active_layer = Some(1);
            s.persona = Persona::Pixel;
            s.tool = Tool::Brush;
            s.brush.color = Rgba::from_hex(0xF5C2E7);
            s.brush.size = 14.;
            s.brush.opacity = 0.55;
            s.brush.flow = 0.6;
            s.brush.hardness = 0.45;
        }
        _ => {
            let file = if scene == "motion" {
                "motion"
            } else {
                "design"
            };
            s.doc = omadesign::project::load_from(
                &Path::new("examples/site-showcase").join(format!("{file}.oma")),
            )
            .unwrap();
            s.persona = if scene == "motion" {
                Persona::Motion
            } else {
                Persona::Design
            };
            s.tool = Tool::Select;
            s.active_layer = Some(1);
            if scene == "motion" {
                s.doc.motion.tracks.clear();
                s.doc.motion.duration = 3.;
                s.doc.motion.looped = true;
                s.motion_preset_options.duration = 1.7;
                s.motion_preset_options.stagger = 0.06;
                s.motion_preset_options.start_at_playhead = false;
                s.selection = s.doc.layers[1]
                    .kind
                    .shapes()
                    .unwrap()
                    .iter()
                    .filter(|shape| omadesign::motion_presets::Preset::DrawStroke.supports(shape))
                    .map(|shape| (1, shape.id))
                    .collect();
            } else {
                let sh = s.doc.layers[1]
                    .kind
                    .shapes()
                    .unwrap()
                    .iter()
                    .find(|sh| {
                        matches!(sh.geom, Geom::Rect { .. })
                            && sh.world_bbox().center().x > 900.
                            && sh.world_bbox().center().y < 400.
                    })
                    .unwrap();
                s.selection = vec![(1, sh.id)];
            }
        }
    }
    if matches!(scene, "brand-kit" | "pixel") {
        let root = prepare_kit();
        s.path = Some(root.join("studio.oma"));
        s.libraries.project_scope = true;
        if scene == "brand-kit" {
            s.libraries.sidebar = omadesign::app::libraries::Sidebar::Palettes;
        }
    }
    // These are disposable showcase copies, so keep their layer/object names in
    // the same US English used by the website without touching source projects.
    for layer in &mut s.doc.layers {
        layer.name = layer
            .name
            .replace("Colour", "Color")
            .replace("colour", "color");
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                shape.name = shape
                    .name
                    .replace("Colour", "Color")
                    .replace("colour", "color");
            }
        }
    }
    s.need_fit = true;
    s.history.clear();
    s.dirty = false;
    s.status = "Native studio recording".into();
    s
}
impl Capture {
    fn new(scene: String, directory: PathBuf, probe: bool, fps: u32) -> Self {
        let studio = seed(&scene);
        let seconds = match scene.as_str() {
            "welcome-browse" => 23,
            "welcome-vector" | "welcome-layout" => 9,
            "welcome-raster" => 8,
            "welcome-agents" => 21,
            "chroma" | "layout" => 15,
            "graphics" => 17,
            "design" => 42,
            "photo" => 24,
            "pixel" | "motion" | "brand-kit" => 30,
            _ => panic!("unknown scene"),
        };
        let mut actions = schedule(&scene);
        for action in &mut actions {
            action.start = action.start * fps / FPS;
            action.end = action.end * fps / FPS;
        }
        Self {
            studio,
            actions,
            scene,
            directory,
            labels: vec![],
            cursor: egui::pos2(850., 80.),
            pressed: false,
            frame: 0,
            warm: 0,
            pending: false,
            stepped: false,
            encoder: None,
            probe,
            events: vec![],
            total: if probe { 1 } else { seconds * fps },
            errors: vec![],
            fps,
            ready_since: Instant::now(),
        }
    }
    fn world(&self, x: f32, y: f32) -> Pos2 {
        let r = self.studio.canvas_rect.unwrap();
        let p = self.studio.view.to_screen(Pt::new(x, y));
        r.min + egui::vec2(p.x, p.y)
    }
    fn target(&self, t: &Target) -> Option<Pos2> {
        let label = |name: &str, right: bool| {
            self.labels
                .iter()
                .find(|(text, r)| {
                    (text == name || text.starts_with(&format!("{name}   ·")))
                        && (!right || r.left() > 1100.)
                })
                .map(|(_, r)| *r)
        };
        match t {
            Target::Text(s) => label(s, true).map(|r| r.center()),
            Target::Any(s) => label(s, false).map(|r| r.center()),
            Target::Left(s) => self
                .labels
                .iter()
                .find(|(text, rect)| text == s && rect.center().x < 640.)
                .map(|(_, rect)| rect.center()),
            Target::At(x, y) => Some(egui::pos2(*x, *y)),
            Target::World(x, y) => Some(self.world(*x, *y)),
            Target::Menu(s) => label(s, true).map(|r| egui::pos2(1565., r.center().y)),
            Target::Offset(s, x, y) => {
                label(s, true).map(|r| egui::pos2(r.left() + x, r.center().y + y))
            }
            Target::RoleApply(s) => {
                let r = label(s, true)?;
                self.labels
                    .iter()
                    .find(|(text, f)| text == "Apply" && (f.center().y - r.center().y).abs() < 6.)
                    .map(|(_, r)| r.center())
            }
            Target::Field(s) => {
                let r = label(s, true)?;
                self.labels
                    .iter()
                    .filter(|(_, f)| {
                        f.left() > r.right() + 2. && (f.center().y - r.center().y).abs() < 6.
                    })
                    .min_by(|(_, a), (_, b)| a.left().total_cmp(&b.left()))
                    .map(|(_, r)| r.center())
            }
            Target::Slider(s, k) => {
                let r = label(s, true)?;
                Some(egui::pos2(1333. + *k * 245., r.center().y + 29.))
            }
            Target::Swatch(s) => label(s, true).map(|r| r.center() - egui::vec2(0., 25.)),
            Target::Asset(s) => label(s, true).map(|r| r.center() - egui::vec2(0., 52.)),
            Target::Handle(i) => self
                .studio
                .deformation
                .as_ref()
                .and_then(|d| d.cage.handles().get(*i).copied())
                .map(|p| self.world(p.x, p.y)),
        }
    }
    fn click(&mut self, p: Pos2, modifiers: Modifiers) {
        self.cursor = p;
        self.events.extend([
            Event::ModifiersChanged(modifiers),
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            },
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers,
            },
        ]);
    }
    fn step(&mut self) {
        let mut actions = std::mem::take(&mut self.actions);
        for a in &mut actions {
            if self.frame < a.start || self.frame > a.end {
                continue;
            }
            if self.frame == a.start {
                match &a.kind {
                    ActionKind::Click(t) => {
                        if let Some(p) = self.target(t) {
                            self.click(p, Modifiers::NONE);
                        } else {
                            self.errors
                                .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                        }
                    }
                    ActionKind::ModifiedClick(t, modifiers) => {
                        if let Some(p) = self.target(t) {
                            self.click(p, *modifiers);
                        } else {
                            self.errors
                                .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                        }
                    }
                    ActionKind::Hover(t) => {
                        if let Some(p) = self.target(t) {
                            self.cursor = p;
                            self.events.push(Event::PointerMoved(p));
                        } else {
                            self.errors
                                .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                        }
                    }
                    ActionKind::Key(key, modifiers) => {
                        self.events.push(Event::ModifiersChanged(*modifiers));
                        for pressed in [true, false] {
                            self.events.push(Event::Key {
                                key: *key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: *modifiers,
                            });
                        }
                    }
                    ActionKind::Scroll(delta) => {
                        let p = egui::pos2(1440., 520.);
                        self.cursor = p;
                        self.events.extend([
                            Event::PointerMoved(p),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0., *delta),
                                modifiers: Modifiers::NONE,
                                phase: egui::TouchPhase::Move,
                            },
                        ]);
                    }
                    ActionKind::Move(target) => {
                        a.points = self.target(target).map(|p| (self.cursor, p));
                        if a.points.is_none() {
                            self.errors
                                .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                        }
                    }
                    ActionKind::ScrollAt(target, delta) => {
                        if let Some(p) = self.target(target) {
                            self.cursor = p;
                            self.events.extend([
                                Event::PointerMoved(p),
                                Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    delta: egui::vec2(0., *delta),
                                    modifiers: Modifiers::NONE,
                                    phase: egui::TouchPhase::Move,
                                },
                            ]);
                        } else {
                            self.errors
                                .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                        }
                    }
                    ActionKind::Expect(text) => {
                        if !self.labels.iter().any(|(label, _)| label.contains(text)) {
                            self.errors.push(format!(
                                "frame {}: expected visible text {text:?}",
                                self.frame
                            ));
                        }
                    }
                    ActionKind::Type(_) => {}
                    ActionKind::Drag(from, to) => a.points = self.target(from).zip(self.target(to)),
                    ActionKind::Delta(t, dx) => {
                        a.points = self.target(t).map(|p| (p, p + egui::vec2(*dx, 0.)))
                    }
                }
                if matches!(a.kind, ActionKind::Drag(..) | ActionKind::Delta(..)) {
                    if let Some((p, _)) = a.points {
                        self.cursor = p;
                        self.pressed = true;
                        self.events.extend([
                            Event::PointerMoved(p),
                            Event::PointerButton {
                                pos: p,
                                button: PointerButton::Primary,
                                pressed: true,
                                modifiers: Modifiers::NONE,
                            },
                        ]);
                    } else {
                        self.errors
                            .push(format!("frame {}: {:?} target missing", self.frame, a.kind));
                    }
                }
            }
            if let ActionKind::Type(text) = &a.kind {
                let chars: Vec<_> = text.chars().collect();
                let span = (a.end - a.start + 1) as usize;
                let elapsed = (self.frame - a.start) as usize;
                let from = elapsed * chars.len() / span;
                let to = (elapsed + 1) * chars.len() / span;
                if to > from {
                    self.events
                        .push(Event::Text(chars[from..to].iter().collect()));
                }
            }
            if let Some((from, to)) = a.points {
                let t = (self.frame - a.start) as f32 / (a.end - a.start).max(1) as f32;
                let eased = t * t * (3. - 2. * t);
                self.cursor = from + (to - from) * eased;
                self.events.push(Event::PointerMoved(self.cursor));
                if self.frame == a.end && !matches!(a.kind, ActionKind::Move(_)) {
                    self.pressed = false;
                    self.events.push(Event::PointerButton {
                        pos: self.cursor,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
        }
        self.actions = actions;
    }
    fn read_labels(&mut self, ctx: &egui::Context) {
        fn visit(shape: &egui::Shape, clip: Rect, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(t) => {
                    let r = t.galley.rect.translate(t.pos.to_vec2());
                    if clip.contains(r.center()) {
                        out.push((t.galley.job.text.clone(), r));
                    }
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        visit(s, clip, out)
                    }
                }
                _ => {}
            }
        }
        let layers = ctx.memory(|m| m.layer_ids().collect::<Vec<_>>());
        self.labels.clear();
        ctx.graphics(|g| {
            for layer in layers {
                if let Some(list) = g.get(layer) {
                    for shape in list.all_entries() {
                        visit(&shape.shape, shape.clip_rect, &mut self.labels);
                    }
                }
            }
        });
    }
}
impl eframe::App for Capture {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input
            .events
            .retain(|e| matches!(e, Event::Screenshot { .. }));
        input.hovered_files.clear();
        input.dropped_files.clear();
        input.focused = true;
        input.time = Some(self.frame as f64 / self.fps as f64);
        input.events.push(Event::ModifiersChanged(Modifiers::NONE));
        if self.warm >= 28 && !self.pending && !self.stepped {
            if !self.probe {
                self.step();
            }
            self.stepped = true;
        }
        input.events.append(&mut self.events);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        eframe::App::ui(&mut self.studio, ui, frame);
        self.read_labels(&ctx);
        // Only a pointer indicator is overlaid. Every panel and artwork pixel
        // below it is the real native application viewport.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("recording-pointer"),
        ));
        painter.circle_filled(
            self.cursor,
            if self.pressed { 8. } else { 5. },
            egui::Color32::from_rgba_unmultiplied(
                203,
                166,
                247,
                if self.pressed { 180 } else { 90 },
            ),
        );
        painter.circle_stroke(
            self.cursor,
            if self.pressed { 8. } else { 5. },
            egui::Stroke::new(1.5, egui::Color32::WHITE),
        );
        self.warm += 1;
        if self.warm == 2 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                SIZE[0] as f32,
                SIZE[1] as f32,
            )));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| {
                if let Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = shot {
            assert_eq!(image.size, [1600, 900]);
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let encoder = self.encoder.get_or_insert_with(|| {
                Encoder::new(
                    &self.directory.join(format!("{}.mp4", self.scene)),
                    self.fps,
                )
            });
            encoder.pipe.write_all(&bytes).unwrap();
            if self.frame % (self.fps * 2) == 0 {
                image::save_buffer(
                    self.directory
                        .join(format!("{}-{:04}.png", self.scene, self.frame)),
                    &bytes,
                    1600,
                    900,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                fs::write(
                    self.directory
                        .join(format!("{}-{:04}.txt", self.scene, self.frame)),
                    self.labels
                        .iter()
                        .map(|(s, r)| format!("{s:?} {r:?}\n"))
                        .collect::<String>(),
                )
                .unwrap();
            }
            self.frame += 1;
            self.ready_since = Instant::now();
            self.pending = false;
            self.stepped = false;
            if self.frame >= self.total {
                if matches!(self.scene.as_str(), "graphics" | "chroma") {
                    omadesign::project::save_to(
                        &self.studio.doc,
                        &self.directory.join(format!("{}-final.oma", self.scene)),
                    )
                    .unwrap();
                }
                self.encoder.take().unwrap().finish();
                fs::write(
                    self.directory.join(format!("{}-errors.json", self.scene)),
                    serde_json::to_vec_pretty(&self.errors).unwrap(),
                )
                .unwrap();
                eprintln!(
                    "{}: {} frames, {} unresolved input targets",
                    self.scene,
                    self.frame,
                    self.errors.len()
                );
                for e in &self.errors {
                    eprintln!("{e}");
                }
                self.studio.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        // An active stroke or animation deliberately keeps live thumbnails
        // dirty. Waiting for those thumbnails would freeze the replay before
        // the next pointer movement/release (or pause) can ever arrive. Capture
        // each real native frame while interacting, then wait for settled
        // previews again after the interaction ends.
        let live_interaction = self.pressed || self.studio.playing;
        if self.stepped
            && !self.pending
            && (live_interaction || omadesign::ui::scene_ready(&ctx, &self.studio))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.pending = true;
        }
        if self.ready_since.elapsed() > Duration::from_secs(180) {
            self.errors.push(format!(
                "frame {}: native readiness or screenshot timed out",
                self.frame
            ));
            fs::write(
                self.directory.join(format!("{}-errors.json", self.scene)),
                serde_json::to_vec_pretty(&self.errors).unwrap(),
            )
            .unwrap();
            panic!("{}: native readiness timeout", self.scene);
        }
        ctx.request_repaint();
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let scene = args
        .first()
        .expect("SCENE OUTPUT_DIR [--probe] [--fps 30|60]")
        .clone();
    let directory = PathBuf::from(args.get(1).expect("output directory"));
    fs::create_dir_all(&directory).unwrap();
    let probe = args.iter().any(|s| s == "--probe");
    let fps = args
        .iter()
        .position(|s| s == "--fps")
        .map(|i| {
            args.get(i + 1)
                .expect("--fps needs 30 or 60")
                .parse::<u32>()
                .expect("invalid FPS")
        })
        .unwrap_or(FPS);
    assert!(matches!(fps, 30 | 60), "FPS must be 30 or 60");
    // Keep the real desktop HOME/theme, isolate every app write and credential.
    let profile = std::env::temp_dir().join(format!("omadesign-recording-{}", std::process::id()));
    for (variable, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = profile.join(name);
        fs::create_dir_all(&path).unwrap();
        // No threads exist yet: this capture process owns its environment.
        unsafe {
            std::env::set_var(variable, path);
        }
    }
    let catalog = scene
        .starts_with("welcome-")
        .then(|| prepare_welcome_catalog(&profile));
    fs::write(
        directory.join(format!("{scene}-capture.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scene": scene, "fps": fps, "size": SIZE, "profile": profile, "catalog": catalog,
            "renderer": "native WGPU", "interaction": "real egui pointer and keyboard events",
        }))
        .unwrap(),
    )
    .unwrap();
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1600., 900.])
            .with_min_inner_size([1600., 900.])
            .with_max_inner_size([1600., 900.])
            .with_resizable(false)
            .with_title(format!("omadesign — isolated {scene} recording")),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "omadesign-studio-recording",
        options,
        Box::new(move |cc| {
            omadesign::ui::theme::apply(&cc.egui_ctx);
            cc.egui_ctx.set_pixels_per_point(1.);
            if let Some(root) = &catalog {
                omadesign::ui::set_capture_catalog_root(&cc.egui_ctx, root);
            }
            Ok(Box::new(Capture::new(scene, directory, probe, fps)))
        }),
    )
}
