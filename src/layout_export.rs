//! Portable frame exports. A selected frame is an explicit data boundary: unrelated
//! layers and siblings never enter the exported document.
mod responsive;

use crate::document::{Document, Fill, Shape};
use crate::geom::{Geom, Pt};
use crate::layout::{Constraint, Sizing, StackAlign, StackAxis, StackFlow, StackJustify};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub fn frame_document(doc: &Document, layer: usize, frame_id: u64) -> Result<Document, String> {
    let frame = doc
        .find_shape(layer, frame_id)
        .filter(|s| s.layout.frame)
        .ok_or("Select a frame to export")?;
    let bounds = frame.world_bbox();
    if !bounds.width().is_finite()
        || !bounds.height().is_finite()
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
    {
        return Err("Frame dimensions must be finite and positive".into());
    }
    let mut ids: HashSet<_> = crate::layout::descendants(doc, layer, frame_id)
        .into_iter()
        .collect();
    ids.insert(frame_id);
    let mut layers: HashSet<_> = doc.layer_ancestors(layer).into_iter().collect();
    layers.insert(layer);
    let delta = Pt::new(-bounds.min.x, -bounds.min.y);
    let mut output = Document::new(frame.name.clone(), 1.0, 1.0, doc.dpi);
    output.width = bounds.width().max(1.0);
    output.height = bounds.height().max(1.0);
    output.transparent = true;
    output.artboards = vec![crate::document::Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(output.width, output.height),
    )];
    output.cloud = None;
    output.comments.clear();
    output.guides.clear();
    output.layers = doc
        .layers
        .iter()
        .enumerate()
        .filter(|(i, _)| layers.contains(i))
        .map(|(i, source)| {
            let mut l = source.clone();
            if let Some(shapes) = l.kind.shapes_mut() {
                shapes.retain(|shape| i == layer && ids.contains(&shape.id));
                for shape in shapes {
                    shape.geom.translate(delta);
                    if shape.id == frame_id {
                        shape.layout.parent = None;
                    }
                }
            }
            l.mask_origin += delta;
            l
        })
        .collect();
    let referenced: HashSet<_> = output
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .flat_map(|s| {
            crate::layout_tokens::TokenProperty::all()
                .into_iter()
                .filter_map(|property| s.layout.tokens.get(property))
        })
        .collect();
    output.layout_tokens = doc
        .layout_tokens
        .iter()
        .filter(|token| referenced.contains(&token.id))
        .cloned()
        .collect();
    Ok(output)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn axis_css(shape: &Shape, parent: Option<&Shape>, horizontal: bool, css: &mut String) {
    let bounds = shape.geom.bbox();
    let (name, span, sizing, min, max, constraint) = if horizontal {
        (
            "width",
            bounds.width(),
            shape.layout.width,
            shape.layout.min_width,
            shape.layout.max_width,
            shape.layout.constraint_x,
        )
    } else {
        (
            "height",
            bounds.height(),
            shape.layout.height,
            shape.layout.min_height,
            shape.layout.max_height,
            shape.layout.constraint_y,
        )
    };
    if let Some(parent) = parent {
        let pb = parent.geom.bbox();
        let stack = parent
            .layout
            .stack
            .as_ref()
            .filter(|_| !shape.layout.absolute);
        if let Some(stack) = stack {
            let main = horizontal == (stack.direction == StackAxis::Horizontal);
            match sizing {
                Sizing::Fixed => {
                    let _ = write!(css, "{name}:{:.3}px;", span.max(1.));
                }
                Sizing::Hug => {
                    let _ = write!(css, "{name}:max-content;");
                }
                Sizing::Fill if main && stack.flow != StackFlow::Grid => {
                    // Wrapping uses the authored basis when choosing line breaks,
                    // then distributes each line's remaining space like native flow.
                    if stack.flow == StackFlow::Wrap {
                        let _ = write!(css, "flex:1 1 {:.3}px;{name}:0;", span.max(1.));
                    } else {
                        let _ = write!(css, "flex:1 1 0;{name}:0;");
                    }
                }
                Sizing::Fill => {
                    let _ = write!(css, "{name}:100%;align-self:stretch;");
                }
            }
            if sizing != Sizing::Fill && main {
                css.push_str("flex-shrink:0;");
            }
        } else {
            let (start, end, min_pos, max_pos, pmin, pmax) = if horizontal {
                (
                    "left",
                    "right",
                    bounds.min.x,
                    bounds.max.x,
                    pb.min.x,
                    pb.max.x,
                )
            } else {
                (
                    "top",
                    "bottom",
                    bounds.min.y,
                    bounds.max.y,
                    pb.min.y,
                    pb.max.y,
                )
            };
            let parent_span = (pmax - pmin).max(1.0);
            if sizing == Sizing::Fill || constraint == Constraint::Stretch {
                let _ = write!(
                    css,
                    "{start}:{:.3}px;{end}:{:.3}px;{name}:auto;",
                    min_pos - pmin,
                    pmax - max_pos
                );
            } else {
                match constraint {
                    Constraint::End => {
                        let _ = write!(css, "{end}:{:.3}px;", pmax - max_pos);
                    }
                    Constraint::Center => {
                        let _ = write!(
                            css,
                            "{start}:calc(50% + {:.3}px);",
                            min_pos - (pmin + pmax) * 0.5
                        );
                    }
                    Constraint::Scale => {
                        let _ = write!(
                            css,
                            "{start}:{:.5}%;{name}:{:.5}%;",
                            (min_pos - pmin) / parent_span * 100.,
                            span / parent_span * 100.
                        );
                    }
                    _ => {
                        let _ = write!(css, "{start}:{:.3}px;", min_pos - pmin);
                    }
                }
                if constraint != Constraint::Scale {
                    if sizing == Sizing::Hug {
                        let _ = write!(css, "{name}:max-content;");
                    } else {
                        let _ = write!(css, "{name}:{:.3}px;", span.max(1.));
                    }
                }
            }
        }
    } else if horizontal {
        let _ = write!(css, "width:100%;max-width:{:.3}px;", span.max(1.));
    } else if sizing == Sizing::Hug {
        css.push_str("height:max-content;");
    } else {
        let _ = write!(css, "height:{:.3}px;", span.max(1.));
    }
    if let Some(min) = min {
        let _ = write!(css, "min-{name}:{:.3}px;", min.max(0.));
    } else if sizing == Sizing::Fill {
        let _ = write!(css, "min-{name}:0;");
    }
    if let Some(max) = max {
        let _ = write!(css, "max-{name}:{:.3}px;", max.max(1.));
    }
}

