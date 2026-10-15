//! Portable brush packs and persistent user libraries.
mod abr;
mod descriptor;
mod illustrator;

use crate::{geom::Pt, paint::Brush};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_FILE: usize = 128 * 1024 * 1024;
const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_BRUSHES: usize = 2048;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tip {
    pub width: u32,
    pub height: u32,
    #[serde(with = "alpha_bytes")]
    pub alpha: Vec<u8>,
}

mod alpha_bytes {
    use super::*;
    pub fn serialize<S: serde::Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(d)?;
        if encoded.len() > MAX_PIXELS * 4 / 3 + 4 {
            return Err(serde::de::Error::custom("Brush tip exceeds 16 megapixels"));
        }
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub size: f32,
    pub spacing: f32,
    pub hardness: f32,
    pub flow: f32,
    pub opacity: f32,
    pub tip: Arc<Tip>,
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub scatter: f32,
    #[serde(default)]
    pub angle_jitter: f32,
    #[serde(default)]
    pub size_jitter: f32,
    #[serde(default)]
    pub opacity_jitter: f32,
    #[serde(default)]
    pub follow_direction: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub notes: Vec<String>,
    pub brushes: Vec<Preset>,
}

impl Pack {
    pub fn new(name: &str) -> Self {
        Self {
            format: "omadesign-brush-pack".into(),
            version: 1,
            name: name.into(),
            author: String::new(),
            license: "Private import; check the source license before sharing".into(),
            notes: vec![],
            brushes: vec![],
        }
    }

    /// Validate a pack before it enters the library.
    /// Accepts this pack and returns an error for invalid settings or excessive memory.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "omadesign-brush-pack" || self.version != 1 {
            return Err("Unsupported Omadesign brush pack version".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.brushes.is_empty()
            || self.brushes.len() > MAX_BRUSHES
        {
            return Err("A brush pack needs a name and 1–2048 brushes".into());
        }
        let mut total = 0usize;
        for p in &self.brushes {
            let t = &p.tip;
            let pixels = u64::from(t.width) * u64::from(t.height);
            if t.width == 0
                || t.height == 0
                || t.width > 8192
                || t.height > 8192
                || pixels > MAX_PIXELS as u64
                || pixels != t.alpha.len() as u64
                || !t.alpha.iter().any(|&v| v > 0)
            {
                return Err(format!("{} has an empty or invalid brush tip", p.name));
            }
            total += t.alpha.len();
            if !p.angle.is_finite()
                || p.angle.abs() > std::f32::consts::TAU
                || !p.scatter.is_finite()
                || !(0.0..=10.0).contains(&p.scatter)
                || !p.angle_jitter.is_finite()
                || !(0.0..=std::f32::consts::PI).contains(&p.angle_jitter)
                || [p.size_jitter, p.opacity_jitter]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || total > MAX_FILE
                || p.name.trim().is_empty()
                || p.name.len() > 512
                || !p.size.is_finite()
                || !(1.0..=2048.0).contains(&p.size)
                || !p.spacing.is_finite()
                || !(0.01..=100.0).contains(&p.spacing)
                || [p.hardness, p.flow, p.opacity]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("Invalid brush settings or pack exceeds 128 MiB of tips".into());
            }
        }
        Ok(())
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_FILE {
            return Err("Pack exceeds 128 MiB".into());
        }
        Ok(bytes)
    }
}

impl Preset {
    pub fn new(name: String, tip: Tip, spacing: f32) -> Self {
        Self {
            name,
            size: tip.width.max(tip.height).min(256).max(1) as f32,
            spacing: spacing.clamp(0.01, 100.0),
            hardness: 1.0,
            flow: 1.0,
            opacity: 1.0,
            tip: Arc::new(tip),
            angle: 0.0,
            scatter: 0.0,
            angle_jitter: 0.0,
            size_jitter: 0.0,
            opacity_jitter: 0.0,
            follow_direction: false,
        }
    }
    pub fn apply(&self, brush: &mut Brush) {
        brush.size = self.size;
        brush.spacing = self.spacing;
        brush.hardness = self.hardness;
        brush.flow = self.flow;
        brush.opacity = self.opacity;
        brush.angle = self.angle;
        brush.scatter = self.scatter;
        brush.angle_jitter = self.angle_jitter;
        brush.size_jitter = self.size_jitter;
        brush.opacity_jitter = self.opacity_jitter;
        brush.follow_direction = self.follow_direction;
    }
}

