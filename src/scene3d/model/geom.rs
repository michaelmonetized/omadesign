//! Adapted from EffectCraft 0311bdc34d0dc10c2d18ff7875b4f52d483e5e27.
//! Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors.
//! Used under the MIT license; see docs/licenses/effectcraft-model-MIT.txt.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
/// Build a vector. Arguments are x, y and z; returns the vector.
pub const fn vec3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}
impl Vec3 {
    pub const ZERO: Self = vec3(0.0, 0.0, 0.0);
    pub const ONE: Self = vec3(1.0, 1.0, 1.0);
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Self) -> Self {
        vec3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l > 0.0 { self / l } else { self }
    }
}
impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, k: f64) -> Self {
        vec3(self.x * k, self.y * k, self.z * k)
    }
}
impl Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, k: f64) -> Self {
        vec3(self.x / k, self.y / k, self.z / k)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        vec3(-self.x, -self.y, -self.z)
    }
}
impl From<[f64; 3]> for Vec3 {
    fn from(v: [f64; 3]) -> Self {
        vec3(v[0], v[1], v[2])
    }
}
/// Row-major 4×4 matrix on column vectors `(x, y, z, 1)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mat4(pub [[f64; 4]; 4]);

impl Default for Mat4 {
    fn default() -> Self {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    pub fn translate(t: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][3] = t.x;
        m.0[1][3] = t.y;
        m.0[2][3] = t.z;
        m
    }
    pub fn scale(s: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][0] = s.x;
        m.0[1][1] = s.y;
        m.0[2][2] = s.z;
        m
    }
    pub fn rotate_x(deg: f64) -> Mat4 {
        let (s, c) = deg.to_radians().sin_cos();
        Mat4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, -s, 0.0],
            [0.0, s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
    pub fn rotate_y(deg: f64) -> Mat4 {
        let (s, c) = deg.to_radians().sin_cos();
        Mat4([
            [c, 0.0, s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
    pub fn rotate_z(deg: f64) -> Mat4 {
        let (s, c) = deg.to_radians().sin_cos();
        Mat4([
            [c, -s, 0.0, 0.0],
            [s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
    /// Orientation (degrees, X then Y then Z applied to the vector in that order).
    pub fn orientation(o: Vec3) -> Mat4 {
        Mat4::rotate_z(o.z) * Mat4::rotate_y(o.y) * Mat4::rotate_x(o.x)
    }
    /// The 3D layer transform: layer space → parent space.
    pub fn layer_3d(
        anchor: Vec3,
        position: Vec3,
        scale_pct: Vec3,
        orientation: Vec3,
        rotation: Vec3,
    ) -> Mat4 {
        Mat4::translate(position)
            * Mat4::orientation(orientation)
            * Mat4::rotate_z(rotation.z)
            * Mat4::rotate_y(rotation.y)
            * Mat4::rotate_x(rotation.x)
            * Mat4::scale(scale_pct / 100.0)
            * Mat4::translate(-anchor)
    }
    pub fn apply(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        let x = m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3];
        let y = m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3];
        let z = m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3];
        let w = m[3][0] * p.x + m[3][1] * p.y + m[3][2] * p.z + m[3][3];
        if w != 1.0 && w != 0.0 {
            vec3(x / w, y / w, z / w)
        } else {
            vec3(x, y, z)
        }
    }
    pub fn apply_vec(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        vec3(
            m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
            m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
            m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
        )
    }
    pub fn transpose(&self) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = self.0[j][i];
            }
        }
        Mat4(r)
    }
    /// General inverse (Gauss-Jordan with partial pivoting).
    pub fn inverse(&self) -> Option<Mat4> {
        let mut a = self.0;
        let mut inv = Mat4::IDENTITY.0;
        for col in 0..4 {
            let piv = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
            if a[piv][col].abs() < 1e-300 {
                return None;
            }
            a.swap(col, piv);
            inv.swap(col, piv);
            let d = a[col][col];
            for j in 0..4 {
                a[col][j] /= d;
                inv[col][j] /= d;
            }
            for i in 0..4 {
                if i != col {
                    let f = a[i][col];
                    if f != 0.0 {
                        for j in 0..4 {
                            a[i][j] -= f * a[col][j];
                            inv[i][j] -= f * inv[col][j];
                        }
                    }
                }
            }
        }
        Some(Mat4(inv))
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = (0..4).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat4(r)
    }
}
