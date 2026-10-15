//! Bounded numeric expressions. Source has no loops, mutation, files or clock access.

use super::{Motion, Prop, eval_keys};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_SOURCE: usize = 4096;
pub const MAX_EXPRESSIONS: usize = 256;
pub const MAX_WORK: usize = 100_000;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub shape: u64,
    pub prop: Prop,
    pub source: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    Syntax,
    UnknownName,
    InvalidArgument,
    DivideByZero,
    NonFinite,
    WorkLimit,
    DepthLimit,
    Cycle,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub shape: u64,
    pub prop: Prop,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: object {} {}: {}",
            self.code,
            self.shape,
            self.prop.name(),
            self.message
        )
    }
}
impl std::error::Error for Error {}

pub struct Evaluation<'a> {
    motion: &'a Motion,
    time: f32,
    remaining: usize,
    active: Vec<(u64, Prop)>,
    keep: &'a mut dyn FnMut() -> bool,
}

impl<'a> Evaluation<'a> {
    /// Start a frame budget.
    /// Args: motion data, finite time and cancellation callback. Returns an isolated evaluator.
    pub fn new(motion: &'a Motion, time: f32, keep: &'a mut dyn FnMut() -> bool) -> Self {
        Self {
            motion,
            time,
            remaining: MAX_WORK,
            active: vec![],
            keep,
        }
    }

    fn error(&self, code: ErrorCode, message: impl Into<String>) -> Error {
        let (shape, prop) = self.active.last().copied().unwrap_or((0, Prop::X));
        Error {
            code,
            shape,
            prop,
            message: message.into(),
        }
    }

    fn step(&mut self) -> Result<(), Error> {
        if !(self.keep)() {
            return Err(self.error(ErrorCode::Cancelled, "render cancelled"));
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| self.error(ErrorCode::WorkLimit, "frame expression budget exhausted"))?;
        Ok(())
    }

    /// Evaluate one numeric channel, including property references.
    /// Args: persistent target id and property. Returns a finite value or a named error.
    pub fn value(&mut self, shape: u64, prop: Prop) -> Result<Option<f32>, Error> {
        self.step()?;
        let base = self
            .motion
            .tracks
            .iter()
            .find(|t| t.shape == shape && t.prop == prop)
            .and_then(|t| eval_keys(&t.keys, self.time, self.motion.duration));
        let Some(expr) = self
            .motion
            .expressions
            .iter()
            .find(|e| e.shape == shape && e.prop == prop && e.enabled)
        else {
            return Ok(base);
        };
        if self.active.contains(&(shape, prop)) {
            return Err(self.error(ErrorCode::Cycle, "circular property reference"));
        }
        if self.active.len() >= MAX_DEPTH {
            return Err(self.error(ErrorCode::DepthLimit, "property references nest too deeply"));
        }
        self.active.push((shape, prop));
        let result = (|| {
            if expr.source.len() > MAX_SOURCE {
                return Err(self.error(ErrorCode::WorkLimit, "expression exceeds 4096 bytes"));
            }
            let mut parser = Parser {
                source: &expr.source,
                at: 0,
                depth: 0,
                base: base.unwrap_or(prop.identity()) as f64,
                shape,
                evaluation: self,
            };
            let value = parser.expression(0)?;
            parser.space();
            if parser.at != parser.source.len() {
                return Err(parser.fail(ErrorCode::Syntax, "unexpected text"));
            }
            if !value.is_finite() || value.abs() > f32::MAX as f64 {
                return Err(parser.fail(ErrorCode::NonFinite, "result must be finite"));
            }
            Ok(Some(value as f32))
        })();
        self.active.pop();
        result
    }
}

struct Parser<'s, 'e, 'm> {
    source: &'s str,
    at: usize,
    depth: usize,
    base: f64,
    shape: u64,
    evaluation: &'e mut Evaluation<'m>,
}

