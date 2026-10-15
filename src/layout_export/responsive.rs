use super::*;

pub(super) fn stack_css(stack: &crate::layout::AutoStack) -> String {
    let align = match stack.align {
        StackAlign::Start => "flex-start",
        StackAlign::Center => "center",
        StackAlign::End => "flex-end",
        StackAlign::Stretch => "stretch",
        StackAlign::Baseline
            if stack.flow != StackFlow::Grid && stack.direction == StackAxis::Vertical =>
        {
            "flex-start"
        }
        StackAlign::Baseline => "baseline",
    };
    let justify = match stack.justify {
        StackJustify::Start => "flex-start",
        StackJustify::Center => "center",
        StackJustify::End => "flex-end",
        StackJustify::SpaceBetween => "space-between",
        StackJustify::SpaceAround => "space-around",
        StackJustify::SpaceEvenly => "space-evenly",
    };
    let mut css = format!(
        "padding:{:.3}px {:.3}px {:.3}px {:.3}px;align-items:{align};justify-content:{justify};",
        stack.padding[0], stack.padding[1], stack.padding[2], stack.padding[3]
    );
    if stack.flow == StackFlow::Grid {
        let _ = write!(
            css,
            "display:grid;grid-auto-columns:minmax(0,1fr);grid-auto-rows:max-content;grid-template-columns:{};grid-template-rows:{};column-gap:{:.3}px;row-gap:{:.3}px;",
            if stack.column_tracks.is_empty() {
                format!("repeat({},minmax(0,1fr))", stack.columns.max(1))
            } else {
                tracks_css(&stack.column_tracks)
            },
            if stack.row_tracks.is_empty() {
                "none".into()
            } else {
                tracks_css(&stack.row_tracks)
            },
            stack.gap,
            stack.cross_gap
        );
    } else {
        let horizontal = stack.direction == StackAxis::Horizontal;
        let _ = write!(
            css,
            "display:flex;flex-direction:{};flex-wrap:{};gap:{:.3}px {:.3}px;",
            if horizontal { "row" } else { "column" },
            if stack.flow == StackFlow::Wrap {
                "wrap"
            } else {
                "nowrap"
            },
            if horizontal {
                stack.cross_gap
            } else {
                stack.gap
            },
            if horizontal {
                stack.gap
            } else {
                stack.cross_gap
            }
        );
    }
    css
}

pub(super) fn breakpoints(shape: &Shape, scope: &str) -> String {
    let mut points: Vec<_> = shape
        .layout
        .breakpoints
        .iter()
        .map(|b| b.max_width)
        .collect();
    points.sort_by(|a, b| b.total_cmp(a));
    points.dedup();
    let mut output = String::new();
    for width in points {
        let resolved = crate::layout::resolve_for_width(&shape.layout, width);
        let mut css = String::new();
        if let Some(stack) = &resolved.stack {
            css.push_str(&stack_css(stack));
        }
        let mut text_css = String::new();
        if let (Geom::Text(run), Some(px)) = (&shape.geom, resolved.text_size) {
            let leading = run.line_height() / run.px.max(1.) * px;
            let ratio = px / run.px.max(1.0);
            let metrics = crate::text::font_metrics_for(run);
            let top = metrics
                .map(|(asc, desc, _)| {
                    run.origin.y
                        - shape.geom.bbox().min.y
                        - (run.line_height() - asc - desc) * 0.5
                        - asc
                })
                .unwrap_or(0.0)
                * ratio;
            if run.wrap_width.is_none() {
                let size = text_box(shape);
                let _ = write!(css, "--text-scale:{ratio:.9};");
                if shape.layout.width == Sizing::Hug || shape.layout.width == Sizing::Fixed {
                    let _ = write!(css, "width:{:.6}px;", size.x * ratio);
                }
                if shape.layout.height != Sizing::Fill {
                    let _ = write!(css, "height:{:.6}px;", size.y * ratio);
                }
            } else {
                let _ = write!(
                    text_css,
                    "[data-state=\"{scope}\"][data-node=\"{}\"]>span{{font-size:{px:.3}px!important;line-height:{leading:.3}px!important;letter-spacing:{:.3}px!important;top:{top:.3}px!important;{} }}",
                    shape.id,
                    run.tracking * ratio,
                    if shape.layout.width == Sizing::Hug {
                        "width:max-content!important;"
                    } else {
                        ""
                    }
                );
            }
        }
        let _ = write!(
            output,
            "@container(max-width:{width:.3}px){{[data-state=\"{scope}\"][data-node=\"{}\"]{{{}}}{text_css}}}",
            shape.id,
            css.replace(';', "!important;")
        );
    }
    output
}

/// Translate native grid tracks to CSS without losing authored limits.
/// Args: ordered native tracks. Returns the equivalent CSS track list.
fn tracks_css(tracks: &[crate::layout::GridTrack]) -> String {
    use crate::layout::GridTrackSize;
    tracks
        .iter()
        .map(|track| {
            let size = match track.size {
                GridTrackSize::Fixed(v) => format!("{v:.6}px"),
                GridTrackSize::Hug => "max-content".into(),
                GridTrackSize::Fraction(v) => format!("{v:.6}fr"),
            };
            let min = track.min.unwrap_or(0.0);
            match track.size {
                GridTrackSize::Fixed(v) => format!(
                    "{:.6}px",
                    v.max(min).min(track.max.unwrap_or(f32::MAX).max(min))
                ),
                GridTrackSize::Hug => format!("minmax({min:.6}px,max-content)"),
                GridTrackSize::Fraction(_) => format!("minmax({min:.6}px,{size})"),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
