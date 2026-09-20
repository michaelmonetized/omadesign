//! A measured tree followed by a parent-first arrangement pass. The index is
//! built once per layer, so nested layouts do not repeatedly scan the document.
use super::*;
use std::collections::{HashMap, HashSet};

const MAX_DEPTH: usize = 256;

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}
fn positive(value: f32) -> f32 {
    finite(value, 1.0).clamp(1.0, 1_000_000.0)
}
fn gap(value: f32) -> f32 {
    finite(value, 0.0).clamp(0.0, 1_000_000.0)
}
fn clamp(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let lo = min
        .filter(|v| v.is_finite())
        .unwrap_or(1.0)
        .clamp(1.0, 1_000_000.0);
    let hi = max
        .filter(|v| v.is_finite())
        .unwrap_or(1_000_000.0)
        .clamp(lo, 1_000_000.0);
    positive(value).clamp(lo, hi)
}
fn main(v: Pt, horizontal: bool) -> f32 {
    if horizontal { v.x } else { v.y }
}
fn cross(v: Pt, horizontal: bool) -> f32 {
    if horizontal { v.y } else { v.x }
}
fn axes(main: f32, cross: f32, horizontal: bool) -> Pt {
    if horizontal {
        Pt::new(main, cross)
    } else {
        Pt::new(cross, main)
    }
}

fn size_text(run: &mut crate::geom::TypeRun, size: f32) -> bool {
    let size = if size.is_finite() {
        size.clamp(1.0, 100_000.0)
    } else {
        run.px
    };
    if (run.px - size).abs() < 0.001 {
        return false;
    }
    let ratio = size / run.px.max(1.0);
    if run.leading > 0.0 {
        run.leading *= ratio;
    }
    run.tracking *= ratio;
    run.px = size;
    true
}

#[derive(Clone)]
struct Node {
    arranged: bool,
    original: Bounds,
    bounds: Bounds,
    measured: Pt,
    layout: FrameLayout,
    visible: bool,
    text: Option<crate::geom::TypeRun>,
    text_measurement: Option<(Option<f32>, f32, Pt)>,
    parent: Option<usize>,
    children: Vec<usize>,
}
impl Node {
    fn clamp_size(&self, size: Pt) -> Pt {
        let width = clamp(size.x, self.layout.min_width, self.layout.max_width);
        let height = if let Some(ratio) = self
            .layout
            .aspect_ratio
            .filter(|r| r.is_finite() && *r > 0.0)
        {
            width / ratio
        } else {
            size.y
        };
        Pt::new(
            width,
            clamp(height, self.layout.min_height, self.layout.max_height),
        )
    }
    fn sizing(&self, horizontal: bool) -> Sizing {
        if horizontal {
            self.layout.width
        } else {
            self.layout.height
        }
    }
    fn clamp_axis(&self, size: f32, horizontal: bool) -> f32 {
        if horizontal {
            clamp(size, self.layout.min_width, self.layout.max_width)
        } else {
            clamp(size, self.layout.min_height, self.layout.max_height)
        }
    }
}