impl Parser<'_, '_, '_> {
    fn fail(&self, code: ErrorCode, message: &str) -> Error {
        self.evaluation
            .error(code, format!("{message} at byte {}", self.at))
    }
    fn space(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, ch: u8) -> bool {
        self.space();
        if self.source.as_bytes().get(self.at) == Some(&ch) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn ident(&mut self) -> &'_ str {
        self.space();
        let start = self.at;
        while self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.at += 1;
        }
        &self.source[start..self.at]
    }
    fn expression(&mut self, min: u8) -> Result<f64, Error> {
        self.evaluation.step()?;
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.fail(ErrorCode::DepthLimit, "expression nests too deeply"));
        }
        let mut lhs = self.primary()?;
        loop {
            self.space();
            let Some(&op) = self.source.as_bytes().get(self.at) else {
                break;
            };
            let (left, right) = match op {
                b'+' | b'-' => (1, 2),
                b'*' | b'/' | b'%' => (3, 4),
                b'^' => (6, 5),
                _ => break,
            };
            if left < min {
                break;
            }
            self.at += 1;
            let rhs = self.expression(right)?;
            lhs = match op {
                b'+' => lhs + rhs,
                b'-' => lhs - rhs,
                b'*' => lhs * rhs,
                b'/' | b'%' if rhs == 0.0 => {
                    return Err(self.fail(ErrorCode::DivideByZero, "zero divisor"));
                }
                b'/' => lhs / rhs,
                b'%' => lhs % rhs,
                b'^' => lhs.powf(rhs),
                _ => lhs,
            };
            if !lhs.is_finite() {
                return Err(self.fail(ErrorCode::NonFinite, "non-finite arithmetic"));
            }
        }
        self.depth -= 1;
        Ok(lhs)
    }
    fn primary(&mut self) -> Result<f64, Error> {
        self.evaluation.step()?;
        if self.eat(b'-') {
            return self.expression(5).map(|x| -x);
        }
        if self.eat(b'+') {
            return self.expression(5);
        }
        if self.eat(b'(') {
            let x = self.expression(0)?;
            if !self.eat(b')') {
                return Err(self.fail(ErrorCode::Syntax, "expected closing parenthesis"));
            }
            return Ok(x);
        }
        self.space();
        if self
            .source
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
        {
            let start = self.at;
            while self
                .source
                .as_bytes()
                .get(self.at)
                .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
            {
                self.at += 1;
            }
            if self
                .source
                .as_bytes()
                .get(self.at)
                .is_some_and(|b| *b == b'e' || *b == b'E')
            {
                self.at += 1;
                if self
                    .source
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(|b| *b == b'+' || *b == b'-')
                {
                    self.at += 1;
                }
                while self
                    .source
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.at += 1;
                }
            }
            return self.source[start..self.at]
                .parse()
                .map_err(|_| self.fail(ErrorCode::Syntax, "invalid number"));
        }
        let name = self.ident().to_string();
        if name.is_empty() {
            return Err(self.fail(ErrorCode::Syntax, "expected number, variable or function"));
        }
        if !self.eat(b'(') {
            return match name.as_str() {
                "time" => Ok(self.evaluation.time as f64),
                "value" => Ok(self.base),
                "id" => Ok(self.shape as f64),
                "duration" => Ok(self.evaluation.motion.duration as f64),
                "fps" => Ok(self.evaluation.motion.fps as f64),
                "pi" => Ok(std::f64::consts::PI),
                "tau" => Ok(std::f64::consts::TAU),
                _ => Err(self.fail(ErrorCode::UnknownName, "unknown variable")),
            };
        }
        if name == "property" {
            let id = self.expression(0)?;
            if id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 || !self.eat(b',') {
                return Err(self.fail(
                    ErrorCode::InvalidArgument,
                    "property requires an exact id and property name",
                ));
            }
            let prop_name = self.ident().to_string();
            let prop: Prop = serde_json::from_value(serde_json::Value::String(prop_name))
                .map_err(|_| self.fail(ErrorCode::UnknownName, "unknown property"))?;
            if !self.eat(b')') || prop == Prop::Fill {
                return Err(self.fail(
                    ErrorCode::InvalidArgument,
                    "property requires a numeric channel",
                ));
            }
            return self
                .evaluation
                .value(id as u64, prop)?
                .map(|v| v as f64)
                .ok_or_else(|| {
                    self.fail(ErrorCode::UnknownName, "referenced channel has no value")
                });
        }
        let mut args = vec![];
        if !self.eat(b')') {
            loop {
                if args.len() >= 4 {
                    return Err(self.fail(ErrorCode::InvalidArgument, "at most four arguments"));
                }
                args.push(self.expression(0)?);
                if self.eat(b')') {
                    break;
                }
                if !self.eat(b',') {
                    return Err(self.fail(ErrorCode::Syntax, "expected comma"));
                }
            }
        }
        let value = match (name.as_str(), args.as_slice()) {
            ("sin", [x]) => x.sin(),
            ("cos", [x]) => x.cos(),
            ("tan", [x]) => x.tan(),
            ("abs", [x]) => x.abs(),
            ("sqrt", [x]) if *x >= 0.0 => x.sqrt(),
            ("floor", [x]) => x.floor(),
            ("ceil", [x]) => x.ceil(),
            ("round", [x]) => x.round(),
            ("min", [a, b]) => a.min(*b),
            ("max", [a, b]) => a.max(*b),
            ("clamp", [x, a, b]) if a <= b => x.clamp(*a, *b),
            ("lerp", [a, b, t]) => a + (b - a) * t,
            ("smoothstep", [a, b, x]) if a < b => {
                let t = ((x - a) / (b - a)).clamp(0., 1.);
                t * t * (3. - 2. * t)
            }
            ("random", [seed]) => random(seed.to_bits() ^ self.shape),
            ("noise", [x, seed]) => {
                let t = x.fract();
                let t = if t < 0. { t + 1. } else { t };
                let n = x.floor() as i64;
                let s = seed.to_bits() ^ self.shape;
                let a = random((n as u64) ^ s);
                let b = random((n.wrapping_add(1) as u64) ^ s);
                a + (b - a) * t * t * (3. - 2. * t)
            }
            _ => {
                return Err(self.fail(
                    ErrorCode::InvalidArgument,
                    "unknown function or invalid arguments",
                ));
            }
        };
        if !value.is_finite() {
            return Err(self.fail(ErrorCode::NonFinite, "function result is non-finite"));
        }
        Ok(value)
    }
}

