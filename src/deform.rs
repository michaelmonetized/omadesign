//! Vector envelopes in document coordinates. The cage is deliberately separate
//! from source geometry, so repeated handle drags never compound tessellation.

use crate::geom::{Bounds, Pt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Distort,
    Skew,
    Perspective,
    Mesh,
}

impl Mode {
    pub const ALL: [Self; 4] = [Self::Distort, Self::Skew, Self::Perspective, Self::Mesh];

    pub fn label(self) -> &'static str {
        match self {
            Self::Distort => "Distort",
            Self::Skew => "Skew",
            Self::Perspective => "Perspective",
            Self::Mesh => "Warp mesh",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Distort => "Pull a corner to reshape the envelope.",
            Self::Skew => "Slide a side handle to shear the objects.",
            Self::Perspective => "Pull a corner; straight lines stay straight.",
            Self::Mesh => "Pull any of the nine points to bend the surface.",
        }
    }
}

/// Quad points run clockwise from the top left. Mesh points run left to right,
/// top to bottom. A mesh is a tensor-product quadratic interpolating surface:
/// all nine handles sit on the surface and its first derivatives are continuous.
#[derive(Clone, Debug, PartialEq)]
pub struct Cage {
    pub mode: Mode,
    pub bounds: Bounds,
    controls: Vec<Pt>,
}

impl Cage {
    pub fn new(mode: Mode, bounds: Bounds) -> Option<Self> {
        if !finite(bounds.min)
            || !finite(bounds.max)
            || bounds.width() < 1e-4
            || bounds.height() < 1e-4
        {
            return None;
        }
        let controls = if mode == Mode::Mesh {
            (0..3)
                .flat_map(|y| {
                    (0..3).map(move |x| {
                        bounds.min
                            + Pt::new(
                                bounds.width() * x as f32 * 0.5,
                                bounds.height() * y as f32 * 0.5,
                            )
                    })
                })
                .collect()
        } else {
            vec![
                bounds.min,
                Pt::new(bounds.max.x, bounds.min.y),
                bounds.max,
                Pt::new(bounds.min.x, bounds.max.y),
            ]
        };
        Some(Self {
            mode,
            bounds,
            controls,
        })
    }

    pub fn handles(&self) -> Vec<Pt> {
        if self.mode == Mode::Skew {
            (0..4)
                .map(|i| self.controls[i].lerp(self.controls[(i + 1) % 4], 0.5))
                .collect()
        } else {
            self.controls.clone()
        }
    }

    /// `delta` is always measured from the cage at pointer-down.
    pub fn dragged(&self, handle: usize, delta: Pt) -> Option<Self> {
        if !finite(delta) || handle >= self.handles().len() {
            return None;
        }
        let mut after = self.clone();
        if self.mode == Mode::Skew {
            let delta = if handle.is_multiple_of(2) {
                Pt::new(delta.x, 0.0)
            } else {
                Pt::new(0.0, delta.y)
            };
            after.controls[handle] += delta;
            after.controls[(handle + 1) % 4] += delta;
        } else {
            after.controls[handle] += delta;
        }
        after.mapper()?;
        Some(after)
    }

    pub fn mapper(&self) -> Option<Mapper> {
        if self.controls.iter().any(|p| !finite(*p)) {
            return None;
        }
        let surface = if self.mode == Mode::Mesh {
            Surface::Mesh(self.controls.as_slice().try_into().ok()?)
        } else {
            let q: [Pt; 4] = self.controls.as_slice().try_into().ok()?;
            if !convex(q) {
                return None;
            }
            if self.mode == Mode::Perspective {
                Surface::Perspective(homography(q)?)
            } else {
                Surface::Quad(q)
            }
        };
        Some(Mapper {
            bounds: self.bounds,
            surface,
        })
    }
}

pub struct Mapper {
    bounds: Bounds,
    surface: Surface,
}

enum Surface {
    Quad([Pt; 4]),
    Perspective([f64; 8]),
    Mesh([Pt; 9]),
}

