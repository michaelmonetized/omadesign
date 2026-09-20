use super::*;

pub(super) fn stack_css(stack: &crate::layout::AutoStack) -> String {
    let align = match stack.align {
        StackAlign::Start => "flex-start",
        StackAlign::Center => "center",
        StackAlign::End => "flex-end",
        StackAlign::Stretch => "stretch",
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
            "display:grid;grid-template-columns:repeat({},minmax(0,1fr));column-gap:{:.3}px;row-gap:{:.3}px;",
            stack.columns.max(1),
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

pub(super) fn breakpoints(shape: &Shape) -> String {
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
            let _ = write!(
                text_css,
                "[data-node=\"{}\"]>span{{font-size:{px:.3}px!important;line-height:{leading:.3}px!important;}}",
                shape.id
            );
        }
        let _ = write!(
            output,
            "@container(max-width:{width:.3}px){{[data-node=\"{}\"]{{{}}}{text_css}}}",
            shape.id,
            css.replace(';', "!important;")
        );
    }
    output
}
