//! Bounded, native IDML publishing interchange. No extraction or external converter.
mod read;
#[cfg(test)]
mod tests;
mod write;
use crate::{
    color::Rgba,
    document::{Fill, Style},
    geom::{Bounds, Pt},
};
pub use read::{read, read_with_links};
use roxmltree::Node;
use std::collections::BTreeMap;
pub use write::write;

pub const MIMETYPE: &str = "application/vnd.adobe.indesign-idml-package";
const NS: &str = "http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging";
const MAX_PART: u64 = 32 << 20;
const MAX_TOTAL: usize = 256 << 20;
type Attributes = BTreeMap<String, String>;

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn tag<'a>(n: Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}
fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && tag(*c) == name)
}
fn attr(n: Node<'_, '_>, name: &str) -> String {
    n.attribute(name).unwrap_or("").into()
}
fn attributes(n: Node<'_, '_>) -> Attributes {
    let mut a: Attributes = n
        .attributes()
        .filter(|a| a.namespace().is_none())
        .map(|a| (a.name().into(), a.value().into()))
        .collect();
    if let Some(p) = child(n, "Properties") {
        for c in p
            .children()
            .filter(|n| n.is_element() && n.children().all(|x| !x.is_element()))
        {
            a.insert(tag(c).into(), c.text().unwrap_or("").into());
        }
    }
    a
}
fn number(a: &Attributes, key: &str, default: f32) -> Result<f32, String> {
    let Some(value) = a.get(key) else {
        return Ok(default);
    };
    let n: f32 = value
        .parse()
        .map_err(|_| format!("IDML {key} is not a number: {value}"))?;
    if !n.is_finite() || n.abs() > 1e7 {
        return Err(format!("IDML {key} is out of range"));
    }
    Ok(n)
}
fn numbers(s: &str) -> Result<Vec<f32>, String> {
    s.split_whitespace()
        .map(|s| {
            let n: f32 = s.parse().map_err(|_| "Invalid IDML geometry")?;
            if n.is_finite() && n.abs() <= 1e7 {
                Ok(n)
            } else {
                Err("IDML geometry is out of range".into())
            }
        })
        .collect()
}
#[derive(Clone, Copy)]
struct Matrix([f32; 6]);
impl Matrix {
    const ID: Self = Self([1., 0., 0., 1., 0., 0.]);
    fn read(n: Node<'_, '_>) -> Result<Self, String> {
        let Some(s) = n.attribute("ItemTransform") else {
            return Ok(Self::ID);
        };
        Ok(Self(
            numbers(s)?
                .try_into()
                .map_err(|_| "IDML transform needs six values")?,
        ))
    }
    fn point(self, p: Pt) -> Pt {
        let [a, b, c, d, x, y] = self.0;
        Pt::new(a * p.x + c * p.y + x, b * p.x + d * p.y + y)
    }
    fn then(self, parent: Self) -> Self {
        let [a, b, c, d, x, y] = self.0;
        let [e, f, g, h, u, v] = parent.0;
        Self([
            e * a + g * b,
            f * a + h * b,
            e * c + g * d,
            f * c + h * d,
            e * x + g * y + u,
            f * x + h * y + v,
        ])
    }
    fn translated(p: Pt) -> Self {
        Self([1., 0., 0., 1., p.x, p.y])
    }
}
fn bounds(n: Node<'_, '_>, m: Matrix) -> Result<Bounds, String> {
    let points = if let Some(s) = n.attribute("GeometricBounds") {
        let v = numbers(s)?;
        if v.len() != 4 {
            return Err("IDML bounds need four values".into());
        }
        vec![
            Pt::new(v[1], v[0]),
            Pt::new(v[3], v[0]),
            Pt::new(v[3], v[2]),
            Pt::new(v[1], v[2]),
        ]
    } else {
        n.descendants()
            .filter(|n| tag(*n) == "PathPointType")
            .map(|n| {
                let p = numbers(n.attribute("Anchor").unwrap_or(""))?;
                if p.len() != 2 {
                    return Err("IDML anchor needs two values".into());
                }
                Ok(Pt::new(p[0], p[1]))
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let mut iter = points.into_iter().map(|p| m.point(p));
    let mut b = Bounds::from_pt(iter.next().ok_or("IDML item has no bounds or path")?);
    for p in iter {
        b.min.x = b.min.x.min(p.x);
        b.min.y = b.min.y.min(p.y);
        b.max.x = b.max.x.max(p.x);
        b.max.y = b.max.y.max(p.y);
    }
    Ok(b)
}
fn fill(a: &Attributes, key: &str, colors: &BTreeMap<String, Rgba>) -> Fill {
    match a.get(key).map(String::as_str) {
        None | Some("Swatch/None" | "n" | "") => Fill::None,
        Some(id) => Fill::Solid(colors.get(id).copied().unwrap_or(Rgba::BLACK)),
    }
}
fn style(a: &Attributes, colors: &BTreeMap<String, Rgba>) -> Result<Style, String> {
    let width = number(a, "StrokeWeight", 0.)?;
    let stroke = if width > 0. {
        match fill(a, "StrokeColor", colors) {
            Fill::Solid(color) => Some(crate::document::Stroke {
                color,
                width,
                ..Default::default()
            }),
            _ => None,
        }
    } else {
        None
    };
    Ok(Style {
        fill: fill(a, "FillColor", colors),
        stroke,
    })
}
fn warn(notes: &mut Vec<String>, note: impl Into<String>) {
    let note = note.into();
    if !notes.contains(&note) {
        notes.push(note);
    }
}
fn package(kind: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><idPkg:{kind} xmlns:idPkg=\"{NS}\" DOMVersion=\"16.0\">{body}</idPkg:{kind}>"
    )
}

fn fingerprint(layer: &crate::document::Layer) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(layer).map_err(|e| e.to_string())?)
    ))
}