impl Mapper {
    pub fn map(&self, p: Pt) -> Option<Pt> {
        let u = (p.x - self.bounds.min.x) / self.bounds.width();
        let v = (p.y - self.bounds.min.y) / self.bounds.height();
        let mapped = match &self.surface {
            Surface::Quad(q) => q[0].lerp(q[1], u).lerp(q[3].lerp(q[2], u), v),
            Surface::Perspective(h) => {
                let (u, v) = (u as f64, v as f64);
                let denominator = h[6] * u + h[7] * v + 1.0;
                if denominator.abs() < 1e-8 {
                    return None;
                }
                Pt::new(
                    ((h[0] * u + h[1] * v + h[2]) / denominator) as f32,
                    ((h[3] * u + h[4] * v + h[5]) / denominator) as f32,
                )
            }
            Surface::Mesh(points) => {
                let x = quadratic_weights(u);
                let y = quadratic_weights(v);
                let mut out = Pt::ZERO;
                for row in 0..3 {
                    for col in 0..3 {
                        out += points[row * 3 + col] * (x[col] * y[row]);
                    }
                }
                out
            }
        };
        // Refuse pathological envelopes rather than handing non-finite or huge
        // coordinates to the path rasterizer.
        (finite(mapped) && mapped.x.abs().max(mapped.y.abs()) < 1e9).then_some(mapped)
    }

    pub fn grid_lines(&self, divisions: usize, samples: usize) -> Vec<Vec<Pt>> {
        let mut lines = Vec::new();
        for axis in 0..2 {
            for line in 0..=divisions {
                let fraction = line as f32 / divisions.max(1) as f32;
                let points: Option<Vec<_>> = (0..=samples)
                    .map(|i| {
                        let t = i as f32 / samples.max(1) as f32;
                        let (u, v) = if axis == 0 {
                            (fraction, t)
                        } else {
                            (t, fraction)
                        };
                        self.map(
                            self.bounds.min
                                + Pt::new(u * self.bounds.width(), v * self.bounds.height()),
                        )
                    })
                    .collect();
                if let Some(points) = points {
                    lines.push(points);
                }
            }
        }
        lines
    }
}

fn quadratic_weights(t: f32) -> [f32; 3] {
    [
        2.0 * (t - 0.5) * (t - 1.0),
        -4.0 * t * (t - 1.0),
        2.0 * t * (t - 0.5),
    ]
}

fn finite(p: Pt) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn convex(q: [Pt; 4]) -> bool {
    let scale = (q[2] - q[0])
        .length_sq()
        .max((q[3] - q[1]).length_sq())
        .max(1.0);
    (0..4).all(|i| (q[(i + 1) % 4] - q[i]).cross(q[(i + 2) % 4] - q[(i + 1) % 4]) > scale * 1e-6)
}