struct Tree {
    nodes: Vec<Node>,
    index: HashMap<u64, usize>,
}
impl Tree {
    fn new(doc: &Document, layer: usize) -> Self {
        let shapes = doc
            .layers
            .get(layer)
            .and_then(|l| l.kind.shapes())
            .unwrap_or(&[]);
        let index: HashMap<_, _> = shapes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut nodes: Vec<_> = shapes
            .iter()
            .map(|s| {
                let bounds = s.geom.bbox();
                Node {
                    arranged: false,
                    original: bounds,
                    bounds,
                    measured: bounds.size(),
                    // Components can contain hundreds of baseline shapes.
                    // Solving geometry never needs to clone those snapshots.
                    layout: FrameLayout {
                        frame: s.layout.frame,
                        parent: s.layout.parent,
                        stack: s.layout.stack.clone(),
                        constraint_x: s.layout.constraint_x,
                        constraint_y: s.layout.constraint_y,
                        placeholder: s.layout.placeholder,
                        width: s.layout.width,
                        height: s.layout.height,
                        min_width: s.layout.min_width,
                        max_width: s.layout.max_width,
                        min_height: s.layout.min_height,
                        max_height: s.layout.max_height,
                        absolute: s.layout.absolute,
                        clip: s.layout.clip,
                        text_size: s.layout.text_size,
                        aspect_ratio: s.layout.aspect_ratio,
                        breakpoints: s.layout.breakpoints.clone(),
                        ..FrameLayout::default()
                    },
                    visible: s.visible,
                    text: if let Geom::Text(run) = &s.geom {
                        Some(crate::geom::TypeRun {
                            origin: run.origin,
                            content: run.content.clone(),
                            px: run.px,
                            tracking: run.tracking,
                            leading: run.leading,
                            font: run.font.clone(),
                            kern: run.kern,
                            liga: run.liga,
                            tnum: run.tnum,
                            smcp: run.smcp,
                            wrap_width: run.wrap_width,
                            align: run.align,
                            contours: vec![],
                        })
                    } else {
                        None
                    },
                    text_measurement: None,
                    parent: None,
                    children: vec![],
                }
            })
            .collect();
        for i in 0..nodes.len() {
            if let Some(parent) = nodes[i]
                .layout
                .parent
                .and_then(|id| index.get(&id).copied())
                && parent != i
                && nodes[parent].layout.frame
            {
                nodes[i].parent = Some(parent);
                nodes[parent].children.push(i);
            }
        }
        // Resolve each viewport root once, including layers stored out of tree
        // order. Responsive rules use the page width, not a nested card width.
        let mut roots = vec![None; nodes.len()];
        for i in 0..nodes.len() {
            let mut current = i;
            let mut path = Vec::new();
            let mut seen = HashSet::new();
            let root;
            loop {
                if let Some(known) = roots[current] {
                    root = known;
                    break;
                }
                if !seen.insert(current) {
                    root = current;
                    break;
                }
                path.push(current);
                let Some(parent) = nodes[current].parent else {
                    root = current;
                    break;
                };
                current = parent;
            }
            for id in path {
                roots[id] = Some(root);
            }
        }
        for (i, root) in roots.into_iter().enumerate() {
            let viewport = nodes[root.unwrap_or(i)].bounds.width();
            apply_breakpoints(&mut nodes[i].layout, viewport);
            nodes[i].layout.breakpoints.clear();
        }
        Self { nodes, index }
    }
    fn flow_children(&self, id: usize) -> Vec<usize> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&i| self.nodes[i].visible && !self.nodes[i].layout.absolute)
            .collect()
    }
    fn padding(stack: &AutoStack) -> [f32; 4] {
        stack.padding.map(gap)
    }
    fn measure(&mut self, id: usize, visited: &mut HashSet<usize>, depth: usize) {
        if depth > MAX_DEPTH || !visited.insert(id) {
            return;
        }
        for child in self.nodes[id].children.clone() {
            self.measure(child, visited, depth + 1);
        }
        let node = &self.nodes[id];
        let mut size = node.bounds.size();
        let mut measured_text = None;
        if let Some(run) = &node.text
            && (node.layout.width == Sizing::Hug
                || node.layout.height == Sizing::Hug
                || node.layout.text_size.is_some())
        {
            let mut run = run.clone();
            if let Some(size) = node.layout.text_size {
                size_text(&mut run, size);
            }
            run.wrap_width = if node.layout.width == Sizing::Hug {
                None
            } else if run.wrap_width.is_some()
                || (node.bounds.width() - node.original.width()).abs() > 0.01
            {
                Some(size.x.max(1.0))
            } else {
                None
            };
            let measured = if let Some((wrap, px, metrics)) = node.text_measurement
                && wrap == run.wrap_width
                && px == run.px
            {
                metrics
            } else {
                let (width, height) = crate::text::measure(&run);
                let metrics = Pt::new(width, height);
                measured_text = Some((run.wrap_width, run.px, metrics));
                metrics
            };
            let (width, height) = (measured.x, measured.y);
            if node.layout.width == Sizing::Hug
                || (node.layout.text_size.is_some() && run.wrap_width.is_none())
            {
                size.x = width;
            }
            if node.layout.height == Sizing::Hug || node.layout.text_size.is_some() {
                size.y = height;
            }
        }
        if let Some(stack) = node.layout.stack.as_ref().filter(|_| node.layout.frame) {
            let kids = self.flow_children(id);
            let pad = Self::padding(stack);
            let mut content = Pt::new(0.0, 0.0);
            if stack.flow == StackFlow::Grid {
                let columns = (stack.columns as usize).clamp(1, 1024);
                let mut widths = vec![0.0f32; columns];
                let mut heights = vec![0.0f32; kids.len().div_ceil(columns)];
                for (n, &child) in kids.iter().enumerate() {
                    widths[n % columns] = widths[n % columns].max(self.nodes[child].measured.x);
                    heights[n / columns] = heights[n / columns].max(self.nodes[child].measured.y);
                }
                content.x = widths.iter().copied().fold(0.0f32, f32::max) * columns as f32
                    + gap(stack.gap) * columns.saturating_sub(1) as f32;
                content.y = heights.iter().sum::<f32>()
                    + gap(stack.cross_gap) * heights.len().saturating_sub(1) as f32;
            } else {
                let horizontal = stack.direction == StackAxis::Horizontal;
                let inset = if horizontal {
                    pad[1] + pad[3]
                } else {
                    pad[0] + pad[2]
                };
                let available = if node.sizing(horizontal) == Sizing::Hug {
                    f32::MAX
                } else {
                    (main(size, horizontal) - inset).max(1.0)
                };
                let lines = self.lines(&kids, stack, available, horizontal);
                let mut m = 0.0f32;
                let mut c = 0.0f32;
                for line in &lines {
                    let lm = line
                        .iter()
                        .map(|&i| main(self.nodes[i].measured, horizontal))
                        .sum::<f32>()
                        + gap(stack.gap) * line.len().saturating_sub(1) as f32;
                    let lc = line
                        .iter()
                        .map(|&i| cross(self.nodes[i].measured, horizontal))
                        .fold(0.0f32, f32::max);
                    m = m.max(lm);
                    c += lc;
                }
                c += gap(stack.cross_gap) * lines.len().saturating_sub(1) as f32;
                content = axes(m, c, horizontal);
            }
            if node.layout.width == Sizing::Hug {
                size.x = content.x + pad[1] + pad[3];
            }
            if node.layout.height == Sizing::Hug {
                size.y = content.y + pad[0] + pad[2];
            }
        } else if node.layout.frame && !node.children.is_empty() {
            // Free-layout hug preserves the frame origin and encloses the
            // furthest visible non-absolute descendant's outer edge.
            if node.layout.width == Sizing::Hug {
                size.x = 1.0;
            }
            if node.layout.height == Sizing::Hug {
                size.y = 1.0;
            }
            for child in self.flow_children(id) {
                let child = &self.nodes[child];
                if node.layout.width == Sizing::Hug {
                    size.x = size
                        .x
                        .max(child.original.min.x - node.original.min.x + child.measured.x);
                }
                if node.layout.height == Sizing::Hug {
                    size.y = size
                        .y
                        .max(child.original.min.y - node.original.min.y + child.measured.y);
                }
            }
        }
        self.nodes[id].measured = self.nodes[id].clamp_size(size);
        if measured_text.is_some() {
            self.nodes[id].text_measurement = measured_text;
        }
    }
    fn lines(
        &self,
        kids: &[usize],
        stack: &AutoStack,
        available: f32,
        horizontal: bool,
    ) -> Vec<Vec<usize>> {
        if kids.is_empty() {
            return vec![];
        }
        if stack.flow != StackFlow::Wrap {
            return vec![kids.to_vec()];
        }
        let mut lines = vec![];
        let mut line = vec![];
        let mut used = 0.0;
        for &id in kids {
            let size = main(self.nodes[id].measured, horizontal);
            let next = if line.is_empty() {
                size
            } else {
                used + gap(stack.gap) + size
            };
            if !line.is_empty() && next > available + 0.001 {
                lines.push(std::mem::take(&mut line));
                used = size;
            } else {
                used = next;
            }
            line.push(id);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
    fn distribute_fill(
        &self,
        kids: &[usize],
        sizes: &mut [Pt],
        available: f32,
        horizontal: bool,
        parent_hug: bool,
    ) {
        if parent_hug {
            return;
        }
        let mut active: Vec<usize> = kids
            .iter()
            .enumerate()
            .filter_map(|(n, &id)| (self.nodes[id].sizing(horizontal) == Sizing::Fill).then_some(n))
            .collect();
        let fill_set: HashSet<_> = active.iter().copied().collect();
        let mut remaining = available
            - sizes
                .iter()
                .enumerate()
                .filter(|(n, _)| !fill_set.contains(n))
                .map(|(_, &s)| main(s, horizontal))
                .sum::<f32>();
        // Freeze min/max-constrained items and redistribute remaining space.
        // This is bounded by the number of children and never produces NaN.
        while !active.is_empty() {
            let share = (remaining / active.len() as f32).max(1.0);
            let clamped: Vec<_> = active
                .iter()
                .map(|&n| (n, self.nodes[kids[n]].clamp_axis(share, horizontal)))
                .collect();
            let total: f32 = clamped.iter().map(|&(_, size)| size).sum();
            let violation = total - remaining;
            // A minimum can make a previously max-clamped sibling smaller.
            // Freeze only violations matching the total excess/deficit, then
            // recalculate the others (the flexbox min/max freezing rule).
            let frozen: Vec<_> = clamped
                .iter()
                .copied()
                .filter(|&(_, actual)| {
                    if violation > 0.001 {
                        actual > share + 0.001
                    } else if violation < -0.001 {
                        actual < share - 0.001
                    } else {
                        (actual - share).abs() > 0.001
                    }
                })
                .collect();
            if frozen.is_empty() {
                for (n, actual) in clamped {
                    sizes[n] = axes(actual, cross(sizes[n], horizontal), horizontal);
                }
                break;
            }
            for (n, actual) in frozen {
                sizes[n] = axes(actual, cross(sizes[n], horizontal), horizontal);
                remaining -= actual;
                active.retain(|&index| index != n);
            }
        }
    }
    fn justify(justify: StackJustify, free: f32, count: usize, base_gap: f32) -> (f32, f32) {
        let free = free.max(0.0);
        match justify {
            StackJustify::Start => (0.0, base_gap),
            StackJustify::Center => (free * 0.5, base_gap),
            StackJustify::End => (free, base_gap),
            StackJustify::SpaceBetween if count > 1 => (0.0, base_gap + free / (count - 1) as f32),
            StackJustify::SpaceAround if count > 0 => {
                (free / count as f32 * 0.5, base_gap + free / count as f32)
            }
            StackJustify::SpaceEvenly => (
                free / (count + 1) as f32,
                base_gap + free / (count + 1) as f32,
            ),
            _ => (0.0, base_gap),
        }
    }
    fn arrangements(&self, id: usize, target: Bounds, stack: &AutoStack) -> Vec<(usize, Bounds)> {
        let kids = self.flow_children(id);
        let pad = Self::padding(stack);
        let origin = target.min + Pt::new(pad[3], pad[0]);
        let inner = Pt::new(
            (target.width() - pad[1] - pad[3]).max(1.0),
            (target.height() - pad[0] - pad[2]).max(1.0),
        );
        if stack.flow == StackFlow::Grid {
            return self.grid(&kids, stack, origin, inner, id);
        }
        let horizontal = stack.direction == StackAxis::Horizontal;
        let available_main = main(inner, horizontal);
        let lines = self.lines(&kids, stack, available_main, horizontal);
        let mut cursor_cross = 0.0;
        let mut placed = Vec::with_capacity(kids.len());
        for line in &lines {
            let mut sizes: Vec<_> = line.iter().map(|&i| self.nodes[i].measured).collect();
            let gaps = gap(stack.gap) * line.len().saturating_sub(1) as f32;
            self.distribute_fill(
                line,
                &mut sizes,
                available_main - gaps,
                horizontal,
                self.nodes[id].sizing(horizontal) == Sizing::Hug,
            );
            for (n, &child) in line.iter().enumerate() {
                sizes[n] = self.nodes[child].clamp_size(sizes[n]);
            }
            let line_cross = if stack.flow == StackFlow::Stack {
                cross(inner, horizontal)
            } else {
                sizes
                    .iter()
                    .map(|&s| cross(s, horizontal))
                    .fold(1.0f32, f32::max)
            };
            let occupied = sizes.iter().map(|&s| main(s, horizontal)).sum::<f32>() + gaps;
            let (mut cursor_main, spacing) = Self::justify(
                stack.justify,
                available_main - occupied,
                line.len(),
                gap(stack.gap),
            );
            for (n, &child) in line.iter().enumerate() {
                let node = &self.nodes[child];
                let mut size = sizes[n];
                if node.sizing(!horizontal) == Sizing::Fill || stack.align == StackAlign::Stretch {
                    size = axes(
                        main(size, horizontal),
                        node.clamp_axis(line_cross, !horizontal),
                        horizontal,
                    );
                }
                size = node.clamp_size(size);
                let offset = match stack.align {
                    StackAlign::Start | StackAlign::Stretch => 0.0,
                    StackAlign::Center => (line_cross - cross(size, horizontal)) * 0.5,
                    StackAlign::End => line_cross - cross(size, horizontal),
                };
                placed.push((
                    child,
                    Bounds::from_min_size(
                        origin + axes(cursor_main, cursor_cross + offset, horizontal),
                        size,
                    ),
                ));
                cursor_main += main(size, horizontal) + spacing;
            }
            cursor_cross += line_cross + gap(stack.cross_gap);
        }
        placed
    }
    fn grid(
        &self,
        kids: &[usize],
        stack: &AutoStack,
        origin: Pt,
        inner: Pt,
        id: usize,
    ) -> Vec<(usize, Bounds)> {
        let columns = (stack.columns as usize).clamp(1, 1024);
        let rows = kids.len().div_ceil(columns);
        let col_width = ((inner.x - gap(stack.gap) * columns.saturating_sub(1) as f32)
            / columns as f32)
            .max(1.0);
        let mut row_heights = vec![1.0f32; rows];
        for (n, &child) in kids.iter().enumerate() {
            row_heights[n / columns] = row_heights[n / columns].max(self.nodes[child].measured.y);
        }
        let mut y = origin.y;
        let mut out = Vec::with_capacity(kids.len());
        // A vertical Fill item makes its row participate in available height.
        let fill_rows: HashSet<_> = kids
            .iter()
            .enumerate()
            .filter_map(|(n, &child)| {
                (self.nodes[child].layout.height == Sizing::Fill).then_some(n / columns)
            })
            .collect();
        if !fill_rows.is_empty() && self.nodes[id].layout.height != Sizing::Hug {
            let fixed: f32 = row_heights
                .iter()
                .enumerate()
                .filter(|(r, _)| !fill_rows.contains(r))
                .map(|(_, &h)| h)
                .sum();
            let h = ((inner.y - fixed - gap(stack.cross_gap) * rows.saturating_sub(1) as f32)
                / fill_rows.len() as f32)
                .max(1.0);
            for &r in &fill_rows {
                row_heights[r] = h;
            }
        }
        for (row, &height) in row_heights.iter().enumerate() {
            for col in 0..columns {
                let Some(&child) = kids.get(row * columns + col) else {
                    break;
                };
                let node = &self.nodes[child];
                let mut size = node.measured;
                if node.layout.width == Sizing::Fill || stack.align == StackAlign::Stretch {
                    size.x = col_width;
                }
                if node.layout.height == Sizing::Fill || stack.align == StackAlign::Stretch {
                    size.y = height;
                }
                size = node.clamp_size(size);
                let align = match stack.align {
                    StackAlign::Center => 0.5,
                    StackAlign::End => 1.0,
                    _ => 0.0,
                };
                let p = Pt::new(
                    origin.x
                        + col as f32 * (col_width + gap(stack.gap))
                        + (col_width - size.x) * align,
                    y + (height - size.y) * align,
                );
                out.push((child, Bounds::from_min_size(p, size)));
            }
            y += height + gap(stack.cross_gap);
        }
        out
    }
    fn arrange(&mut self, id: usize, target: Bounds, seen: &mut HashSet<usize>, depth: usize) {
        if depth > MAX_DEPTH || !seen.insert(id) {
            return;
        }
        let target = Bounds::from_min_size(target.min, self.nodes[id].clamp_size(target.size()));
        self.nodes[id].arranged = true;
        self.nodes[id].bounds = target;
        let layout = self.nodes[id].layout.clone();
        let mut pending = if let Some(stack) = &layout.stack {
            self.arrangements(id, target, stack)
        } else {
            vec![]
        };
        let in_flow: HashSet<_> = pending.iter().map(|&(i, _)| i).collect();
        for child in self.nodes[id].children.clone() {
            if in_flow.contains(&child) {
                continue;
            }
            let child_node = &self.nodes[child];
            let mut bounds = constrain_bounds(
                child_node.original,
                self.nodes[id].original,
                target,
                child_node.layout.constraint_x,
                child_node.layout.constraint_y,
            );
            let mut size = bounds.size();
            if child_node.layout.width == Sizing::Hug {
                size.x = child_node.measured.x;
            }
            if child_node.layout.height == Sizing::Hug {
                size.y = child_node.measured.y;
            }
            size = child_node.clamp_size(size);
            bounds.max = bounds.min + size;
            pending.push((child, bounds));
        }
        // Parent placement is final before child layout, so nested content
        // follows frame translations and recomputes against the final size.
        for (child, bounds) in pending {
            self.arrange(child, bounds, seen, depth + 1);
        }
    }
    fn solve(&mut self, roots: &[usize], preserve_root: bool) {
        let root_bounds: Vec<_> = roots.iter().map(|&i| self.nodes[i].bounds).collect();
        // Parent Fill sizing can change a nested wrapping frame's intrinsic
        // height. Re-measure final boxes until stable; typical trees settle in
        // two passes. The bound also makes circular Hug/Fill dependencies safe.
        for _ in 0..8 {
            let before: Vec<_> = self.nodes.iter().map(|n| n.bounds).collect();
            let mut measured = HashSet::new();
            for &root in roots {
                self.measure(root, &mut measured, 0);
            }
            let mut arranged = HashSet::new();
            for (n, &root) in roots.iter().enumerate() {
                let size = if preserve_root {
                    root_bounds[n].size()
                } else {
                    self.nodes[root].measured
                };
                let target = Bounds::from_min_size(self.nodes[root].bounds.min, size);
                self.arrange(root, target, &mut arranged, 0);
            }
            if self.nodes.iter().zip(&before).all(|(n, b)| {
                (n.bounds.min - b.min).length_sq() < 0.0001
                    && (n.bounds.max - b.max).length_sq() < 0.0001
            }) {
                break;
            }
        }
    }

    fn apply(self, doc: &mut Document, layer: usize) {
        let Some(shapes) = doc.layers.get_mut(layer).and_then(|l| l.kind.shapes_mut()) else {
            return;
        };
        for (shape, node) in shapes.iter_mut().zip(self.nodes) {
            if !node.arranged {
                continue;
            }
            if let Geom::Text(run) = &mut shape.geom {
                if shape.layout.text_size.is_none()
                    && shape
                        .layout
                        .breakpoints
                        .iter()
                        .any(|b| b.text_size.is_some())
                {
                    shape.layout.text_size = Some(run.px);
                }
                let changed_size = node
                    .layout
                    .text_size
                    .is_some_and(|size| size_text(run, size));
                let width = if node.layout.width == Sizing::Hug {
                    None
                } else if run.wrap_width.is_some()
                    || (node.bounds.width() - node.original.width()).abs() > 0.01
                {
                    Some(node.bounds.width().max(1.0))
                } else {
                    None
                };
                if run.wrap_width != width || changed_size {
                    run.wrap_width = width;
                    run.contours = crate::text::shape(run);
                }
                if width.is_none() {
                    let before = shape.geom.bbox();
                    shape.geom.translate(node.bounds.min - before.min);
                } else if shape.geom.bbox() != node.bounds {
                    set_bounds(&mut shape.geom, node.bounds);
                }
            } else if shape.geom.bbox() != node.bounds {
                set_bounds(&mut shape.geom, node.bounds);
            }
        }
    }
}

/// Recalculate a frame subtree with intrinsic measurement and parent-first placement.
pub fn reflow(doc: &mut Document, layer: usize, frame_id: u64) {
    let mut tree = Tree::new(doc, layer);
    let Some(&root) = tree.index.get(&frame_id) else {
        return;
    };
    if !tree.nodes[root].layout.frame {
        return;
    }
    tree.solve(&[root], false);
    tree.apply(doc, layer);
}

/// Reflow every root frame in one indexed pass. Call once after a document edit
/// instead of once per affected child when multiple layouts share ancestors.
pub fn reflow_roots(doc: &mut Document, layer: usize) {
    let mut tree = Tree::new(doc, layer);
    let roots: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (n.layout.frame && n.parent.is_none()).then_some(i))
        .collect();
    tree.solve(&roots, false);
    tree.apply(doc, layer);
}