pub type ImportJob = Arc<std::sync::Mutex<Option<Result<(Pack, String), String>>>>;

struct CursorOutline {
    tip: Arc<Tip>,
    edges: Arc<Vec<(Pt, Pt)>>,
}

pub struct Library {
    pub packs: Vec<Pack>,
    pub selected: Option<(usize, usize)>,
    pub errors: Vec<String>,
    pub(crate) directory: PathBuf,
    fingerprints: Vec<String>,
    outline_cache: std::cell::RefCell<Option<CursorOutline>>,
}

impl Library {
    pub fn load(directory: PathBuf) -> Self {
        let mut library = Self {
            packs: vec![],
            selected: None,
            errors: vec![],
            directory,
            fingerprints: vec![],
            outline_cache: Default::default(),
        };
        for bytes in [
            include_bytes!("../assets/brushes/essentials.omabrush").as_slice(),
            include_bytes!("../assets/brushes/texture.omabrush").as_slice(),
        ] {
            match decode(bytes) {
                Ok(pack) => {
                    let hash = format!("{:x}", Sha256::digest(pack.bytes().unwrap()));
                    library.accept(pack, hash);
                }
                Err(e) => library.errors.push(e),
            }
        }
        if let Ok(entries) = std::fs::read_dir(&library.directory) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_pack(p))
                .collect();
            paths.sort();
            for path in paths {
                match import(&path) {
                    Ok(pack) => library.packs.push(pack),
                    Err(e) => library.errors.push(format!("{}: {e}", path.display())),
                }
            }
        }
        library
    }

    pub fn tip(&self) -> Option<&Tip> {
        let (pack, brush) = self.selected?;
        Some(&self.packs.get(pack)?.brushes.get(brush)?.tip)
    }

    /// Reuse the selected tip's cursor boundary between canvas frames.
    /// Takes this library and returns its outline, or none for a round brush.
    pub fn tip_outline(&self) -> Option<Arc<Vec<(Pt, Pt)>>> {
        let (pack, brush) = self.selected?;
        let tip = &self.packs.get(pack)?.brushes.get(brush)?.tip;
        let mut cached = self.outline_cache.borrow_mut();
        if let Some(outline) = &*cached
            && Arc::ptr_eq(&outline.tip, tip)
        {
            return Some(outline.edges.clone());
        }
        let outline = Arc::new(outline(tip));
        *cached = Some(CursorOutline { tip: tip.clone(), edges: outline.clone() });
        Some(outline)
    }

    /// Install a validated pack without replacing another user's file.
    /// Accepts a pack and returns its library index; identical imports reuse the existing pack.
    pub fn install(&mut self, pack: Pack) -> Result<usize, String> {
        let hash = Self::persist(&self.directory, &pack)?;
        Ok(self.accept(pack, hash))
    }

    pub(crate) fn persist(directory: &Path, pack: &Pack) -> Result<String, String> {
        let bytes = pack.bytes()?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!("{hash}.omabrush"));
        if !path.exists() {
            crate::formats::write_atomic(&path, &bytes)?;
        }
        Ok(hash)
    }

    pub(crate) fn accept(&mut self, pack: Pack, hash: String) -> usize {
        if let Some(index) = self.fingerprints.iter().position(|h| h == &hash) {
            return index;
        }
        self.fingerprints.push(hash);
        self.packs.push(pack);
        self.packs.len() - 1
    }
}