fn random(mut seed: u64) -> f64 {
    seed = seed.wrapping_add(0x9e3779b97f4a7c15);
    seed = (seed ^ (seed >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    seed = (seed ^ (seed >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((seed ^ (seed >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn motion(source: &str) -> Motion {
        let mut m = Motion::default();
        m.set_key(1, Prop::X, 0., 10., super::super::Ease::Linear);
        m.expressions.push(Expression {
            shape: 1,
            prop: Prop::X,
            source: source.into(),
            enabled: true,
        });
        m
    }
    fn evaluate(m: &Motion) -> Result<Option<f32>, Error> {
        Evaluation::new(m, 0.5, &mut || true).value(1, Prop::X)
    }
    #[test]
    fn deterministic_numeric_expressions_and_property_reads() {
        let mut m = motion("value + sin(time*pi)*20 + random(42) + noise(time*4, 7)");
        assert_eq!(evaluate(&m), evaluate(&m));
        assert!(evaluate(&m).unwrap().unwrap() > 30.);
        m.set_key(2, Prop::Y, 0., 7., super::super::Ease::Linear);
        m.expressions[0].source = "property(2,Y)*3 - 2^3^2".into();
        assert_eq!(evaluate(&m).unwrap(), Some(-491.));
    }
    #[test]
    fn errors_and_cancellation_are_named_and_work_is_bounded() {
        for (s, code) in [
            ("1/0", ErrorCode::DivideByZero),
            ("time + missing", ErrorCode::UnknownName),
            ("sqrt(-1)", ErrorCode::InvalidArgument),
            ("property(1,X)", ErrorCode::Cycle),
            ("1e999", ErrorCode::NonFinite),
            ("while(true)", ErrorCode::UnknownName),
        ] {
            let e = evaluate(&motion(s)).unwrap_err();
            if s == "while(true)" {
                assert_eq!(e.code, ErrorCode::UnknownName);
            } else {
                assert_eq!(e.code, code, "{s}");
            }
        }
        assert_eq!(
            evaluate(&motion(&"(".repeat(100))).unwrap_err().code,
            ErrorCode::DepthLimit
        );
        assert_eq!(
            evaluate(&motion(&"1+".repeat(3000))).unwrap_err().code,
            ErrorCode::WorkLimit
        );
        let m = motion("time");
        assert_eq!(
            Evaluation::new(&m, 0., &mut || false)
                .value(1, Prop::X)
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        let mut keep = || true;
        let mut budget = Evaluation::new(&m, 0., &mut keep);
        budget.remaining = 1;
        assert_eq!(
            budget.value(1, Prop::X).unwrap_err().code,
            ErrorCode::WorkLimit
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        compositor,
        document::{Document, Layer},
        geom::Pt,
        layout, project,
    };
    #[test]
    fn save_reopen_preserves_source_keys_and_exported_pixels() {
        let mut doc = Document::new("Expressions", 64., 32., 72.);
        doc.layers.clear();
        doc.artboards.clear();
        doc.artboardless = true;
        doc.transparent = true;
        let mut layer = Layer::vector("Animated");
        let shape = layout::make_frame(Pt::new(1., 1.), Pt::new(8., 8.));
        let id = shape.id;
        layer.kind.shapes_mut().unwrap().push(shape);
        doc.layers.push(layer);
        doc.motion
            .set_key(id, Prop::X, 0., 0., crate::motion::Ease::Linear);
        doc.motion
            .set_key(id, Prop::X, 1., 8., crate::motion::Ease::Linear);
        doc.motion
            .set_expression(id, Prop::X, Some("value + time * 20".into()))
            .unwrap();
        let keys = doc.motion.tracks.clone();
        let reopened = project::decode(&project::encode(&doc).unwrap()).unwrap();
        assert_eq!(reopened.motion, doc.motion);
        assert_eq!(reopened.motion.tracks, keys);
        let a = compositor::render_export_at(&reopened, 1., 0., true).unwrap();
        let b = compositor::render_export_at(&reopened, 1., 1., true).unwrap();
        assert_ne!(a.data(), b.data());
        let mut polls = 0;
        let e = compositor::render_export_at_cancellable(&reopened, 1., 1., true, &mut || {
            polls += 1;
            polls < 5
        })
        .unwrap_err();
        assert!(e.contains("Cancelled"));
        doc.motion
            .set_expression(id, Prop::X, Some("1/0".into()))
            .unwrap();
        assert!(
            compositor::render_export_at(&doc, 1., 0., true)
                .unwrap_err()
                .contains("DivideByZero")
        );
        let old: Motion =
            serde_json::from_str(r#"{"duration":2,"fps":30,"looped":true,"tracks":[]}"#).unwrap();
        assert!(old.expressions.is_empty());
    }
}