/// Apply constraints from the original gesture snapshot, never from the last
/// pointer update. Descendants that the canvas initially scaled are restored
/// before layout, preventing cumulative drift and double transformations.
pub fn apply_resize(doc: &mut Document, orig: &[(usize, u64, Geom)], changed: &[(usize, u64)]) {
    let layers: HashSet<_> = changed.iter().map(|&(li, _)| li).collect();
    for layer in layers {
        let mut tree = Tree::new(doc, layer);
        let changed: HashSet<_> = changed
            .iter()
            .filter(|&&(li, _)| li == layer)
            .filter_map(|&(_, id)| tree.index.get(&id).copied())
            .filter(|&i| tree.nodes[i].layout.frame)
            .collect();
        let roots: Vec<_> = changed
            .iter()
            .copied()
            .filter(|&i| {
                let mut parent = tree.nodes[i].parent;
                let mut seen = HashSet::from([i]);
                while let Some(p) = parent {
                    if !seen.insert(p) {
                        return false;
                    }
                    if changed.contains(&p) {
                        return false;
                    }
                    parent = tree.nodes[p].parent;
                }
                true
            })
            .collect();
        let root_set: HashSet<_> = roots.iter().copied().collect();
        let mut affected = HashSet::new();
        let mut pending = roots.clone();
        while let Some(i) = pending.pop() {
            if !affected.insert(i) {
                continue;
            }
            pending.extend(tree.nodes[i].children.iter().copied());
        }
        for (li, id, geom) in orig {
            if *li != layer {
                continue;
            }
            if let Some(&i) = tree.index.get(id).filter(|i| affected.contains(i)) {
                tree.nodes[i].original = geom.bbox();
                if !root_set.contains(&i) {
                    tree.nodes[i].bounds = geom.bbox();
                }
                // Restore actual geometry as well: map_into on scaled glyph
                // outlines cannot be undone from bounding boxes alone.
                if !root_set.contains(&i)
                    && let Some(shape) = doc.find_shape_mut(layer, *id)
                {
                    shape.geom = geom.clone();
                }
            }
        }
        tree.solve(&roots, true);
        tree.apply(doc, layer);
    }
}

