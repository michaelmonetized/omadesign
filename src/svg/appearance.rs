use super::*;
use crate::{color::Blend, filter::FilterStack};

/// Paint siblings, so effect mix-blend-mode resolves against the artwork beneath it.
/// `<use>` preserves a single editable vector definition and avoids BackgroundImage.
pub(super) fn wrap(
    defs: &mut String,
    id: &str,
    content: &str,
    bounds: Bounds,
    stack: &FilterStack,
    blend: Blend,
    opacity: f32,
    fill: f32,
    interior: bool,
) -> String {
    let r = bounds.inflate(crate::filter::svg_pad(stack).max(2.));
    let region = [r.min.x, r.min.y, r.width().max(1.), r.height().max(1.)];
    let pixel_stack = FilterStack {
        enabled: stack.enabled,
        legacy_composite: false,
        items: stack
            .items
            .iter()
            .copied()
            .filter(|fx| fx.appearance().is_none())
            .collect(),
    };
    let pixel_id = format!("{id}-pixels");
    let pixel_attr = if let Some(xml) = crate::filter::svg_filter(&pixel_id, &pixel_stack, region) {
        defs.push_str(&xml);
        format!(" filter=\"url(#{pixel_id})\"")
    } else {
        String::new()
    };
    defs.push_str(&format!(
        "<g id=\"{id}-source\"{pixel_attr}>{content}</g>\n"
    ));
    let mut outer = String::new();
    let mut inner = String::new();
    if stack.active() {
        for (i, fx) in stack.items.iter().enumerate() {
            let Some((mode, alpha)) = fx.appearance() else {
                continue;
            };
            let filter_id = format!("{id}-effect-{i}");
            let primitives = crate::filter::svg_effect_primitives(fx, "SourceGraphic", "effect");
            defs.push_str(&format!("<filter id=\"{filter_id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\">{primitives}</filter>\n",region[0],region[1],region[2],region[3]));
            let item = format!(
                "<use href=\"#{id}-source\" filter=\"url(#{filter_id})\" opacity=\"{alpha}\" style=\"mix-blend-mode:{}\"/>\n",
                mode.css()
            );
            if fx.outer() {
                outer.push_str(&item);
            } else {
                inner.push_str(&item);
            }
        }
    }
    let fill_content = format!(
        "<use href=\"#{id}-source\" opacity=\"{fill}\"{}/>",
        if interior {
            String::new()
        } else {
            format!(" style=\"mix-blend-mode:{}\"", blend.css())
        }
    );
    let middle = if interior {
        format!(
            "<g style=\"isolation:isolate;mix-blend-mode:{}\">{fill_content}{inner}</g>",
            blend.css()
        )
    } else {
        format!("{fill_content}{inner}")
    };
    format!("<g id=\"{id}-appearance\" opacity=\"{opacity}\">{outer}{middle}</g>")
}