pub fn is_pack(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("omabrush"))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE as u64 {
        return Err("Choose a brush file smaller than 128 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("Brush file exceeds 128 MiB".into());
    }
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<Pack, String> {
    if bytes.len() > MAX_FILE {
        return Err("Brush file exceeds 128 MiB".into());
    }
    let pack: Pack =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid brush pack: {e}"))?;
    pack.validate()?;
    Ok(pack)
}

pub fn import(path: &Path) -> Result<Pack, String> {
    let bytes = read(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported brushes");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let pack = match ext.as_str() {
        "omabrush" => decode(&bytes)?,
        "abr" => abr::decode(&bytes, name)?,
        "ai" => illustrator::decode(&bytes, name)?,
        "svg" | "eps" | "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff" => artwork(path, name)?,
        _ => {
            return Err(
                "Import .omabrush, Photoshop .abr, Illustrator .ai, SVG, or a brush-tip image"
                    .into(),
            );
        }
    };
    pack.validate()?;
    Ok(pack)
}

fn artwork(path: &Path, name: &str) -> Result<Pack, String> {
    let tip = match crate::import::open_any(path)? {
        crate::import::Imported::Raster { image, .. } => {
            tip_from_rgba(image.w, image.h, &image.data)?
        }
        crate::import::Imported::Document(mut doc) => {
            doc.transparent = true;
            doc.artboards.clear();
            let scale = (512.0 / doc.width.max(doc.height).max(1.0)).min(1.0);
            let pm = crate::compositor::render_export_at(&doc, scale, 0.0, true)?;
            let pixels = crate::document::Pixels::from_pixmap(&pm)?;
            tip_from_rgba(pixels.w, pixels.h, &pixels.data)?
        }
        _ => return Err("Use a static image or vector brush tip".into()),
    };
    let mut pack = Pack::new(name);
    pack.brushes.push(Preset::new(name.into(), tip, 0.15));
    Ok(pack)
}

pub fn tip_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Result<Tip, String> {
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > MAX_PIXELS as u64
        || rgba.len() != w as usize * h as usize * 4
    {
        return Err("Brush image exceeds 16 megapixels or has invalid pixels".into());
    }
    let transparent = rgba.chunks_exact(4).any(|p| p[3] < 255);
    let alpha = rgba
        .chunks_exact(4)
        .map(|p| {
            if transparent {
                p[3]
            } else {
                255 - ((u32::from(p[0]) * 54 + u32::from(p[1]) * 183 + u32::from(p[2]) * 19) / 256)
                    as u8
            }
        })
        .collect();
    crop(Tip {
        width: w,
        height: h,
        alpha,
    })
}

fn crop(tip: Tip) -> Result<Tip, String> {
    let mut x0 = tip.width;
    let mut y0 = tip.height;
    let (mut x1, mut y1) = (0, 0);
    for (i, &a) in tip.alpha.iter().enumerate() {
        if a > 0 {
            let x = i as u32 % tip.width;
            let y = i as u32 / tip.width;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return Err("Brush artwork is empty".into());
    }
    let mut alpha = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize);
    for y in y0..y1 {
        alpha.extend_from_slice(
            &tip.alpha[(y * tip.width + x0) as usize..(y * tip.width + x1) as usize],
        );
    }
    Ok(Tip {
        width: x1 - x0,
        height: y1 - y0,
        alpha,
    })
}

/// Paint a sampled tip in the current foreground color.
/// Arguments are the target, center, brush settings, tip, and erase flag; pixels are blended in place.
pub fn stamp(pm: &mut tiny_skia::Pixmap, pos: Pt, brush: &Brush, tip: &Tip, erase: bool) {
    let (pos, brush) = brush.dab(pos);
    let brush = &brush;
    let scale = brush.size.max(1.0) / tip.width.max(tip.height) as f32;
    let (w, h) = (tip.width as f32 * scale, tip.height as f32 * scale);
    let radius = (w * w + h * h).sqrt() * 0.5;
    let (x0, y0) = (
        (pos.x - radius).clamp(0.0, pm.width() as f32) as u32,
        (pos.y - radius).clamp(0.0, pm.height() as f32) as u32,
    );
    let (x1, y1) = (
        (pos.x + radius).ceil().clamp(0.0, pm.width() as f32) as u32,
        (pos.y + radius).ceil().clamp(0.0, pm.height() as f32) as u32,
    );
    let width = pm.width();
    for y in y0..y1 {
        for x in x0..x1 {
            let local = (Pt::new(x as f32 + 0.5, y as f32 + 0.5) - pos).rotate(-brush.angle);
            let sx = (local.x + w * 0.5) / scale - 0.5;
            let sy = (local.y + h * 0.5) / scale - 0.5;
            let sample = |ix: i32, iy: i32| {
                if ix < 0 || iy < 0 || ix >= tip.width as i32 || iy >= tip.height as i32 {
                    0.0
                } else {
                    tip.alpha[iy as usize * tip.width as usize + ix as usize] as f32 / 255.0
                }
            };
            let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - sx.floor(), sy - sy.floor());
            let coverage = (sample(ix, iy) * (1.0 - fx) + sample(ix + 1, iy) * fx) * (1.0 - fy)
                + (sample(ix, iy + 1) * (1.0 - fx) + sample(ix + 1, iy + 1) * fx) * fy;
            let alpha = (coverage
                * brush.flow
                * brush.opacity
                * if erase {
                    1.0
                } else {
                    brush.color.a as f32 / 255.0
                })
            .clamp(0.0, 1.0);
            if alpha == 0.0 {
                continue;
            }
            let dest = &mut pm.pixels_mut()[(y * width + x) as usize];
            let remain = 1.0 - alpha;
            let channel = |old: u8, color: u8| {
                (old as f32 * remain + if erase { 0.0 } else { color as f32 * alpha })
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            *dest = tiny_skia::PremultipliedColorU8::from_rgba(
                channel(dest.red(), brush.color.r),
                channel(dest.green(), brush.color.g),
                channel(dest.blue(), brush.color.b),
                (dest.alpha() as f32 * remain + if erase { 0.0 } else { alpha * 255.0 })
                    .round()
                    .clamp(0.0, 255.0) as u8,
            )
            .unwrap();
        }
    }
}

pub fn stroke(pm: &mut tiny_skia::Pixmap, from: Pt, to: Pt, brush: &Brush, tip: &Tip, erase: bool) {
    let mut brush = *brush;
    if brush.follow_direction && (to - from).length() > 0.001 {
        brush.angle += (to - from).y.atan2((to - from).x);
    }
    let brush = &brush;
    let distance = (to - from).length();
    let spacing = brush.spacing_distance();
    let count = (distance / spacing).ceil().max(1.0) as u32;
    for i in 1..=count {
        stamp(
            pm,
            from.lerp(to, i as f32 / count as f32),
            brush,
            tip,
            erase,
        );
    }
}

/// Stamp a segment while carrying spacing across pointer events.
/// Takes the target, segment, brush, tip, erase flag and remaining distance; updates pixels and distance.
pub fn stroke_spaced(
    pm: &mut tiny_skia::Pixmap,
    from: Pt,
    to: Pt,
    brush: &Brush,
    tip: &Tip,
    erase: bool,
    remaining: &mut f32,
) {
    crate::paint::spaced_dabs(from, to, brush, remaining, |at, brush| {
        stamp(pm, at, brush, tip, erase)
    });
}

/// Trace a sampled tip's visible edge with bounded cursor work.
/// Takes a tip and returns boundary segments in coordinates centered on its longest side.
pub fn outline(tip: &Tip) -> Vec<(Pt, Pt)> {
    let step = tip.width.max(tip.height).div_ceil(128).max(1);
    let w = tip.width.div_ceil(step);
    let h = tip.height.div_ceil(step);
    let mut mask = vec![false; (w * h) as usize];
    for y in 0..h { for x in 0..w {
        mask[(y * w + x) as usize] = (y * step..((y + 1) * step).min(tip.height)).any(|sy| {
            let start = (sy * tip.width + x * step) as usize;
            let end = (sy * tip.width + ((x + 1) * step).min(tip.width)) as usize;
            tip.alpha[start..end].iter().any(|&alpha| alpha >= 32)
        });
    }}
    let filled = |x: i32, y: i32| x >= 0 && y >= 0 && x < w as i32 && y < h as i32 && mask[(y as u32 * w + x as u32) as usize];
    let scale = tip.width.max(tip.height) as f32;
    let point = |x: u32, y: u32| {
        Pt::new(
            (x * step).min(tip.width) as f32 - tip.width as f32 * 0.5,
            (y * step).min(tip.height) as f32 - tip.height as f32 * 0.5,
        ) / scale
    };
    let mut edges = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !filled(x as i32, y as i32) {
                continue;
            }
            for (dx, dy, a, b) in [
                (0, -1, (x, y), (x + 1, y)),
                (1, 0, (x + 1, y), (x + 1, y + 1)),
                (0, 1, (x + 1, y + 1), (x, y + 1)),
                (-1, 0, (x, y + 1), (x, y)),
            ] {
                if !filled(x as i32 + dx, y as i32 + dy) {
                    edges.push((point(a.0, a.1), point(b.0, b.1)));
                }
            }
        }
    }
    edges
}