/// Validate layout ownership independently from the unrelated layer-group tree.
pub fn validate_hierarchy(doc: &Document) -> Result<(), String> {
    for (li, layer) in doc.layers.iter().enumerate() {
        let Some(shapes) = layer.kind.shapes() else {
            continue;
        };
        for shape in shapes {
            if shape
                .layout
                .text_size
                .is_some_and(|v| !v.is_finite() || !(1.0..=100_000.0).contains(&v))
                || shape
                    .layout
                    .aspect_ratio
                    .is_some_and(|v| !v.is_finite() || !(0.0001..=10_000.0).contains(&v))
            {
                return Err("Text sizes and aspect ratios must be finite positive values".into());
            }
            let mut widths = HashSet::new();
            for point in &shape.layout.breakpoints {
                if !point.max_width.is_finite()
                    || !(1.0..=1_000_000.0).contains(&point.max_width)
                    || !widths.insert(point.max_width.to_bits())
                {
                    return Err(
                        "Responsive breakpoints need unique widths between 1 and 1000000".into(),
                    );
                }
                if point
                    .text_size
                    .is_some_and(|n| !n.is_finite() || !(1.0..=100_000.0).contains(&n))
                    || point.columns.is_some_and(|n| !(1..=1024).contains(&n))
                    || point
                        .gap
                        .into_iter()
                        .chain(point.cross_gap)
                        .chain(point.padding.into_iter().flatten())
                        .any(|n| !n.is_finite() || !(0.0..=1_000_000.0).contains(&n))
                {
                    return Err("Responsive spacing and column counts are invalid".into());
                }
            }
        }
        let index: HashMap<_, _> = shapes.iter().map(|s| (s.id, s)).collect();
        let mut depths = HashMap::new();
        for shape in shapes {
            let mut seen = HashSet::new();
            let mut path = Vec::new();
            let mut current = shape;
            let mut depth;
            loop {
                if let Some(&known_depth) = depths.get(&current.id) {
                    depth = known_depth;
                    break;
                }
                if !seen.insert(current.id) {
                    return Err(format!("Layout frame cycle on layer {li}"));
                }
                path.push(current.id);
                if path.len() > MAX_DEPTH {
                    return Err(format!("Layout nesting exceeds {MAX_DEPTH} frames"));
                }
                let Some(parent) = current.layout.parent else {
                    depth = 0;
                    break;
                };
                current = index
                    .get(&parent)
                    .copied()
                    .filter(|s| s.layout.frame)
                    .ok_or_else(|| {
                        format!("Layout parent {parent} on layer {li} is missing or is not a frame")
                    })?;
            }
            for id in path.into_iter().rev() {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(format!("Layout nesting exceeds {MAX_DEPTH} frames"));
                }
                depths.insert(id, depth);
            }
        }
    }
    Ok(())
}

