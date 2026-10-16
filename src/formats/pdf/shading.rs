use super::*;

impl Reader<'_> {
    pub(super) fn shading(
        &mut self,
        resources: &Dictionary,
        name: &[u8],
        state: &State,
        marked: &[(String, bool)],
    ) -> Result<(), String> {
        let shading = resource(self.pdf, resources, b"Shading", name)
            .and_then(|object| object.as_dict().ok())
            .ok_or("Shading resource is missing")?;
        let kind = shading
            .get(b"ShadingType")
            .ok()
            .and_then(number_value)
            .unwrap_or(0.) as i32;
        if kind != 2 && kind != 3 {
            return Err(format!(
                "shading type {kind} is unsupported; axial and radial gradients are supported"
            ));
        }
        let coords = shading
            .get(b"Coords")
            .ok()
            .and_then(|object| resolve(self.pdf, object))
            .and_then(|object| object.as_array().ok())
            .ok_or("Axial gradient coordinates missing")?;
        if coords.len() != if kind == 3 { 6 } else { 4 } {
            return Err("Invalid axial gradient coordinates".into());
        }
        let coordinate = |i| number_value(&coords[i]).ok_or("Invalid gradient coordinate");
        let from = state.matrix.map(Pt::new(coordinate(0)?, coordinate(1)?));
        let to = if kind == 2 {
            state.matrix.map(Pt::new(coordinate(2)?, coordinate(3)?))
        } else {
            let center = Pt::new(coordinate(3)?, coordinate(4)?);
            if coordinate(2)?.abs() > 0.0001
                || (Pt::new(coordinate(0)?, coordinate(1)?) - center).length() > 0.0001
            {
                self.warn(
                    "PDF focal/two-circle radial gradients use a centered radial approximation.",
                );
            }
            state.matrix.map(center + Pt::new(coordinate(5)?, 0.))
        };
        let function = shading
            .get(b"Function")
            .map_err(|_| "Gradient function missing")?;
        let domain = shading
            .get(b"Domain")
            .ok()
            .and_then(|object| object.as_array().ok());
        let start = domain
            .and_then(|array| array.first())
            .and_then(number_value)
            .unwrap_or(0.);
        let end = domain
            .and_then(|array| array.get(1))
            .and_then(number_value)
            .unwrap_or(1.);
        let c0 = values(self.pdf, function, start, 0)?;
        let c1 = values(self.pdf, function, end, 0)?;
        let mut color0 = color(&c0);
        let mut color1 = color(&c1);
        color0.a = byte(state.fill_alpha);
        color1.a = byte(state.fill_alpha);
        let geom = if let Some(clip) = &state.clip {
            (**clip).clone()
        } else if let Some(bbox) = shading
            .get(b"BBox")
            .ok()
            .and_then(|object| rectangle(self.pdf, object))
        {
            masks::transformed_rectangle(bbox, state.matrix)
        } else {
            let board = self
                .document
                .artboards
                .last()
                .ok_or("Shading has no page bounds")?;
            Geom::Rect {
                origin: board.origin,
                size: board.size,
                radius: 0.,
            }
        };
        let bounds = geom.bbox();
        if bounds.width() <= 0. || bounds.height() <= 0. {
            return Ok(());
        }
        let fill = if color0 == color1 && values(self.pdf, function, (start + end) * 0.5, 0)? == c0
        {
            Fill::Solid(color0)
        } else if kind == 2
            && dictionary(self.pdf, function)
                .and_then(|d| d.get(b"FunctionType").ok())
                .and_then(number_value)
                == Some(2.)
            && dictionary(self.pdf, function)
                .and_then(|d| d.get(b"N").ok())
                .and_then(number_value)
                == Some(1.)
        {
            Fill::Linear {
                from: [
                    (from.x - bounds.min.x) / bounds.width(),
                    (from.y - bounds.min.y) / bounds.height(),
                ],
                to: [
                    (to.x - bounds.min.x) / bounds.width(),
                    (to.y - bounds.min.y) / bounds.height(),
                ],
                c0: color0,
                c1: color1,
            }
        } else {
            let mut positions = vec![start, end];
            if let Some(dict) = dictionary(self.pdf, function) {
                if let Some(cuts) = dict.get(b"Bounds").ok().and_then(|o| o.as_array().ok()) {
                    positions.extend(cuts.iter().filter_map(number_value));
                }
                if dict
                    .get(b"N")
                    .ok()
                    .and_then(number_value)
                    .is_some_and(|n| (n - 1.).abs() > 0.001)
                {
                    positions.extend((1..32).map(|i| start + (end - start) * i as f32 / 32.));
                }
            }
            positions.sort_by(f32::total_cmp);
            positions.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
            let stops = positions
                .into_iter()
                .map(|t| {
                    let mut c = color(&values(self.pdf, function, t, 0)?);
                    c.a = byte(state.fill_alpha);
                    Ok(crate::gradient::GradientStop {
                        offset: (t - start) / (end - start).max(1e-9),
                        color: c,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut gradient = crate::gradient::Gradient::new(
                if kind == 3 {
                    crate::gradient::GradientKind::Radial
                } else {
                    crate::gradient::GradientKind::Linear
                },
                color0,
                color1,
            );
            gradient.from = [
                (from.x - bounds.min.x) / bounds.width(),
                (from.y - bounds.min.y) / bounds.height(),
            ];
            gradient.to = [
                (to.x - bounds.min.x) / bounds.width(),
                (to.y - bounds.min.y) / bounds.height(),
            ];
            gradient.stops = stops;
            Fill::Gradient(gradient)
        };
        if shading
            .get(b"Extend")
            .ok()
            .and_then(|object| object.as_array().ok())
            .is_some_and(|array| {
                array
                    .iter()
                    .any(|value| value.as_bool().ok() == Some(false))
            })
        {
            self.warn("Unextended PDF axial gradients extend their endpoint colors to the clipping boundary.");
        }
        let mut shape = Shape::new(geom, Style { fill, stroke: None });
        shape.name = "Gradient".into();
        self.add_shapes(vec![shape], state, marked)
    }
}

fn color(values: &[f32]) -> Rgba {
    match values {
        [gray_value] => gray(*gray_value),
        [r, g, b] => rgb(*r, *g, *b),
        [c, m, y, k] => cmyk(*c, *m, *y, *k),
        _ => Rgba::BLACK,
    }
}

fn values(pdf: &Pdf, object: &Object, t: f32, depth: usize) -> Result<Vec<f32>, String> {
    if depth > 12 {
        return Err("Gradient function nesting exceeds 12".into());
    }
    let object = resolve(pdf, object).ok_or("Gradient function reference missing")?;
    if let Object::Array(functions) = object {
        let mut out = vec![];
        for function in functions {
            out.extend(values(pdf, function, t, depth + 1)?);
        }
        return Ok(out);
    }
    let function = object
        .as_dict()
        .map_err(|_| "Unsupported gradient function object")?;
    match function
        .get(b"FunctionType")
        .ok()
        .and_then(number_value)
        .unwrap_or(0.) as i32
    {
        2 => {
            let c0 = function
                .get(b"C0")
                .ok()
                .and_then(|object| object.as_array().ok())
                .map(|array| array.iter().filter_map(number_value).collect::<Vec<_>>())
                .unwrap_or(vec![0.]);
            let c1 = function
                .get(b"C1")
                .ok()
                .and_then(|object| object.as_array().ok())
                .map(|array| array.iter().filter_map(number_value).collect::<Vec<_>>())
                .unwrap_or(vec![1.]);
            if c0.len() != c1.len() {
                return Err("Gradient color dimensions differ".into());
            }
            let exponent = function.get(b"N").ok().and_then(number_value).unwrap_or(1.);
            let value = t.powf(exponent);
            Ok(c0
                .iter()
                .zip(&c1)
                .map(|(a, b)| a + value * (b - a))
                .collect())
        }
        3 => {
            let functions = function
                .get(b"Functions")
                .ok()
                .and_then(|object| object.as_array().ok())
                .ok_or("Stitched gradient functions missing")?;
            let bounds = function
                .get(b"Bounds")
                .ok()
                .and_then(|object| object.as_array().ok())
                .map(|array| array.iter().filter_map(number_value).collect::<Vec<_>>())
                .unwrap_or_default();
            let domain = function
                .get(b"Domain")
                .ok()
                .and_then(|object| object.as_array().ok());
            let min = domain
                .and_then(|array| array.first())
                .and_then(number_value)
                .unwrap_or(0.);
            let max = domain
                .and_then(|array| array.get(1))
                .and_then(number_value)
                .unwrap_or(1.);
            let index = bounds
                .iter()
                .position(|bound| t < *bound)
                .unwrap_or(bounds.len());
            let low = if index == 0 { min } else { bounds[index - 1] };
            let high = bounds.get(index).copied().unwrap_or(max);
            let encode = function
                .get(b"Encode")
                .ok()
                .and_then(|object| object.as_array().ok());
            let a = encode
                .and_then(|array| array.get(index * 2))
                .and_then(number_value)
                .unwrap_or(0.);
            let b = encode
                .and_then(|array| array.get(index * 2 + 1))
                .and_then(number_value)
                .unwrap_or(1.);
            let mapped = if (high - low).abs() < 1e-8 {
                a
            } else {
                a + (t - low) / (high - low) * (b - a)
            };
            values(
                pdf,
                functions
                    .get(index)
                    .ok_or("Stitched gradient index out of range")?,
                mapped,
                depth + 1,
            )
        }
        other => Err(format!("gradient function type {other} is unsupported")),
    }
}