pub fn cli(args: &[String]) -> Option<Result<(), String>> {
    if args.first().is_none_or(|a| a != "--convert-brushes") {
        return None;
    }
    Some((|| {
        if args.len() != 3 {
            return Err("Use --convert-brushes INPUT OUTPUT.omabrush".into());
        }
        let pack = import(Path::new(&args[1]))?;
        crate::formats::write_atomic(Path::new(&args[2]), &pack.bytes()?)?;
        println!("{}: {} brushes", pack.name, pack.brushes.len());
        for note in pack.notes {
            println!("{note}");
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_outline_retains_thin_tips_and_reuses_the_selected_boundary() {
        let mut alpha = vec![0; 256 * 256];
        for y in 0..256 { alpha[y * 256] = 255; }
        assert!(!outline(&Tip { width: 256, height: 256, alpha }).is_empty());
        let mut library = Library::load(root());
        library.selected = Some((0, 0));
        let first = library.tip_outline().unwrap();
        assert!(Arc::ptr_eq(&first, &library.tip_outline().unwrap()));
        library.selected = Some((0, 6));
        assert!(!Arc::ptr_eq(&first, &library.tip_outline().unwrap()));
    }
    fn root() -> PathBuf {
        let root = std::env::current_dir()
            .unwrap()
            .join("artifacts/brush-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn square_outline_rotation_pressure_and_spacing_match_stamped_pixels() {
        let tip = Tip {
            width: 8,
            height: 2,
            alpha: vec![255; 16],
        };
        let brush = Brush {
            size: 16.,
            angle: std::f32::consts::FRAC_PI_2,
            flow: 1.,
            ..Default::default()
        };
        let mut pm = tiny_skia::Pixmap::new(64, 64).unwrap();
        stamp(&mut pm, Pt::new(32., 32.), &brush, &tip, false);
        assert!(pm.pixel(32, 38).unwrap().alpha() > 0);
        assert_eq!(pm.pixel(38, 32).unwrap().alpha(), 0);
        let outline = outline(&Tip {
            width: 4,
            height: 4,
            alpha: vec![255; 16],
        });
        assert_eq!(outline.len(), 16);
        assert!(
            outline
                .iter()
                .all(|(a, b)| a.x.abs() == 0.5 && b.x.abs() == 0.5
                    || a.y.abs() == 0.5 && b.y.abs() == 0.5)
        );
        let (position, small) = Brush {
            pressure: 0.25,
            pressure_size: true,
            pressure_flow: true,
            ..brush
        }
        .dab(Pt::new(32., 32.));
        assert_eq!(position, Pt::new(32., 32.));
        assert_eq!(small.size, 4.);
        assert_eq!(small.flow, 0.25);
        let tip = Tip {
            width: 2,
            height: 2,
            alpha: vec![255; 4],
        };
        let brush = Brush {
            size: 2.,
            spacing: 10.,
            flow: 1.,
            ..Default::default()
        };
        let mut pm = tiny_skia::Pixmap::new(64, 16).unwrap();
        let mut remaining = 20.;
        for x in 1..=40 {
            stroke_spaced(
                &mut pm,
                Pt::new(x as f32 - 1., 8.),
                Pt::new(x as f32, 8.),
                &brush,
                &tip,
                false,
                &mut remaining,
            );
        }
        assert!(pm.pixel(20, 8).unwrap().alpha() > 0);
        assert_eq!(pm.pixel(10, 8).unwrap().alpha(), 0);
        assert!(pm.pixel(40, 8).unwrap().alpha() > 0);
    }

    #[test]
    fn dynamics_roundtrip_and_scatter_stay_inside_affected_region() {
        let mut pack = decode(include_bytes!("../assets/brushes/essentials.omabrush")).unwrap();
        let square = pack.brushes.len() - 1;
        let preset = &mut pack.brushes[square];
        assert_eq!(preset.name, "Square");
        preset.spacing = 10.;
        preset.angle = 1.;
        preset.scatter = 3.;
        preset.angle_jitter = 2.;
        preset.size_jitter = 0.7;
        preset.opacity_jitter = 0.8;
        preset.follow_direction = true;
        let decoded = decode(&pack.bytes().unwrap()).unwrap();
        let mut brush = Brush::default();
        decoded.brushes[square].apply(&mut brush);
        assert_eq!(brush.spacing, 10.);
        assert_eq!(brush.angle, 1.);
        assert!(brush.follow_direction);
        let center = Pt::new(128., 128.);
        let region = crate::paint::PixelRegion::stroke(256, 256, center, center, &brush);
        for x in 100..150 {
            let (p, dab) = brush.dab(Pt::new(x as f32, 128.));
            assert!((p - Pt::new(x as f32, 128.)).length() <= brush.size * brush.scatter);
            assert!(dab.size >= brush.size * 0.3 && dab.size <= brush.size);
            assert!(dab.flow >= brush.flow * 0.2 && dab.flow <= brush.flow);
        }
        let mut pm = tiny_skia::Pixmap::new(256, 256).unwrap();
        stamp(&mut pm, center, &brush, &decoded.brushes[square].tip, false);
        for y in 0..256 {
            for x in 0..256 {
                if pm.pixel(x, y).unwrap().alpha() > 0 {
                    assert!(
                        x as usize >= region.x0
                            && (x as usize) < region.x1
                            && y as usize >= region.y0
                            && (y as usize) < region.y1
                    );
                }
            }
        }
    }

    #[test]
    fn imported_pack_survives_reload_and_duplicate_imports_preserve_files() {
        let directory = root();
        let pack = import(Path::new("assets/brushes/texture.omabrush")).unwrap();
        let mut library = Library::load(directory.clone());
        let index = library.install(pack.clone()).unwrap();
        assert_eq!(library.install(pack.clone()).unwrap(), index);
        let reloaded = Library::load(directory.clone());
        assert_eq!(
            reloaded.packs[index].bytes().unwrap(),
            pack.bytes().unwrap()
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn malformed_packs_do_not_install_or_replace_existing_brushes() {
        let directory = root();
        let mut library = Library::load(directory.clone());
        let mut pack = library.packs[0].clone();
        pack.brushes[0].tip = Arc::new(Tip {
            width: u32::MAX,
            height: u32::MAX,
            alpha: vec![255],
        });
        assert!(library.install(pack).is_err());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        for bad in [
            br#"{"format":"omadesign-brush-pack","version":500}"#.as_slice(),
            b"not json",
        ] {
            assert!(decode(bad).is_err());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn artwork_uses_transparency_or_dark_ink_and_crops_blank_edges() {
        let tip = tip_from_rgba(
            3,
            1,
            &[255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255],
        )
        .unwrap();
        assert_eq!(
            tip,
            Tip {
                width: 1,
                height: 1,
                alpha: vec![255]
            }
        );
        let tip = tip_from_rgba(2, 1, &[255, 0, 0, 0, 255, 255, 255, 128]).unwrap();
        assert_eq!(tip.alpha, [128]);
        assert!(tip_from_rgba(1, 1, &[255, 255, 255, 255]).is_err());
    }
    #[test]
    fn sampled_color_and_erase_match_coverage_and_keep_pixels_outside_tip() {
        let tip = Tip {
            width: 2,
            height: 2,
            alpha: vec![255, 0, 128, 0],
        };
        let mut pm = tiny_skia::Pixmap::new(8, 8).unwrap();
        let brush = Brush {
            size: 2.0,
            flow: 1.0,
            opacity: 1.0,
            color: crate::color::Rgba::rgb(240, 20, 40),
            ..Default::default()
        };
        stamp(&mut pm, Pt::new(4.0, 4.0), &brush, &tip, false);
        assert_eq!(pm.pixel(3, 3).unwrap().demultiply().red(), 240);
        assert_eq!(pm.pixel(3, 4).unwrap().alpha(), 128);
        assert_eq!(pm.pixel(4, 3).unwrap().alpha(), 0);
        let before = pm.clone();
        stamp(&mut pm, Pt::new(4.0, 4.0), &brush, &tip, true);
        assert_eq!(pm.pixel(3, 3).unwrap().alpha(), 0);
        assert!(pm.pixel(3, 4).unwrap().alpha() < before.pixel(3, 4).unwrap().alpha());
        assert_eq!(pm.pixel(0, 0).unwrap().alpha(), 0);
    }
    #[test]
    #[ignore = "Requires private real source files in OMADESIGN_BRUSH_FIXTURES"]
    fn real_adobe_sources_decode_paint_and_roundtrip_when_supplied() {
        let directory =
            std::env::var("OMADESIGN_BRUSH_FIXTURES").expect("private source fixture directory");
        for name in [
            "jaxgrass.abr",
            "grass20.abr",
            "rodeo.ai",
            "photoshop-legacy.abr",
            "adobe-stock.ai",
        ] {
            let pack = import(&Path::new(&directory).join(name)).unwrap();
            let bytes = pack.bytes().unwrap();
            let reloaded = decode(&bytes).unwrap();
            assert_eq!(bytes, reloaded.bytes().unwrap());
            for preset in &pack.brushes {
                let mut pm = tiny_skia::Pixmap::new(160, 80).unwrap();
                let mut brush = Brush::default();
                preset.apply(&mut brush);
                brush.size = 60.0;
                stamp(&mut pm, Pt::new(80.0, 40.0), &brush, &preset.tip, false);
                assert!(pm.pixels().iter().any(|p| p.alpha() > 0), "{}", preset.name);
            }
            eprintln!(
                "{name}: {} named tips decoded, painted, and roundtripped",
                pack.brushes.len()
            );
        }
    }
}