/// Repair links during import/paste without deleting artwork. Invalid ownership
/// is detached; in a cycle only the first encountered offending edge is removed.
pub fn repair_hierarchy(doc: &mut Document) {
    for layer in &mut doc.layers {
        let Some(shapes) = layer.kind.shapes_mut() else {
            continue;
        };
        let index: HashMap<_, _> = shapes.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        for i in 0..shapes.len() {
            if let Some(parent) = shapes[i].layout.parent
                && (parent == shapes[i].id
                    || index.get(&parent).is_none_or(|&p| !shapes[p].layout.frame))
            {
                shapes[i].layout.parent = None;
            }
        }
        let mut depths = HashMap::new();
        for i in 0..shapes.len() {
            let mut current = i;
            let mut seen = HashSet::new();
            let mut path = Vec::new();
            let mut depth;
            loop {
                if let Some(&known_depth) = depths.get(&current) {
                    depth = known_depth;
                    break;
                }
                seen.insert(current);
                path.push(current);
                let Some(parent) = shapes[current]
                    .layout
                    .parent
                    .and_then(|p| index.get(&p).copied())
                else {
                    depth = 0;
                    break;
                };
                if seen.contains(&parent) || path.len() >= MAX_DEPTH {
                    shapes[current].layout.parent = None;
                    depth = 0;
                    break;
                }
                current = parent;
            }
            for index in path.into_iter().rev() {
                depth += 1;
                if depth > MAX_DEPTH {
                    shapes[index].layout.parent = None;
                    depth = 1;
                }
                depths.insert(index, depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, apply};

    fn document() -> Document {
        Document::new("Responsive", 1200.0, 900.0, 72.0)
    }
    fn add(doc: &mut Document, parent: Option<u64>, origin: Pt, size: Pt, frame: bool) -> u64 {
        let mut s = make_frame(origin, size);
        s.layout.frame = frame;
        s.layout.parent = parent;
        let id = s.id;
        apply(doc, &Cmd::AddShape { layer: 1, shape: s });
        id
    }
    fn shape(doc: &mut Document, id: u64) -> &mut Shape {
        doc.find_shape_mut(1, id).unwrap()
    }
    fn bounds(doc: &Document, id: u64) -> Bounds {
        doc.find_shape(1, id).unwrap().geom.bbox()
    }
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.05, "{a} != {b}");
    }
    fn stack() -> AutoStack {
        AutoStack {
            gap: 10.0,
            padding: [10.0; 4],
            ..AutoStack::default()
        }
    }

    #[test]
    fn nested_translation_reflows_children_against_final_parent() {
        let mut doc = document();
        let outer = add(
            &mut doc,
            None,
            Pt::new(100.0, 200.0),
            Pt::new(300.0, 400.0),
            true,
        );
        let inner = add(&mut doc, Some(outer), Pt::ZERO, Pt::new(100.0, 100.0), true);
        let leaf = add(&mut doc, Some(inner), Pt::ZERO, Pt::new(20.0, 20.0), false);
        shape(&mut doc, outer).layout.stack = Some(stack());
        shape(&mut doc, inner).layout.stack = Some(stack());
        reflow(&mut doc, 1, outer);
        close(bounds(&doc, inner).min.x, 110.0);
        close(bounds(&doc, leaf).min.x, 120.0);
        close(bounds(&doc, leaf).min.y, 220.0);
        let first = bounds(&doc, leaf);
        reflow(&mut doc, 1, outer);
        assert_eq!(bounds(&doc, leaf), first);
    }

    #[test]
    fn fill_redistributes_space_after_max_and_min_constraints() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(500.0, 100.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            direction: StackAxis::Horizontal,
            ..stack()
        });
        let a = add(&mut doc, Some(root), Pt::ZERO, Pt::new(50.0, 20.0), false);
        let b = add(&mut doc, Some(root), Pt::ZERO, Pt::new(50.0, 20.0), false);
        shape(&mut doc, a).layout.width = Sizing::Fill;
        shape(&mut doc, a).layout.max_width = Some(100.0);
        shape(&mut doc, b).layout.width = Sizing::Fill;
        reflow(&mut doc, 1, root);
        close(bounds(&doc, a).width(), 100.0);
        close(bounds(&doc, b).width(), 370.0);
        close(bounds(&doc, b).max.x, 490.0);
    }

    #[test]
    fn mixed_min_and_max_fill_constraints_do_not_overflow_available_space() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(130.0, 100.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            direction: StackAxis::Horizontal,
            ..stack()
        });
        let a = add(&mut doc, Some(root), Pt::ZERO, Pt::new(20.0, 20.0), false);
        let b = add(&mut doc, Some(root), Pt::ZERO, Pt::new(20.0, 20.0), false);
        shape(&mut doc, a).layout.width = Sizing::Fill;
        shape(&mut doc, a).layout.min_width = Some(80.0);
        shape(&mut doc, b).layout.width = Sizing::Fill;
        shape(&mut doc, b).layout.max_width = Some(40.0);
        reflow(&mut doc, 1, root);
        close(bounds(&doc, a).width(), 80.0);
        close(bounds(&doc, b).width(), 20.0);
        close(bounds(&doc, b).max.x, 120.0);
    }

    #[test]
    fn hug_measures_nested_content_without_absolute_or_hidden_children() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(500.0, 500.0), true);
        shape(&mut doc, root).layout.stack = Some(stack());
        shape(&mut doc, root).layout.width = Sizing::Hug;
        shape(&mut doc, root).layout.height = Sizing::Hug;
        add(&mut doc, Some(root), Pt::ZERO, Pt::new(80.0, 20.0), false);
        add(&mut doc, Some(root), Pt::ZERO, Pt::new(40.0, 30.0), false);
        let overlay = add(&mut doc, Some(root), Pt::ZERO, Pt::new(800.0, 800.0), false);
        shape(&mut doc, overlay).layout.absolute = true;
        let hidden = add(&mut doc, Some(root), Pt::ZERO, Pt::new(800.0, 800.0), false);
        shape(&mut doc, hidden).visible = false;
        reflow(&mut doc, 1, root);
        close(bounds(&doc, root).width(), 100.0);
        close(bounds(&doc, root).height(), 80.0);
    }

    #[test]
    fn wrapping_reacts_to_frame_resize_and_nested_hug_height() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(220.0, 500.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            flow: StackFlow::Wrap,
            direction: StackAxis::Horizontal,
            cross_gap: 8.0,
            ..stack()
        });
        shape(&mut doc, root).layout.height = Sizing::Hug;
        let ids: Vec<_> = (0..3)
            .map(|_| add(&mut doc, Some(root), Pt::ZERO, Pt::new(90.0, 30.0), false))
            .collect();
        reflow(&mut doc, 1, root);
        close(bounds(&doc, ids[2]).min.y, 48.0);
        close(bounds(&doc, root).height(), 88.0);
        set_bounds(
            &mut shape(&mut doc, root).geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(330.0, 88.0)),
        );
        reflow(&mut doc, 1, root);
        close(bounds(&doc, ids[2]).min.y, 10.0);
        close(bounds(&doc, root).height(), 50.0);
    }

    #[test]
    fn grid_places_rows_with_fill_cells() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(320.0, 200.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            flow: StackFlow::Grid,
            columns: 2,
            cross_gap: 15.0,
            ..stack()
        });
        let ids: Vec<_> = (0..4)
            .map(|_| add(&mut doc, Some(root), Pt::ZERO, Pt::new(50.0, 30.0), false))
            .collect();
        for &id in &ids {
            shape(&mut doc, id).layout.width = Sizing::Fill;
        }
        reflow(&mut doc, 1, root);
        close(bounds(&doc, ids[0]).width(), 145.0);
        close(bounds(&doc, ids[1]).min.x, 165.0);
        close(bounds(&doc, ids[2]).min.y, 55.0);
    }

    #[test]
    fn space_between_aligns_outer_edges() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(300.0, 100.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            direction: StackAxis::Horizontal,
            justify: StackJustify::SpaceBetween,
            ..stack()
        });
        let a = add(&mut doc, Some(root), Pt::ZERO, Pt::new(20.0, 20.0), false);
        let b = add(&mut doc, Some(root), Pt::ZERO, Pt::new(20.0, 20.0), false);
        reflow(&mut doc, 1, root);
        close(bounds(&doc, a).min.x, 10.0);
        close(bounds(&doc, b).max.x, 290.0);
    }

    #[test]
    fn nested_free_constraints_are_applied_once_per_resize() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(200.0, 200.0), true);
        let inner = add(
            &mut doc,
            Some(root),
            Pt::new(20.0, 20.0),
            Pt::new(160.0, 100.0),
            true,
        );
        shape(&mut doc, inner).layout.constraint_x = Constraint::Stretch;
        let leaf = add(
            &mut doc,
            Some(inner),
            Pt::new(140.0, 30.0),
            Pt::new(30.0, 20.0),
            false,
        );
        shape(&mut doc, leaf).layout.constraint_x = Constraint::End;
        let orig: Vec<_> = [root, inner, leaf]
            .iter()
            .map(|&id| (1, id, doc.find_shape(1, id).unwrap().geom.clone()))
            .collect();
        for _ in 0..2 {
            set_bounds(
                &mut shape(&mut doc, root).geom,
                Bounds::from_min_size(Pt::new(100.0, 100.0), Pt::new(400.0, 200.0)),
            );
            apply_resize(&mut doc, &orig, &[(1, root), (1, inner), (1, leaf)]);
            close(bounds(&doc, inner).min.x, 120.0);
            close(bounds(&doc, inner).width(), 360.0);
            close(bounds(&doc, leaf).min.x, 440.0);
            close(bounds(&doc, leaf).min.y, 130.0);
        }
    }

    #[test]
    fn malformed_cycles_and_missing_parents_repair_without_losing_shapes() {
        let mut doc = document();
        let a = add(&mut doc, None, Pt::ZERO, Pt::splat(100.0), true);
        let b = add(&mut doc, Some(a), Pt::ZERO, Pt::splat(50.0), true);
        shape(&mut doc, a).layout.parent = Some(b);
        let c = add(&mut doc, Some(u64::MAX), Pt::ZERO, Pt::splat(20.0), false);
        assert!(validate_hierarchy(&doc).is_err());
        assert_eq!(descendants(&doc, 1, a), vec![b]);
        reflow(&mut doc, 1, a); // Invalid input must never hang the solver.
        repair_hierarchy(&mut doc);
        assert!(validate_hierarchy(&doc).is_ok());
        assert!(doc.find_shape(1, c).unwrap().layout.parent.is_none());
        assert_eq!(doc.layers[1].kind.shapes().unwrap().len(), 3);
    }

    #[test]
    fn reflow_preserves_unrelated_artwork_and_nested_text_font_size() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(200.0, 200.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            align: StackAlign::Stretch,
            ..stack()
        });
        let mut run = crate::geom::TypeRun {
            content: "Readable paragraph with fixed typography".into(),
            px: 20.0,
            ..Default::default()
        };
        run.contours = crate::text::shape(&run);
        let outside = Shape::new(Geom::Text(run.clone()), Style::default());
        let untouched = outside.clone();
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: outside,
            },
        );
        let mut text = Shape::new(Geom::Text(run), Style::default());
        text.layout.parent = Some(root);
        text.layout.height = Sizing::Hug;
        let id = text.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: text,
            },
        );
        reflow(&mut doc, 1, root);
        let Geom::Text(run) = &doc.find_shape(1, id).unwrap().geom else {
            panic!("Live text lost")
        };
        close(run.px, 20.0);
        assert_eq!(run.wrap_width, Some(180.0));
        assert!(bounds(&doc, id).height() > 24.0);
        assert_eq!(doc.find_shape(1, untouched.id).unwrap(), &untouched);
    }

    #[test]
    fn thousand_nodes_settle_and_remain_stable() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(1400.0, 900.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            flow: StackFlow::Grid,
            columns: 10,
            ..stack()
        });
        for _ in 0..1000 {
            let id = add(&mut doc, Some(root), Pt::ZERO, Pt::new(80.0, 20.0), false);
            shape(&mut doc, id).layout.width = Sizing::Fill;
        }
        let start = std::time::Instant::now();
        reflow(&mut doc, 1, root);
        let elapsed = start.elapsed();
        let first: Vec<_> = doc.layers[1]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.geom.bbox())
            .collect();
        reflow(&mut doc, 1, root);
        let second: Vec<_> = doc.layers[1]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.geom.bbox())
            .collect();
        assert_eq!(first, second);
        // A broad runaway guard; performance timings are reported, not used as
        // a fragile millisecond requirement on shared CI machines.
        assert!(elapsed < std::time::Duration::from_secs(2));
        eprintln!("1000 layout nodes: {elapsed:?}");
    }

    #[test]
    fn nesting_depth_guard_checks_previously_visited_ancestors() {
        let mut doc = document();
        let mut parent = None;
        for _ in 0..MAX_DEPTH + 2 {
            parent = Some(add(&mut doc, parent, Pt::ZERO, Pt::splat(10.0), true));
        }
        assert!(validate_hierarchy(&doc).is_err());
        repair_hierarchy(&mut doc);
        assert!(validate_hierarchy(&doc).is_ok());
    }

    #[test]
    fn responsive_overrides_cascade_and_restore_desktop_layout() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(1000.0, 400.0), true);
        shape(&mut doc, root).layout.stack = Some(AutoStack {
            direction: StackAxis::Horizontal,
            ..stack()
        });
        shape(&mut doc, root).layout.breakpoints = vec![
            LayoutBreakpoint {
                max_width: 800.0,
                gap: Some(20.0),
                ..Default::default()
            },
            LayoutBreakpoint {
                max_width: 500.0,
                direction: Some(StackAxis::Vertical),
                gap: Some(8.0),
                ..Default::default()
            },
        ];
        let a = add(&mut doc, Some(root), Pt::ZERO, Pt::new(100.0, 20.0), false);
        let b = add(&mut doc, Some(root), Pt::ZERO, Pt::new(100.0, 20.0), false);
        for id in [a, b] {
            shape(&mut doc, id).layout.width = Sizing::Fill;
        }
        reflow(&mut doc, 1, root);
        let desktop_a = bounds(&doc, a);
        let desktop_b = bounds(&doc, b);
        set_bounds(
            &mut shape(&mut doc, root).geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(400.0, 400.0)),
        );
        reflow(&mut doc, 1, root);
        close(bounds(&doc, b).min.y, 38.0);
        close(bounds(&doc, b).width(), 380.0);
        assert_eq!(
            shape(&mut doc, root)
                .layout
                .stack
                .as_ref()
                .unwrap()
                .direction,
            StackAxis::Horizontal
        );
        close(
            shape(&mut doc, root).layout.stack.as_ref().unwrap().gap,
            10.0,
        );
        set_bounds(
            &mut shape(&mut doc, root).geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(1000.0, 400.0)),
        );
        reflow(&mut doc, 1, root);
        assert_eq!(bounds(&doc, a), desktop_a);
        assert_eq!(bounds(&doc, b), desktop_b);
        let serialized = serde_json::to_string(&shape(&mut doc, root).layout).unwrap();
        let restored: FrameLayout = serde_json::from_str(&serialized).unwrap();
        close(resolve_for_width(&restored, 700.0).stack.unwrap().gap, 20.0);
        assert_eq!(
            resolve_for_width(&restored, 400.0).stack.unwrap().direction,
            StackAxis::Vertical
        );
    }

    #[test]
    fn nested_breakpoint_uses_viewport_instead_of_card_width() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(1000.0, 400.0), true);
        let card = add(&mut doc, Some(root), Pt::ZERO, Pt::new(200.0, 200.0), true);
        shape(&mut doc, card).layout.stack = Some(AutoStack {
            direction: StackAxis::Horizontal,
            ..stack()
        });
        shape(&mut doc, card).layout.breakpoints = vec![LayoutBreakpoint {
            max_width: 600.0,
            direction: Some(StackAxis::Vertical),
            ..Default::default()
        }];
        let a = add(&mut doc, Some(card), Pt::ZERO, Pt::splat(20.0), false);
        let b = add(&mut doc, Some(card), Pt::ZERO, Pt::splat(20.0), false);
        reflow(&mut doc, 1, root);
        close(bounds(&doc, a).min.y, bounds(&doc, b).min.y);
        close(bounds(&doc, b).min.x, bounds(&doc, a).max.x + 10.0);
    }

    #[test]
    fn responsive_text_size_preserves_and_restores_authored_typography() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(1000.0, 400.0), true);
        shape(&mut doc, root).layout.stack = Some(stack());
        let mut run = crate::geom::TypeRun {
            content: "Possibility".into(),
            px: 72.0,
            leading: 80.0,
            ..Default::default()
        };
        run.contours = crate::text::shape(&run);
        let mut heading = Shape::new(Geom::Text(run), Style::default());
        heading.layout.parent = Some(root);
        heading.layout.width = Sizing::Fill;
        heading.layout.height = Sizing::Hug;
        heading.layout.breakpoints = vec![LayoutBreakpoint {
            max_width: 600.0,
            text_size: Some(36.0),
            ..Default::default()
        }];
        let id = heading.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: heading,
            },
        );
        reflow(&mut doc, 1, root);
        for (width, px, leading) in [(400.0, 36.0, 40.0), (1000.0, 72.0, 80.0)] {
            set_bounds(
                &mut shape(&mut doc, root).geom,
                Bounds::from_min_size(Pt::ZERO, Pt::new(width, 400.0)),
            );
            reflow(&mut doc, 1, root);
            let Geom::Text(run) = &doc.find_shape(1, id).unwrap().geom else {
                panic!()
            };
            close(run.px, px);
            close(run.leading, leading);
            assert_eq!(doc.find_shape(1, id).unwrap().layout.text_size, Some(72.0));
        }
    }

    #[test]
    fn aspect_ratio_tracks_fill_width_without_stretching_artwork_proportions() {
        let mut doc = document();
        let root = add(&mut doc, None, Pt::ZERO, Pt::new(420.0, 500.0), true);
        shape(&mut doc, root).layout.stack = Some(stack());
        let child = add(&mut doc, Some(root), Pt::ZERO, Pt::new(200.0, 100.0), true);
        shape(&mut doc, child).layout.width = Sizing::Fill;
        shape(&mut doc, child).layout.aspect_ratio = Some(2.0);
        reflow(&mut doc, 1, root);
        close(bounds(&doc, child).width(), 400.0);
        close(bounds(&doc, child).height(), 200.0);
        set_bounds(
            &mut shape(&mut doc, root).geom,
            Bounds::from_min_size(Pt::ZERO, Pt::new(220.0, 500.0)),
        );
        reflow(&mut doc, 1, root);
        close(bounds(&doc, child).height(), 100.0);
    }

    #[test]
    fn responsive_demo_hug_frames_contain_their_wrapped_text() {
        let doc = crate::layout_demo::build();
        let root = doc.layers[0].kind.shapes().unwrap()[0].id;
        for width in [1280.0, 768.0, 375.0] {
            let mut doc = doc.layout_snapshot();
            let frame = doc.find_shape_mut(0, root).unwrap();
            let b = frame.geom.bbox();
            set_bounds(
                &mut frame.geom,
                Bounds::from_min_size(b.min, Pt::new(width, b.height())),
            );
            reflow(&mut doc, 0, root);
            for shape in doc.layers[0].kind.shapes().unwrap() {
                if shape.layout.height != Sizing::Hug || !matches!(shape.geom, Geom::Text(_)) {
                    continue;
                }
                let Some(parent) = shape.layout.parent.and_then(|id| doc.find_shape(0, id)) else {
                    continue;
                };
                if parent.layout.height != Sizing::Hug {
                    continue;
                }
                let child = shape.geom.bbox();
                let parent_bounds = parent.geom.bbox();
                assert!(
                    child.max.y <= parent_bounds.max.y + 0.1,
                    "{width}px: {} bottom {} exceeds {} bottom {}",
                    shape.name,
                    child.max.y,
                    parent.name,
                    parent_bounds.max.y
                );
            }
        }
    }

    #[test]
    fn old_layout_json_defaults_to_fixed_sizing() {
        let layout: FrameLayout = serde_json::from_str(r#"{"frame":true,"stack":{"direction":"Vertical","gap":12,"padding":[16,16,16,16],"align":"Start"}}"#).unwrap();
        assert_eq!(layout.width, Sizing::Fixed);
        assert_eq!(layout.stack.unwrap().flow, StackFlow::Stack);
        assert!(!layout.clip);
        assert!(FrameLayout::frame().clip);
    }
}