fn fill_css(fill: &Fill) -> String {
    match fill {
        Fill::Gradient(g) => {
            use crate::gradient::GradientKind;
            let stops = g
                .stops
                .iter()
                .map(|s| format!("{} {:.3}%", s.color.css(), s.offset * 100.))
                .collect::<Vec<_>>()
                .join(",");
            let angle = (g.to[1] - g.from[1])
                .atan2(g.to[0] - g.from[0])
                .to_degrees();
            match g.kind {
                GradientKind::Linear => format!("linear-gradient({:.3}deg,{stops})", angle + 90.),
                GradientKind::Conic => format!(
                    "conic-gradient(from {:.3}deg at {:.3}% {:.3}%,{stops})",
                    angle + 90.,
                    g.from[0] * 100.,
                    g.from[1] * 100.
                ),
                _ => format!(
                    "radial-gradient(circle at {:.3}% {:.3}%,{stops})",
                    g.from[0] * 100.,
                    g.from[1] * 100.
                ),
            }
        }
        Fill::None => "transparent".into(),
        Fill::Solid(color) => color.css(),
        Fill::Linear { from, to, c0, c1 } => {
            let angle = (to[0] - from[0]).atan2(-(to[1] - from[1])).to_degrees();
            format!("linear-gradient({angle:.3}deg,{}, {})", c0.css(), c1.css())
        }
        Fill::Radial { c0, c1 } => format!("radial-gradient(ellipse,{}, {})", c0.css(), c1.css()),
    }
}

struct Html<'a> {
    shapes: HashMap<u64, &'a Shape>,
    children: HashMap<u64, Vec<u64>>,
    fonts: HashMap<String, (String, f32, f32)>,
    styles: String,
    interactions: serde_json::Map<String, serde_json::Value>,
    visited: HashSet<u64>,
}

