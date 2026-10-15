//! Bounded Photoshop ActionDescriptors shared by type and placed-layer interchange.
use super::codec::Reader;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Descriptor {
    pub name: String,
    pub class: String,
    pub fields: Vec<(String, Value)>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Object(Descriptor),
    List(Vec<Value>),
    Text(String),
    Integer(i32),
    Large(i64),
    Double(f64),
    Unit { unit: [u8; 4], value: f64 },
    Bool(bool),
    Enum { class: String, value: String },
    Bytes { kind: [u8; 4], value: Vec<u8> },
    Class { name: String, class: String },
}
impl Descriptor {
    pub fn new(class: &str) -> Self {
        Self {
            class: class.into(),
            ..Default::default()
        }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn set(&mut self, key: &str, value: Value) {
        if let Some((_, v)) = self.fields.iter_mut().find(|(k, _)| k == key) {
            *v = value;
        } else {
            self.fields.push((key.into(), value));
        }
    }
    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.set(key, value);
        self
    }
}
impl Value {
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Integer(v) => Some(*v as f64),
            Self::Large(v) => Some(*v as f64),
            Self::Double(v) | Self::Unit { value: v, .. } => Some(*v),
            _ => None,
        }
    }
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(v) => Some(v),
            Self::Enum { value, .. } => Some(value),
            _ => None,
        }
    }
    pub fn object(&self) -> Option<&Descriptor> {
        if let Self::Object(d) = self {
            Some(d)
        } else {
            None
        }
    }
    pub fn list(&self) -> Option<&[Value]> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
