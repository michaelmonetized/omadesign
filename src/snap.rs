//! Snap a world point to grid, guides, and other object edges.

use crate::document::Document;
use crate::geom::Pt;

#[derive(Clone, Copy, Debug)]
pub struct SnapSettings {
    pub enabled: bool,
    pub grid: bool,
    pub guides: bool,
    pub objects: bool,
    pub threshold: f32,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            grid: true,
            guides: true,
            objects: true,
            threshold: 6.0,
        }
    }
}

pub fn snap_point(doc: &Document, settings: SnapSettings, p: Pt, view_scale: f32) -> Pt {
    if !settings.enabled {
        return p;
    }
    let thresh = (settings.threshold / view_scale.max(0.05)).max(0.5);
    let mut x = p.x;
    let mut y = p.y;
    let mut dx = thresh + 1.0;
    let mut dy = thresh + 1.0;

    let consider = |candidate: f32, current: f32, best: &mut f32, out: &mut f32| {
        let d = (candidate - current).abs();
        if d < *best && d <= thresh {
            *best = d;
            *out = candidate;
        }
    };

    if settings.grid && doc.grid.snap {
        let g = doc.grid.size.max(1.0);
        consider((p.x / g).round() * g, p.x, &mut dx, &mut x);
        consider((p.y / g).round() * g, p.y, &mut dy, &mut y);
    }

    if settings.guides {
        for g in &doc.guides {
            if g.vertical {
                consider(g.pos, p.x, &mut dx, &mut x);
            } else {
                consider(g.pos, p.y, &mut dy, &mut y);
            }
        }
        consider(0.0, p.x, &mut dx, &mut x);
        consider(doc.width, p.x, &mut dx, &mut x);
        consider(doc.width * 0.5, p.x, &mut dx, &mut x);
        consider(0.0, p.y, &mut dy, &mut y);
        consider(doc.height, p.y, &mut dy, &mut y);
        consider(doc.height * 0.5, p.y, &mut dy, &mut y);
    }

    if settings.objects {
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            if let Some(shapes) = layer.kind.shapes() {
                for s in shapes {
                    let b = s.world_bbox();
                    consider(b.min.x, p.x, &mut dx, &mut x);
                    consider(b.max.x, p.x, &mut dx, &mut x);
                    consider(b.center().x, p.x, &mut dx, &mut x);
                    consider(b.min.y, p.y, &mut dy, &mut y);
                    consider(b.max.y, p.y, &mut dy, &mut y);
                    consider(b.center().y, p.y, &mut dy, &mut y);
                }
            }
        }
    }

    Pt::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    #[test]
    fn snaps_to_grid() {
        let mut doc = Document::new("t", 100.0, 100.0, 72.0);
        doc.grid.size = 8.0;
        doc.grid.snap = true;
        let s = SnapSettings {
            enabled: true,
            grid: true,
            guides: false,
            objects: false,
            threshold: 4.0,
        };
        let p = snap_point(&doc, s, Pt::new(9.0, 3.0), 1.0);
        assert!((p.x - 8.0).abs() < 0.01);
        assert!((p.y - 0.0).abs() < 0.01 || (p.y - 8.0).abs() < 0.01);
    }
}