impl<'a> Html<'a> {
    fn font(&mut self, run: &crate::geom::TypeRun) -> (String, f32, f32) {
        if let Some(font) = self.fonts.get(&run.font) {
            return font.clone();
        }
        let family = format!("oma-font-{}", self.fonts.len());
        let bytes = if run.font.starts_with("omatype:") {
            crate::text::project_font_bytes(&run.font).map(|b| b.as_ref().clone())
        } else {
            let path = if run.font.is_empty() {
                crate::text::default_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                run.font.clone()
            };
            std::fs::read(path)
                .ok()
                .filter(|data| data.len() <= 20_000_000)
        };
        let mut metrics = (0.8, 0.2);
        let name = if let Some(bytes) =
            bytes.filter(|bytes| rustybuzz::Face::from_slice(bytes, 0).is_some())
        {
            if let Some(face) = rustybuzz::Face::from_slice(&bytes, 0) {
                metrics = (
                    face.ascender() as f32 / face.units_per_em() as f32,
                    -(face.descender() as f32) / face.units_per_em() as f32,
                );
            }
            let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            let _ = write!(
                self.styles,
                "@font-face{{font-family:'{family}';src:url(data:font/ttf;base64,{data});font-display:block;}}"
            );
            format!("'{family}',sans-serif")
        } else {
            "sans-serif".into()
        };
        let font = (name, metrics.0, metrics.1);
        self.fonts.insert(run.font.clone(), font.clone());
        font
    }