fn unicode(r: &mut Reader<'_>) -> Result<String, String> {
    let n = r.u32()? as usize;
    if n > 1_048_576 || n > r.remaining() / 2 {
        return Err("Excessive or truncated Photoshop descriptor string".into());
    }
    let values = (0..n).map(|_| r.u16()).collect::<Result<Vec<_>, _>>()?;
    Ok(String::from_utf16_lossy(&values)
        .trim_end_matches('\0')
        .to_string())
}
fn id(r: &mut Reader<'_>) -> Result<String, String> {
    let n = r.u32()? as usize;
    let n = if n == 0 { 4 } else { n };
    if n > 4096 {
        return Err("Excessive Photoshop descriptor key".into());
    }
    Ok(String::from_utf8_lossy(r.take(n)?).into_owned())
}
fn number(r: &mut Reader<'_>) -> Result<f64, String> {
    let v = f64::from_be_bytes(r.take(8)?.try_into().unwrap());
    if v.is_finite() {
        Ok(v)
    } else {
        Err("Nonfinite Photoshop descriptor value".into())
    }
}
/// Read one versioned ActionDescriptor at the current cursor.
/// Args: bounded Photoshop reader. Returns its typed fields or a malformed-data error.
pub fn read_versioned(r: &mut Reader<'_>) -> Result<Descriptor, String> {
    if r.u32()? != 16 {
        return Err("Unsupported Photoshop descriptor version".into());
    }
    read(r, 0)
}
fn read(r: &mut Reader<'_>, depth: usize) -> Result<Descriptor, String> {
    if depth > 32 {
        return Err("Excessive Photoshop descriptor nesting".into());
    }
    let name = unicode(r)?;
    let class = id(r)?;
    let n = r.u32()? as usize;
    if n > 8192 {
        return Err("Excessive Photoshop descriptor fields".into());
    }
    let mut fields = Vec::with_capacity(n);
    for _ in 0..n {
        let key = id(r)?;
        let kind = r.key()?;
        fields.push((key, value(r, kind, depth + 1)?));
    }
    Ok(Descriptor {
        name,
        class,
        fields,
    })
}
fn value(r: &mut Reader<'_>, kind: [u8; 4], depth: usize) -> Result<Value, String> {
    if depth > 32 {
        return Err("Excessive Photoshop descriptor nesting".into());
    }
    Ok(match &kind {
        b"Objc" | b"GlbO" => Value::Object(read(r, depth)?),
        b"VlLs" => {
            let n = r.u32()? as usize;
            if n > 65536 {
                return Err("Excessive descriptor list".into());
            }
            let mut values = Vec::with_capacity(n);
            for _ in 0..n {
                let kind = r.key()?;
                values.push(value(r, kind, depth + 1)?);
            }
            Value::List(values)
        }
        b"TEXT" => Value::Text(unicode(r)?),
        b"long" => Value::Integer(r.i32()?),
        b"comp" => Value::Large(r.u64()? as i64),
        b"doub" => Value::Double(number(r)?),
        b"UntF" => Value::Unit {
            unit: r.key()?,
            value: number(r)?,
        },
        b"bool" => Value::Bool(r.u8()? != 0),
        b"enum" => Value::Enum {
            class: id(r)?,
            value: id(r)?,
        },
        b"tdta" | b"alis" | b"Pth " => {
            let n = r.u32()? as usize;
            if n > 16 * 1024 * 1024 {
                return Err("Excessive descriptor payload".into());
            }
            Value::Bytes {
                kind,
                value: r.take(n)?.to_vec(),
            }
        }
        b"type" | b"GlbC" => Value::Class {
            name: unicode(r)?,
            class: id(r)?,
        },
        _ => {
            return Err(format!(
                "Unsupported Photoshop descriptor type {}",
                String::from_utf8_lossy(&kind)
            ));
        }
    })
}
fn write_unicode(out: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
}
fn write_id(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(
        &(if value.len() == 4 {
            0
        } else {
            value.len() as u32
        })
        .to_be_bytes(),
    );
    out.extend_from_slice(value.as_bytes());
}
/// Encode a versioned ActionDescriptor.
/// Args: typed descriptor and output bytes. Returns after appending its descriptor version and fields.
pub fn write_versioned(out: &mut Vec<u8>, d: &Descriptor) {
    out.extend_from_slice(&16u32.to_be_bytes());
    write(out, d);
}
fn write(out: &mut Vec<u8>, d: &Descriptor) {
    write_unicode(out, &d.name);
    write_id(out, &d.class);
    out.extend_from_slice(&(d.fields.len() as u32).to_be_bytes());
    for (key, value) in &d.fields {
        write_id(out, key);
        write_value(out, value);
    }
}
fn write_value(out: &mut Vec<u8>, value: &Value) {
    out.extend_from_slice(match value {
        Value::Object(_) => b"Objc",
        Value::List(_) => b"VlLs",
        Value::Text(_) => b"TEXT",
        Value::Integer(_) => b"long",
        Value::Large(_) => b"comp",
        Value::Double(_) => b"doub",
        Value::Unit { .. } => b"UntF",
        Value::Bool(_) => b"bool",
        Value::Enum { .. } => b"enum",
        Value::Bytes { kind, .. } => kind,
        Value::Class { .. } => b"type",
    });
    match value {
        Value::Object(d) => write(out, d),
        Value::List(values) => {
            out.extend_from_slice(&(values.len() as u32).to_be_bytes());
            for v in values {
                write_value(out, v);
            }
        }
        Value::Text(v) => write_unicode(out, v),
        Value::Integer(v) => out.extend_from_slice(&v.to_be_bytes()),
        Value::Large(v) => out.extend_from_slice(&v.to_be_bytes()),
        Value::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Value::Unit { unit, value } => {
            out.extend_from_slice(unit);
            out.extend_from_slice(&value.to_be_bytes());
        }
        Value::Bool(v) => out.push(u8::from(*v)),
        Value::Enum { class, value } => {
            write_id(out, class);
            write_id(out, value);
        }
        Value::Bytes { value, .. } => {
            out.extend_from_slice(&(value.len() as u32).to_be_bytes());
            out.extend_from_slice(value);
        }
        Value::Class { name, class } => {
            write_unicode(out, name);
            write_id(out, class);
        }
    }
}
