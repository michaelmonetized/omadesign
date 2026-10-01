use crate::ui::theme;
use eframe::egui::{
    self, Color32, TextureHandle, Ui,
};
const SPARKLE: &[u8] = include_bytes!("../../../assets/phosphor/welcome/sparkle.svg");

fn source_with_gradient(source: &[u8], from: Color32, to: Color32) -> Option<String> {
    let source = std::str::from_utf8(source).ok()?;
    let color = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
    let root = source.find("<svg")?;
    let start = root + source[root..].find('>')? + 1;
    let defs = format!(
        "<defs><linearGradient id=\"omadesign-ai\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"{}\"/><stop offset=\"100%\" stop-color=\"{}\"/></linearGradient></defs>",
        color(from),
        color(to)
    );
    let mut svg = source.to_owned();
    svg.insert_str(start, &defs);
    Some(svg.replace("currentColor", "url(#omadesign-ai)"))
}

/// Cache by source identity and both stops, so theme changes invalidate the tint.
pub fn gradient_svg_icon(
    ui: &Ui,
    key: &'static str,
    source: &[u8],
    from: Color32,
    to: Color32,
) -> Option<TextureHandle> {
    let id = egui::Id::new(("gradient-svg", key, from, to));
    if let Some(texture) = ui.ctx().data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let svg = source_with_gradient(source, from, to)?;
    super::svg_texture_id(ui, key, id, svg.as_bytes())
}

pub fn sparkle_texture(ui: &Ui) -> Option<TextureHandle> {
    let [from, to] = theme::agent_gradient(theme::p().dark);
    gradient_svg_icon(ui, "agent-sparkle", SPARKLE, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_sparkle_has_alpha_and_distinct_colors_in_both_themes() {
        for dark in [false, true] {
            let [from, to] = theme::agent_gradient(dark);
            let source = source_with_gradient(SPARKLE, from, to).unwrap();
            let tree = usvg::Tree::from_data(source.as_bytes(), &usvg::Options::default()).unwrap();
            let mut pixmap = tiny_skia::Pixmap::new(256, 256).unwrap();
            resvg::render(
                &tree,
                tiny_skia::Transform::identity(),
                &mut pixmap.as_mut(),
            );
            let opaque: Vec<_> = pixmap.pixels().iter().filter(|p| p.alpha() > 240).collect();
            assert!(opaque.len() > 100);
            assert!(
                opaque.iter().map(|p| p.red()).max().unwrap()
                    - opaque.iter().map(|p| p.red()).min().unwrap()
                    > 10
            );
            assert!(pixmap.pixels().iter().any(|p| p.alpha() == 0));
        }
    }
    #[test]
    fn repeated_gradient_requests_share_a_texture_and_color_changes_do_not() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let [a, b] = theme::agent_gradient(true);
            let first = gradient_svg_icon(ui, "cache-proof", SPARKLE, a, b).unwrap();
            let next = gradient_svg_icon(ui, "cache-proof", SPARKLE, a, b).unwrap();
            assert_eq!(first.id(), next.id());
            let [a, b] = theme::agent_gradient(false);
            let light = gradient_svg_icon(ui, "cache-proof", SPARKLE, a, b).unwrap();
            assert_ne!(first.id(), light.id());
        });
        output.textures_delta.clear();
    }
}