    fn node(
        &mut self,
        id: u64,
        parent: Option<&Shape>,
        out: &mut String,
        depth: usize,
    ) -> Result<(), String> {
        let shape = *self.shapes.get(&id).ok_or("Missing frame child")?;
        if !shape.visible || shape.guide {
            return Ok(());
        }
        if depth >= 64 || !self.visited.insert(id) {
            return Err("Invalid frame hierarchy in HTML export".into());
        }
        self.styles.push_str(&responsive::breakpoints(shape));
        let mut css = String::new();
        let in_stack = parent.is_some_and(|p| p.layout.stack.is_some()) && !shape.layout.absolute;
        css.push_str(if parent.is_none() || in_stack {
            "position:relative;"
        } else {
            "position:absolute;"
        });
        axis_css(shape, parent, true, &mut css);
        axis_css(shape, parent, false, &mut css);
        let _ = write!(css, "opacity:{:.4};", shape.opacity.clamp(0., 1.));
        let _ = write!(css, "mix-blend-mode:{};", shape.blend.css());
        if shape.rotation.abs() > 1e-5 {
            let _ = write!(
                css,
                "transform:rotate({:.5}deg);",
                shape.rotation.to_degrees()
            );
        }
        let rect = matches!(shape.geom, Geom::Rect { .. });
        if rect {
            let corners = shape.effective_corners();
            let _ = write!(
                css,
                "background:{};border-radius:{:.3}px {:.3}px {:.3}px {:.3}px;",
                fill_css(&shape.style.fill),
                corners[0],
                corners[1],
                corners[2],
                corners[3]
            );
            if let Some(stroke) = shape.style.stroke.as_ref().filter(|s| s.width > 0.) {
                let _ = write!(
                    css,
                    "outline:{:.3}px {} {};outline-offset:{:.3}px;",
                    stroke.width,
                    if stroke.dash.is_some() {
                        "dashed"
                    } else {
                        "solid"
                    },
                    stroke.color.css(),
                    -stroke.width * 0.5
                );
            }
        }
        if shape.layout.frame {
            css.push_str("isolation:isolate;");
            css.push_str(if shape.layout.clip {
                "overflow:hidden;"
            } else {
                "overflow:visible;"
            });
            if let Some(stack) = &shape.layout.stack {
                css.push_str(&responsive::stack_css(stack));
            }
        }
        if let Some(ratio) = shape.layout.aspect_ratio {
            let _ = write!(css, "height:auto;aspect-ratio:{ratio:.6};");
        }

        if shape.filters.active() {
            let mut filters = String::new();
            for fx in &shape.filters.items {
                use crate::filter::Fx;
                match fx {
                    Fx::Shadow {
                        dx,
                        dy,
                        blur,
                        color,
                    } => {
                        let _ = write!(
                            filters,
                            "drop-shadow({dx}px {dy}px {blur}px {}) ",
                            color.css()
                        );
                    }
                    Fx::Blur { std } => {
                        let _ = write!(filters, "blur({std}px) ");
                    }
                    Fx::Saturate { amount } => {
                        let _ = write!(filters, "saturate({amount}) ");
                    }
                    Fx::Brightness { amount } => {
                        let _ = write!(filters, "brightness({amount}) ");
                    }
                    Fx::Contrast { amount } => {
                        let _ = write!(filters, "contrast({amount}) ");
                    }
                    Fx::HueRotate { degrees } => {
                        let _ = write!(filters, "hue-rotate({degrees}deg) ");
                    }
                    Fx::Invert { amount } => {
                        let _ = write!(filters, "invert({amount}) ");
                    }
                    Fx::InnerShadow {
                        dx,
                        dy,
                        blur,
                        color,
                    } => {
                        let _ = write!(
                            css,
                            "box-shadow:inset {dx}px {dy}px {blur}px {};",
                            color.css()
                        );
                    }
                    _ => {}
                }
            }
            if !filters.is_empty() {
                let _ = write!(css, "filter:{filters};");
            }
        }
        let interactive = !shape.layout.interactions.is_empty();
        if interactive {
            self.interactions.insert(
                id.to_string(),
                serde_json::to_value(&shape.layout.interactions).map_err(|e| e.to_string())?,
            );
        }
        let action_data =
            escape(&serde_json::to_string(&shape.layout.interactions).map_err(|e| e.to_string())?);
        let component = if shape.layout.component.is_some() {
            format!(" data-component=\"{id}\"")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "<div id=\"oma-{id}\" data-node=\"{id}\" data-actions=\"{action_data}\" class=\"node{}\" aria-label=\"{}\"{}{component} style=\"{}\">",
            if interactive { " interactive" } else { "" },
            escape(&shape.name),
            if interactive {
                " role=\"button\" tabindex=\"0\""
            } else {
                ""
            },
            escape(&css)
        );
        if let Some(image) = shape.layout.image.as_ref().filter(|_| rect) {
            let fit = match image.fit {
                crate::layout_images::ImageFit::Cover => "cover",
                crate::layout_images::ImageFit::Contain => "contain",
                crate::layout_images::ImageFit::Stretch => "fill",
            };
            let _ = write!(
                out,
                "<img alt=\"{}\" src=\"data:image/png;base64,{}\" style=\"position:absolute;inset:0;width:100%;height:100%;object-fit:{fit};object-position:{:.3}% {:.3}%;border-radius:inherit;pointer-events:none\">",
                escape(&shape.name),
                escape(&image.data),
                image.focal.x * 100.,
                image.focal.y * 100.
            );
        }
        if let Geom::Text(run) = &shape.geom {
            let (font, asc, desc) = self.font(run);
            let b = shape.geom.bbox();
            if run.wrap_width.is_none() {
                let _ = write!(
                    out,
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"100%\" height=\"100%\" style=\"overflow:visible\"><text font-family=\"{}\" font-size=\"{}\" letter-spacing=\"{}\" fill=\"{}\" style=\"font-kerning:{};font-feature-settings:'liga' {},'tnum' {},'smcp' {}\" xml:space=\"preserve\">",
                    b.min.x,
                    b.min.y,
                    b.width().max(1.),
                    b.height().max(1.),
                    escape(&font),
                    run.px,
                    run.tracking,
                    match &shape.style.fill {
                        Fill::Solid(c) => c.css(),
                        _ => "#111111".into(),
                    },
                    if run.kern { "normal" } else { "none" },
                    u8::from(run.liga),
                    u8::from(run.tnum),
                    u8::from(run.smcp)
                );
                for (line, content) in run.content.split('\n').enumerate() {
                    let _ = write!(
                        out,
                        "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                        run.origin.x,
                        run.origin.y + line as f32 * run.line_height(),
                        escape(content)
                    );
                }
                out.push_str("</text></svg></div>");
                return Ok(());
            }
            let px = shape.layout.text_size.unwrap_or(run.px);
            let leading = run.line_height() / run.px.max(1.) * px;
            let baseline = (leading - (asc + desc) * px) * 0.5 + asc * px;
            let mut text_style = format!(
                "position:relative;display:block;left:{:.3}px;top:{:.3}px;font-family:{font};font-size:{:.3}px;line-height:{:.3}px;letter-spacing:{:.3}px;font-kerning:{};font-feature-settings:'liga' {},'tnum' {},'smcp' {};white-space:pre;color:{};",
                run.origin.x - b.min.x,
                run.origin.y - b.min.y - baseline,
                px,
                leading,
                run.tracking,
                if run.kern { "normal" } else { "none" },
                u8::from(run.liga),
                u8::from(run.tnum),
                u8::from(run.smcp),
                match &shape.style.fill {
                    Fill::Solid(c) => c.css(),
                    _ => "#111111".into(),
                }
            );
            if let Some(width) = run.wrap_width {
                let _ = write!(
                    text_style,
                    "width:{width:.3}px;max-width:100%;white-space:pre-wrap;overflow-wrap:anywhere;text-align:{};",
                    match run.align {
                        crate::geom::TextAlign::Start => "left",
                        crate::geom::TextAlign::Center => "center",
                        crate::geom::TextAlign::End => "right",
                    }
                );
            }
            let _ = write!(
                out,
                "<span style=\"{}\">{}</span>",
                escape(&text_style),
                escape(&run.content)
            );
        } else if !rect {
            let mut art = shape.clone();
            art.rotation = 0.;
            art.opacity = 1.;
            art.blend = crate::color::Blend::Normal;
            out.push_str(&crate::svg::shape_fragment(&art));
        }
        for child in self.children.get(&id).cloned().unwrap_or_default() {
            self.node(child, Some(shape), out, depth + 1)?;
        }
        out.push_str("</div>");
        Ok(())
    }
}