fn homography(q: [Pt; 4]) -> Option<[f64; 8]> {
    let q = q.map(|p| [p.x as f64, p.y as f64]);
    let dx1 = q[1][0] - q[2][0];
    let dx2 = q[3][0] - q[2][0];
    let dx3 = q[0][0] - q[1][0] + q[2][0] - q[3][0];
    let dy1 = q[1][1] - q[2][1];
    let dy2 = q[3][1] - q[2][1];
    let dy3 = q[0][1] - q[1][1] + q[2][1] - q[3][1];
    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() < 1e-10 {
        return None;
    }
    let g = (dx3 * dy2 - dx2 * dy3) / det;
    let h = (dx1 * dy3 - dx3 * dy1) / det;
    if [1.0, 1.0 + g, 1.0 + h, 1.0 + g + h]
        .iter()
        .any(|d| *d < 1e-7)
    {
        return None;
    }
    Some([
        q[1][0] - q[0][0] + g * q[1][0],
        q[3][0] - q[0][0] + h * q[3][0],
        q[0][0],
        q[1][1] - q[0][1] + g * q[1][1],
        q[3][1] - q[0][1] + h * q[3][1],
        q[0][1],
        g,
        h,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bounds() -> Bounds {
        Bounds::from_min_size(Pt::new(10.0, 20.0), Pt::new(200.0, 100.0))
    }
    fn near(a: Pt, b: Pt) {
        assert!((a - b).length() < 0.001, "{a:?} != {b:?}");
    }

    #[test]
    fn untouched_modes_preserve_corners_and_interior() {
        for mode in Mode::ALL {
            let mapper = Cage::new(mode, bounds()).unwrap().mapper().unwrap();
            for p in [
                bounds().min,
                bounds().max,
                bounds().center(),
                Pt::new(73.0, 81.0),
            ] {
                near(mapper.map(p).unwrap(), p);
            }
        }
    }

    #[test]
    fn perspective_hits_each_corner_and_keeps_a_diagonal_straight() {
        let cage = Cage::new(Mode::Perspective, bounds())
            .unwrap()
            .dragged(0, Pt::new(50.0, 10.0))
            .unwrap();
        let mapper = cage.mapper().unwrap();
        let original = Cage::new(Mode::Distort, bounds()).unwrap().handles();
        for (p, q) in original.iter().zip(cage.handles()) {
            near(mapper.map(*p).unwrap(), q);
        }
        let a = mapper.map(bounds().min).unwrap();
        let b = mapper.map(bounds().max).unwrap();
        let middle = mapper.map(bounds().center()).unwrap();
        assert!((middle - a).cross(b - a).abs() < 0.002);
    }

    #[test]
    fn distortion_bends_interior_with_bilinear_weights() {
        let cage = Cage::new(Mode::Distort, bounds())
            .unwrap()
            .dragged(0, Pt::new(-40.0, -20.0))
            .unwrap();
        near(
            cage.mapper().unwrap().map(bounds().center()).unwrap(),
            bounds().center() + Pt::new(-10.0, -5.0),
        );
    }

    #[test]
    fn skew_slides_an_edge_and_leaves_the_opposite_edge_anchored() {
        let cage = Cage::new(Mode::Skew, bounds())
            .unwrap()
            .dragged(0, Pt::new(30.0, 90.0))
            .unwrap();
        let mapper = cage.mapper().unwrap();
        near(
            mapper.map(bounds().min).unwrap(),
            bounds().min + Pt::new(30.0, 0.0),
        );
        near(mapper.map(bounds().max).unwrap(), bounds().max);
        near(
            mapper.map(bounds().center()).unwrap(),
            bounds().center() + Pt::new(15.0, 0.0),
        );
    }

    #[test]
    fn mesh_interpolates_center_without_moving_boundary_and_is_smooth() {
        let cage = Cage::new(Mode::Mesh, bounds())
            .unwrap()
            .dragged(4, Pt::new(18.0, 24.0))
            .unwrap();
        let mapper = cage.mapper().unwrap();
        near(
            mapper.map(bounds().center()).unwrap(),
            bounds().center() + Pt::new(18.0, 24.0),
        );
        for p in [bounds().min, bounds().max, Pt::new(110.0, 20.0)] {
            near(mapper.map(p).unwrap(), p);
        }
        let center = bounds().center();
        let left = mapper.map(center - Pt::new(0.1, 0.0)).unwrap();
        let mid = mapper.map(center).unwrap();
        let right = mapper.map(center + Pt::new(0.1, 0.0)).unwrap();
        assert!(((mid - left) - (right - mid)).length() < 0.001);
    }

    #[test]
    fn singular_inverted_and_nonfinite_envelopes_are_rejected() {
        assert!(Cage::new(Mode::Mesh, Bounds::from_pt(Pt::ZERO)).is_none());
        for mode in [Mode::Distort, Mode::Skew, Mode::Perspective] {
            let cage = Cage::new(mode, bounds()).unwrap();
            assert!(cage.dragged(0, Pt::new(f32::NAN, 0.0)).is_none());
            if mode != Mode::Skew {
                assert!(cage.dragged(0, Pt::new(200.0, 0.0)).is_none());
                assert!(cage.dragged(0, Pt::new(210.0, 110.0)).is_none());
            }
        }
    }
}
