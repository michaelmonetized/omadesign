//! Operand values, and the dictionaries the editing data writes postfix: `/Type :` opens one,
//! `values key ,` adds an entry (a type name such as `/Real` may stand before the key; in an
//! `/Array` there is no key), `;` closes it (after a last `values /key` entry, or a bare value: its
//! content), leaving it as a value.

use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum V {
    Num(f64),
    Str(Vec<u8>),
    Name(String),
    Arr(Vec<V>),
    Obj(Rc<Obj>),
    /// `[`, until its `]`.
    Mark,
}

impl V {
    pub fn num(&self) -> Option<f64> {
        match self {
            V::Num(v) => Some(*v),
            _ => None,
        }
    }
    pub fn obj(&self) -> Option<&Obj> {
        match self {
            V::Obj(o) => Some(o),
            _ => None,
        }
    }
    /// A string's or name's text.
    pub fn text(&self) -> Option<String> {
        match self {
            V::Str(s) => Some(text(s)),
            V::Name(n) => Some(n.clone()),
            _ => None,
        }
    }
}

/// Text from the data's bytes: UTF-8, else Windows-1252 (Latin-1 for its gaps).
pub(super) fn text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|b| cp1252(*b)).collect(),
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9f => HIGH.get(usize::from(b - 0x80)).copied().unwrap_or('?'),
        _ => char::from(b),
    }
}

/// A dictionary.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Obj {
    pub ty: String,
    /// Entries in order: the key (`None` in an array) and the values before it.
    pub entries: Vec<(Option<String>, Vec<V>)>,
    /// Values closed without a key.
    pub content: Vec<V>,
}

impl Obj {
    /// The values of entry `key` (the first one).
    pub fn get(&self, key: &str) -> Option<&[V]> {
        self.entries
            .iter()
            .find(|(k, _)| k.as_deref() == Some(key))
            .map(|(_, v)| v.as_slice())
    }
    /// The numbers of entry `key`.
    pub fn nums(&self, key: &str) -> Vec<f64> {
        self.get(key)
            .unwrap_or_default()
            .iter()
            .filter_map(V::num)
            .collect()
    }
    /// The first text value of entry `key`.
    pub fn text(&self, key: &str) -> Option<String> {
        self.get(key)?
            .iter()
            .find(|v| matches!(v, V::Str(_)))
            .and_then(V::text)
    }
    /// The dictionary value of entry `key`.
    pub fn obj(&self, key: &str) -> Option<&Obj> {
        self.get(key)?.iter().find_map(V::obj)
    }
    /// The dictionaries an array holds, in order.
    pub fn items(&self) -> impl Iterator<Item = &Obj> {
        self.entries
            .iter()
            .flat_map(|(_, v)| v.iter())
            .chain(self.content.iter())
            .filter_map(V::obj)
    }
    /// The first text of the content.
    pub fn content_text(&self) -> Option<String> {
        self.content.iter().find_map(V::text)
    }
}

/// Add the values on `stack` above `base` to `o` as one entry (`,`): the last is the key, except
/// in an array.
pub(super) fn add_entry(o: &mut Obj, stack: &mut Vec<V>, base: usize) {
    let mut vals: Vec<V> = stack.drain(base.min(stack.len())..).collect();
    if o.ty == "Array" {
        o.entries.push((None, vals));
        return;
    }
    let key = match vals.last() {
        Some(V::Str(_) | V::Name(_)) => vals.pop().and_then(|k| k.text()),
        _ => None,
    };
    o.entries.push((key, vals));
}

/// Close `o` (`;`): a last entry ending with a name is an entry, other values are its content.
pub(super) fn close(o: &mut Obj, stack: &mut Vec<V>, base: usize) {
    if stack.len() <= base {
        return;
    }
    if o.ty != "Array" && matches!(stack.last(), Some(V::Name(_))) {
        add_entry(o, stack, base);
    } else {
        let vals: Vec<V> = stack.drain(base.min(stack.len())..).collect();
        o.content.extend(vals);
    }
}