pub fn export_html(doc: &Document, layer: usize, frame_id: u64) -> Result<String, String> {
    let frame = doc
        .find_shape(layer, frame_id)
        .filter(|s| s.layout.frame)
        .ok_or("Select a frame to export")?;
    let shapes = doc
        .layers
        .get(layer)
        .and_then(|l| l.kind.shapes())
        .ok_or("Missing frame layer")?;
    let mut html = Html {
        shapes: shapes.iter().map(|s| (s.id, s)).collect(),
        children: HashMap::new(),
        fonts: HashMap::new(),
        styles: String::new(),
        interactions: serde_json::Map::new(),
        visited: HashSet::new(),
    };
    for s in shapes {
        if let Some(parent) = s.layout.parent {
            html.children.entry(parent).or_default().push(s.id);
        }
    }
    let mut pending = vec![frame_id];
    let mut screens = HashSet::new();
    let mut body = String::new();
    let mut instances = HashSet::new();
    let mut variant_targets = HashSet::new();
    while let Some(id) = pending.pop() {
        if !screens.insert(id) {
            continue;
        }
        if screens.len() > 256 {
            return Err("Prototype has more than 256 linked screens".into());
        }
        let mut content = String::new();
        html.visited.clear();
        html.node(id, None, &mut content, 0)?;
        let _ = write!(body, "<template id=\"screen-{id}\">{content}</template>");
        for node in &html.visited {
            if let Some(shape) = html.shapes.get(node) {
                if matches!(
                    shape.layout.component,
                    Some(crate::layout_components::ComponentBinding::Instance { .. })
                ) {
                    instances.insert(*node);
                }
                for interaction in &shape.layout.interactions {
                    use crate::layout_prototype::PrototypeAction;
                    if let PrototypeAction::SetVariant { component } = interaction.action {
                        variant_targets.insert(component);
                    }
                    match interaction.action {
                        PrototypeAction::Navigate { target }
                        | PrototypeAction::OpenOverlay { target }
                        | PrototypeAction::SetVariant { component: target }
                            if html.shapes.get(&target).is_some_and(|s| s.layout.frame) =>
                        {
                            pending.push(target);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    // Compile each used variant through the native merge engine. This preserves
    // instance text/style overrides and gives the portable preview the same state.
    let mut variant_count = 0;
    for instance in instances {
        for &target in &variant_targets {
            if crate::layout_components::family_of(doc, instance)
                != crate::layout_components::family_of(doc, target)
            {
                continue;
            }
            variant_count += 1;
            if variant_count > 1024 {
                return Err("Prototype contains too many component variant combinations".into());
            }
            let mut variant_doc = doc.layout_snapshot();
            crate::layout_components::swap_variant(&mut variant_doc, layer, instance, target)?;
            let shapes = variant_doc.layers[layer]
                .kind
                .shapes()
                .ok_or("Missing variant layer")?;
            let mut variant = Html {
                shapes: shapes.iter().map(|s| (s.id, s)).collect(),
                children: HashMap::new(),
                fonts: html.fonts.clone(),
                styles: String::new(),
                interactions: serde_json::Map::new(),
                visited: HashSet::new(),
            };
            for shape in shapes {
                if let Some(parent) = shape.layout.parent {
                    variant.children.entry(parent).or_default().push(shape.id);
                }
            }
            let mut content = String::new();
            variant.node(instance, None, &mut content, 0)?;
            let _ = write!(
                body,
                "<template id=\"variant-{instance}-{target}\">{content}</template>"
            );
            html.fonts = variant.fonts;
            html.styles.push_str(&variant.styles);
        }
    }
    let actions = serde_json::Value::Object(html.interactions)
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let script = PROTOTYPE_RUNTIME
        .replace("__ROOT__", &frame_id.to_string())
        .replace("__ACTIONS__", &actions);
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{BASE_CSS}{}</style></head><body><main id=\"stage\" aria-label=\"Design prototype\"></main><dialog id=\"overlay\"></dialog>{body}<script>{script}</script></body></html>",
        escape(&frame.name),
        html.styles
    ))
}

const BASE_CSS: &str = "*{box-sizing:border-box}body{margin:0;background:#17191d;font-family:system-ui,sans-serif}#stage{display:flex;justify-content:center;min-height:100vh;width:100%;margin:0 auto;container-type:inline-size}.node{flex-shrink:0}.node>svg{display:block;overflow:visible}.interactive{cursor:pointer}.interactive:focus-visible{outline:2px solid #5b9fff!important;outline-offset:3px!important}#overlay{width:100vw;container-type:inline-size;padding:0;border:0;background:transparent;max-width:100vw;max-height:100vh}#overlay::backdrop{background:#0008}@media(prefers-reduced-motion:reduce){*{animation:none!important;transition:none!important}}";
const PROTOTYPE_RUNTIME: &str = r#"(()=>{
'use strict';
const actions=__ACTIONS__, stage=document.getElementById('stage'), overlay=document.getElementById('overlay'), history=[], hoverRestore=new WeakMap();
let current=__ROOT__;
const template=id=>document.getElementById('screen-'+id);
function mount(id,target,interaction){
 const source=template(id); if(!source)return false;
 target.replaceChildren(source.content.cloneNode(true));
 target.style.maxWidth=target.firstElementChild.style.maxWidth;
 const node=target.firstElementChild;
 if(node&&interaction&&!matchMedia('(prefers-reduced-motion: reduce)').matches){
  const kind=interaction.transition, duration=Math.min(3000,Math.max(0,interaction.duration_ms||0));
  if(kind!=='Instant'){
   const frames=kind==='Dissolve'?[{opacity:0},{opacity:1}]:[{transform:'translateX('+(kind==='SlideLeft'?'40px':'-40px')+')',opacity:0},{transform:'translateX(0)',opacity:1}];
   node.animate(frames,{duration,easing:'cubic-bezier(.2,.8,.2,1)'});
  }
 }
 return true;
}
function variant(node,id,temporary){
 node=node.closest('[data-component]')||node;
 const source=document.getElementById('variant-'+node.dataset.component+'-'+id)||template(id); if(!source)return;
 if(!temporary)hoverRestore.delete(node);
 if(temporary&&!hoverRestore.has(node))hoverRestore.set(node,{style:node.style.cssText,content:node.innerHTML,key:node.dataset.node,actions:node.dataset.actions});
 const next=source.content.firstElementChild.cloneNode(true);
 // Instance placement belongs to the layout, independent of the active variant.
 for(const property of ['position','left','right','top','bottom','width','height','minWidth','maxWidth','minHeight','maxHeight','flex','alignSelf'])next.style[property]=node.style[property];
 node.style.cssText=next.style.cssText;
 node.dataset.node=next.dataset.node;
 node.dataset.actions=next.dataset.actions;
 node.replaceChildren(...next.childNodes);
 // Component source IDs identify actions; DOM IDs remain unique per live instance.
 node.querySelectorAll('[id]').forEach(child=>child.removeAttribute('id'));
}
function act(node,interaction){
 const a=interaction.action;
 if(a.Navigate){if(mount(a.Navigate.target,stage,interaction)){history.push(current);current=a.Navigate.target;overlay.close();}}
 else if(a==='Back'){if(overlay.open){overlay.close();return;}const previous=history.pop();if(previous!==undefined&&mount(previous,stage,interaction))current=previous;}
 else if(a.OpenOverlay){if(mount(a.OpenOverlay.target,overlay,interaction)&&!overlay.open)overlay.showModal();}
 else if(a==='CloseOverlay')overlay.close();
 else if(a.SetVariant)variant(node,a.SetVariant.component,interaction.trigger==='Hover');
}
function dispatch(event,trigger){
 const node=event.target.closest('[data-node]');if(!node)return;
 for(let ancestor=node;ancestor&&ancestor!==document.body;ancestor=ancestor.parentElement){
  const handlers=(ancestor.dataset.actions?JSON.parse(ancestor.dataset.actions):(actions[ancestor.dataset.node]||[])).filter(i=>i.trigger===trigger);
  if(handlers.length){
   if(trigger==='Hover'&&event.relatedTarget&&ancestor.contains(event.relatedTarget))return;
   event.preventDefault();handlers.forEach(i=>act(ancestor,i));break;
  }
 }
}
document.addEventListener('click',e=>dispatch(e,'Click'));
document.addEventListener('pointerover',e=>dispatch(e,'Hover'));
document.addEventListener('pointerdown',e=>dispatch(e,'Press'));
document.addEventListener('pointerout',event=>{
 for(let node=event.target.closest('[data-node]');node;node=node.parentElement){
  const original=hoverRestore.get(node);
  if(original&&(!event.relatedTarget||!node.contains(event.relatedTarget))){node.style.cssText=original.style;node.innerHTML=original.content;node.dataset.node=original.key;node.dataset.actions=original.actions;hoverRestore.delete(node);}
 }
});
document.addEventListener('keydown',e=>{if((e.key==='Enter'||e.key===' ')&&e.target.matches('.interactive'))dispatch(e,'Click');});
overlay.addEventListener('click',e=>{if(e.target===overlay)overlay.close();});
mount(current,stage);
})();"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::document::{Layer, Style};
    fn rect(name: &str, origin: Pt, size: Pt, color: Rgba) -> Shape {
        let mut s = Shape::new(
            Geom::Rect {
                origin,
                size,
                radius: 0.,
            },
            Style {
                fill: Fill::Solid(color),
                stroke: None,
            },
        );
        s.name = name.into();
        s
    }
    fn scene() -> (Document, u64, u64) {
        let mut doc = Document::new("Export boundary", 100., 100., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::vector("Artwork")];
        let mut frame = rect(
            "Selected",
            Pt::new(-20., -10.),
            Pt::new(60., 40.),
            Rgba::rgb(255, 255, 255),
        );
        frame.layout = crate::layout::FrameLayout::frame();
        let mut child = rect(
            "Public child",
            Pt::new(-10., 0.),
            Pt::new(20., 20.),
            Rgba::rgb(0, 255, 0),
        );
        child.layout.parent = Some(frame.id);
        let private = rect(
            "PRIVATE SIBLING",
            Pt::new(-20., -10.),
            Pt::new(60., 40.),
            Rgba::rgb(255, 0, 0),
        );
        let ids = (frame.id, child.id);
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![frame, child, private];
        (doc, ids.0, ids.1)
    }
    #[test]
    fn frame_png_and_svg_exclude_overlapping_siblings_and_support_negative_origins() {
        let (doc, frame, child) = scene();
        let image = crate::compositor::export_frame_png(&doc, 0, frame, 2).unwrap();
        let pixels = tiny_skia::Pixmap::decode_png(&image).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (120, 80));
        assert_eq!(pixels.pixel(30, 30).unwrap().green(), 255);
        assert_eq!(pixels.pixel(30, 30).unwrap().red(), 0);
        assert_eq!(pixels.pixel(5, 5).unwrap().red(), 255);
        assert_eq!(pixels.pixel(5, 5).unwrap().green(), 255);
        let svg = crate::svg::export_frame(&doc, 0, frame).unwrap();
        assert!(!svg.contains("PRIVATE"));
        assert!(svg.contains(&format!("id=\"oma-{child}\"")));
        assert!(svg.contains(&format!("clip-path=\"url(#oma-frame-clip-{frame})\"")));
        assert_eq!(
            roxmltree::Document::parse(&svg)
                .unwrap()
                .root_element()
                .attribute("width"),
            Some("60")
        );
        assert_eq!(
            doc.find_shape(0, frame).unwrap().geom.bbox().min,
            Pt::new(-20., -10.)
        );
    }
    #[test]
    fn html_preserves_containment_layout_typography_and_escapes_user_content() {
        let (mut doc, frame, child) = scene();
        let root = doc.find_shape_mut(0, frame).unwrap();
        root.layout.stack = Some(crate::layout::AutoStack {
            direction: StackAxis::Horizontal,
            flow: StackFlow::Wrap,
            justify: StackJustify::SpaceBetween,
            ..Default::default()
        });
        let nested = doc.find_shape_mut(0, child).unwrap();
        nested.layout.frame = true;
        nested.layout.width = Sizing::Fill;
        let mut run = crate::geom::TypeRun {
            content: "<script>private()</script> & \"text\"".into(),
            origin: Pt::new(-8., 12.),
            px: 14.,
            tracking: 1.5,
            wrap_width: Some(40.),
            align: crate::geom::TextAlign::Center,
            ..Default::default()
        };
        run.contours = crate::text::shape(&run);
        let mut text = Shape::new(
            Geom::Text(run),
            Style {
                fill: Fill::Solid(Rgba::BLACK),
                stroke: None,
            },
        );
        text.layout.parent = Some(child);
        let tid = text.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(text);
        let html = export_html(&doc, 0, frame).unwrap();
        assert!(!html.contains("PRIVATE SIBLING") && !html.contains("<script>private()"));
        assert!(html.contains("&lt;script&gt;private()&lt;/script&gt;"));
        assert!(
            html.contains("flex-wrap:wrap")
                && html.contains("justify-content:space-between")
                && html.contains("flex:1 1 20.000px")
        );
        assert!(
            html.contains("font-size:14.000px")
                && html.contains("letter-spacing:1.500px")
                && html.contains("text-align:center")
        );
        let start = html
            .find(&format!("<template id=\"screen-{frame}\">"))
            .unwrap();
        let content = html[start..]
            .split_once('>')
            .unwrap()
            .1
            .split_once("</template>")
            .unwrap()
            .0;
        let parsed = roxmltree::Document::parse(content).unwrap();
        let node = parsed
            .descendants()
            .find(|n| n.attribute("id") == Some(format!("oma-{tid}").as_str()))
            .unwrap();
        assert_eq!(
            node.parent_element().unwrap().attribute("id"),
            Some(format!("oma-{child}").as_str())
        );
    }
    #[test]
    fn html_includes_only_reachable_prototype_screens() {
        let (mut doc, frame, _) = scene();
        let mut destination = rect(
            "Destination",
            Pt::new(110., 0.),
            Pt::new(60., 40.),
            Rgba::rgb(0, 0, 255),
        );
        destination.layout = crate::layout::FrameLayout::frame();
        let target = destination.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(destination);
        doc.find_shape_mut(0, frame)
            .unwrap()
            .layout
            .interactions
            .push(crate::layout_prototype::Interaction {
                trigger: crate::layout_prototype::Trigger::Click,
                action: crate::layout_prototype::PrototypeAction::Navigate { target },
                transition: crate::layout_prototype::Transition::Dissolve,
                duration_ms: 200,
            });
        let html = export_html(&doc, 0, frame).unwrap();
        assert!(html.contains(&format!("id=\"screen-{target}\"")));
        assert!(html.contains("\"Navigate\":{\"target\":"));
        assert!(!html.contains("PRIVATE SIBLING"));
        assert_eq!(html.matches("<template ").count(), 2);
    }
}
