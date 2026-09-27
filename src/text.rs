//! Live type: rustybuzz shapes OpenType, ab_glyph traces outlines.

use crate::geom::{Pt, TypeRun};
use ab_glyph::{Font as _, GlyphId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

const PREFERRED: &[&str] = &[
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

const FONT_CAP: usize = 2000;

static FONTS: OnceLock<Vec<FontFace>> = OnceLock::new();
static BYTES: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
static MEMORY_NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub fn project_font_loaded(id: &str) -> bool {
    BYTES.get().is_some_and(|cache| {
        cache
            .lock()
            .is_ok_and(|cache| cache.contains_key(Path::new(id)))
    })
}

pub(crate) fn project_font_bytes(id: &str) -> Option<Arc<Vec<u8>>> {
    if !id.starts_with("omatype:") {
        return None;
    }
    BYTES.get()?.lock().ok()?.get(Path::new(id)).cloned()
}

/// Project fonts are addressed by their bytes, independent of a machine's paths.
/// They do not join the global installed-font picker.
pub fn register_memory_font(id: &str, name: &str, bytes: Arc<Vec<u8>>) -> Result<(), String> {
    let expected = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    if id != expected
        || rustybuzz::Face::from_slice(&bytes, 0).is_none()
        || ab_glyph::FontRef::try_from_slice(&bytes).is_err()
    {
        return Err("Invalid project font data".into());
    }
    let mut cache = BYTES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "Font cache is unavailable")?;
    let path = PathBuf::from(id);
    if let Some(old) = cache.get(&path) {
        if **old != *bytes {
            return Err("Project font fingerprint collision".into());
        }
    } else {
        cache.insert(path, bytes);
    }
    MEMORY_NAMES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "Font labels are unavailable")?
        .insert(id.into(), name.into());
    Ok(())
}

pub fn font_name(bytes: &[u8]) -> Option<String> {
    let face = rustybuzz::ttf_parser::Face::parse(bytes, 0).ok()?;
    for id in [4, 1] {
        if let Some(name) = face
            .names()
            .into_iter()
            .filter(|name| name.name_id == id)
            .find_map(|name| name.to_string().filter(|name| !name.trim().is_empty()))
        {
            return Some(name.trim().to_owned());
        }
    }
    None
}

#[derive(Clone)]
pub struct FontFace {
    pub name: String,
    pub path: PathBuf,
}

fn family_name(path: &Path) -> String {
    // Try to read the OpenType name table for a human family + style.
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(face) = rustybuzz::ttf_parser::Face::parse(&bytes, 0)
    {
        let mut family: Option<String> = None;
        let mut style: Option<String> = None;
        for n in face.names() {
            if n.name_id == 1
                && n.is_unicode()
                && let Some(s) = n.to_string()
            {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    family = Some(s);
                }
            }
            if n.name_id == 2
                && n.is_unicode()
                && let Some(s) = n.to_string()
            {
                let s = s.trim().to_string();
                if !s.is_empty() && !s.eq_ignore_ascii_case("regular") {
                    style = Some(s);
                }
            }
        }
        if let Some(fam) = family {
            if let Some(st) = style {
                // Avoid duplicating family when style already contains it.
                if st.to_lowercase().contains(&fam.to_lowercase()) {
                    return st;
                }
                return format!("{fam} {st}");
            }
            return fam;
        }
    }
    path.file_stem()
        .map(|s| {
            s.to_string_lossy()
                .replace(['-', '_'], " ")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "Font".into())
}

fn fc_font_files() -> Vec<PathBuf> {
    let output = std::process::Command::new("fc-list")
        .args(["-f", "%{file}\n", ":"])
        .output();
    let Ok(out) = output else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut v = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let p = PathBuf::from(t);
        if p.extension()
            .and_then(|e| e.to_str())
            .map(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc" | "otc"
                )
            })
            .unwrap_or(false)
        {
            v.push(p);
        }
    }
    v.sort();
    v.dedup();
    v
}

fn scan_fonts() -> Vec<FontFace> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Preferred still first so the initial sort is stable for bundled fallbacks.
    for p in PREFERRED {
        let path = PathBuf::from(p);
        if path.exists() && seen.insert(path.clone()) {
            let name = family_name(&path);
            out.push(FontFace { name, path });
        }
    }
    // Canonical set from fontconfig – this is the broadest and respects the
    // user's fontconfig excludes/substitutes.
    for path in fc_font_files() {
        if out.len() >= FONT_CAP {
            break;
        }
        if seen.insert(path.clone()) {
            let name = family_name(&path);
            out.push(FontFace { name, path });
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    // Fallback recursive walk for directories fontconfig may not yet know
    // (e.g. freshly downloaded Google Fonts before fc-cache).
    for root in [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        &format!("{home}/.local/share/fonts"),
        &format!("{home}/.fonts"),
        "/usr/share/fonts/liberation",
        "/usr/share/fonts/noto",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/adobe-source-code-pro",
        "/usr/share/fonts/gsfonts",
        "/usr/share/fonts/ttf-ia-writer",
    ] {
        if root.is_empty() {
            continue;
        }
        walk_ttfs(Path::new(root), &mut out, &mut seen, 0);
        if out.len() >= FONT_CAP {
            break;
        }
    }
    out.sort_by_cached_key(|f| f.name.to_lowercase());
    out.truncate(FONT_CAP);
    out
}

fn walk_ttfs(
    dir: &Path,
    out: &mut Vec<FontFace>,
    seen: &mut std::collections::HashSet<PathBuf>,
    depth: u8,
) {
    if depth > 8 || out.len() >= FONT_CAP {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_ttfs(&p, out, seen, depth + 1);
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        if matches!(
            ext.as_deref(),
            Some("ttf") | Some("otf") | Some("ttc") | Some("otc")
        ) && seen.insert(p.clone())
        {
            let name = family_name(&p);
            out.push(FontFace { name, path: p });
        }
    }
}

pub fn fonts() -> &'static [FontFace] {
    FONTS.get_or_init(scan_fonts)
}

static DYNAMIC_FONTS: OnceLock<Mutex<Vec<FontFace>>> = OnceLock::new();
static FONT_LIST: OnceLock<Mutex<Option<Arc<[FontFace]>>>> = OnceLock::new();
static FONT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn register_font(face: FontFace) {
    let path = face.path.clone();
    let m = DYNAMIC_FONTS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut g) = m.lock()
        && !g.iter().any(|f| f.path == face.path)
    {
        g.push(face);
    }
    if let Some(list) = FONT_LIST.get()
        && let Ok(mut cached) = list.lock()
    {
        *cached = None;
    }
    // Reload only the installed face; other open documents keep their fonts.
    if let Some(cache) = BYTES.get()
        && let Ok(mut m) = cache.lock()
    {
        m.remove(&path);
    }
    FONT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn all_fonts_cached() -> Arc<[FontFace]> {
    let cache = FONT_LIST.get_or_init(|| Mutex::new(None));
    let mut cached = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(list) = cached.as_ref() {
        return Arc::clone(list);
    }
    let mut v = fonts().to_vec();
    if let Some(m) = DYNAMIC_FONTS.get()
        && let Ok(g) = m.lock()
    {
        for f in g.iter() {
            if !v.iter().any(|e| e.path == f.path) {
                v.push(f.clone());
            }
        }
    }
    v.sort_by_cached_key(|f| f.name.to_lowercase());
    let list: Arc<[FontFace]> = v.into();
    *cached = Some(Arc::clone(&list));
    list
}

pub fn default_path() -> Option<PathBuf> {
    fonts().first().map(|f| f.path.clone())
}

pub fn face_for(path: &str) -> Option<&'static FontFace> {
    if path.is_empty() {
        return fonts().first();
    }
    fonts().iter().find(|f| f.path.to_string_lossy() == path)
}

pub fn label_for(path: &str) -> String {
    if path.starts_with("omatype:") {
        return MEMORY_NAMES
            .get()
            .and_then(|names| names.lock().ok()?.get(path).cloned())
            .unwrap_or_else(|| "Project font (load its typography kit)".into());
    }
    face_for(path).map(|f| f.name.clone()).unwrap_or_else(|| {
        if path.is_empty() {
            fonts()
                .first()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "System sans".into())
        } else {
            Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| "Font".into())
        }
    })
}

fn resolve_path(run: &TypeRun) -> Option<PathBuf> {
    if run.font.starts_with("omatype:") {
        let path = PathBuf::from(&run.font);
        return BYTES
            .get()
            .and_then(|cache| cache.lock().ok()?.contains_key(&path).then_some(path));
    }
    if !run.font.is_empty() {
        let p = PathBuf::from(&run.font);
        if p.exists() {
            return Some(p);
        }
    }
    default_path()
}

fn font_bytes(path: &Path) -> Option<Arc<Vec<u8>>> {
    let cache = BYTES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().ok()?;
    if let Some(b) = map.get(path) {
        return Some(b.clone());
    }
    let raw = std::fs::read(path).ok()?;
    let arc = Arc::new(raw);
    map.insert(path.to_path_buf(), arc.clone());
    Some(arc)
}

fn ot_features(run: &TypeRun) -> Vec<rustybuzz::Feature> {
    let tag = |b: &[u8; 4]| rustybuzz::ttf_parser::Tag::from_bytes(b);
    let feat = |bytes: &[u8; 4], on: bool| rustybuzz::Feature::new(tag(bytes), u32::from(on), ..);
    vec![
        feat(b"kern", run.kern),
        feat(b"liga", run.liga),
        feat(b"clig", run.liga),
        feat(b"tnum", run.tnum),
        feat(b"smcp", run.smcp),
        feat(b"c2sc", run.smcp),
    ]
}

fn map_pt(p: ab_glyph::Point, ox: f32, oy: f32, scale: f32) -> ab_glyph::Point {
    ab_glyph::point(ox + p.x * scale, oy - p.y * scale)
}

fn map_curves(
    curves: &[ab_glyph::OutlineCurve],
    ox: f32,
    oy: f32,
    scale: f32,
) -> Vec<ab_glyph::OutlineCurve> {
    use ab_glyph::OutlineCurve;
    curves
        .iter()
        .map(|c| match *c {
            OutlineCurve::Line(p0, p1) => {
                OutlineCurve::Line(map_pt(p0, ox, oy, scale), map_pt(p1, ox, oy, scale))
            }
            OutlineCurve::Quad(p0, p1, p2) => OutlineCurve::Quad(
                map_pt(p0, ox, oy, scale),
                map_pt(p1, ox, oy, scale),
                map_pt(p2, ox, oy, scale),
            ),
            OutlineCurve::Cubic(p0, p1, p2, p3) => OutlineCurve::Cubic(
                map_pt(p0, ox, oy, scale),
                map_pt(p1, ox, oy, scale),
                map_pt(p2, ox, oy, scale),
                map_pt(p3, ox, oy, scale),
            ),
        })
        .collect()
}

fn flatten_outline(curves: &[ab_glyph::OutlineCurve]) -> Vec<Vec<Pt>> {
    use ab_glyph::OutlineCurve;
    let start_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    };
    let end_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(_, p1)
        | OutlineCurve::Quad(_, _, p1)
        | OutlineCurve::Cubic(_, _, _, p1) => *p1,
    };
    let mut out = vec![];
    let mut pts: Vec<Pt> = vec![];
    let mut last_end: Option<ab_glyph::Point> = None;
    for c in curves {
        let s = start_of(c);
        let broke = last_end
            .map(|e| (e.x - s.x).abs() > 1e-3 || (e.y - s.y).abs() > 1e-3)
            .unwrap_or(true);
        if broke {
            if pts.len() >= 3 {
                out.push(std::mem::take(&mut pts));
            } else {
                pts.clear();
            }
            pts.push(Pt::new(s.x, s.y));
        }
        match c {
            OutlineCurve::Line(_, p1) => pts.push(Pt::new(p1.x, p1.y)),
            OutlineCurve::Quad(p0, p1, p2) => {
                for i in 1..=8 {
                    let t = i as f32 / 8.0;
                    let a = (1.0 - t) * (1.0 - t);
                    let b = 2.0 * (1.0 - t) * t;
                    let d = t * t;
                    pts.push(Pt::new(
                        a * p0.x + b * p1.x + d * p2.x,
                        a * p0.y + b * p1.y + d * p2.y,
                    ));
                }
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                for i in 1..=12 {
                    let t = i as f32 / 12.0;
                    let a = (1.0 - t) * (1.0 - t) * (1.0 - t);
                    let b = 3.0 * (1.0 - t) * (1.0 - t) * t;
                    let d = 3.0 * (1.0 - t) * t * t;
                    let e = t * t * t;
                    pts.push(Pt::new(
                        a * p0.x + b * p1.x + d * p2.x + e * p3.x,
                        a * p0.y + b * p1.y + d * p2.y + e * p3.y,
                    ));
                }
            }
        }
        last_end = Some(end_of(c));
    }
    if pts.len() >= 3 {
        out.push(pts);
    }
    out
}

fn fallback_block(content: &str, px: f32, origin: Pt) -> Vec<Vec<Pt>> {
    let lines = content.split('\n').count().max(1);
    let cols = content
        .split('\n')
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let w = px * 0.55 * cols as f32;
    let h = px * lines as f32;
    vec![vec![
        Pt::new(origin.x, origin.y - px),
        Pt::new(origin.x + w, origin.y - px),
        Pt::new(origin.x + w, origin.y - px + h),
        Pt::new(origin.x, origin.y - px + h),
    ]]
}

/// Shape `run` into closed contours in world space.
pub fn shape(run: &TypeRun) -> Vec<Vec<Pt>> {
    if run.on_path.is_some() { return crate::text_geometry::shape_on_path(run); }
    let Some(path) = resolve_path(run) else {
        return fallback_block(&run.content, run.px, run.origin);
    };
    let Some(bytes) = font_bytes(&path) else {
        return fallback_block(&run.content, run.px, run.origin);
    };
    buzz_shape(&bytes, run).unwrap_or_else(|| fallback_block(&run.content, run.px, run.origin))
}

fn buzz_shape(_bytes: &[u8], run: &TypeRun) -> Option<Vec<Vec<Pt>>> {
    Some(compose(run).iter().flat_map(|line|line.glyphs.iter().flat_map(|g|glyph_contours(run,g))).collect())
}

mod composer;
mod edit;
mod metrics;
pub use metrics::character_metrics;
mod opentype;
pub use opentype::{font_features, feature_value, feature_css, glyph_alternates};
pub mod hyphenation;
pub use composer::{compose, compose_plain, glyph_contours, paragraph_style, LayoutGlyph, LayoutLine};

pub fn fill_contours(geom: &mut crate::geom::Geom) { if let crate::geom::Geom::Text(run)=geom {run.contours=shape(run);} }
pub fn measure(run:&TypeRun)->(f32,f32) {
    let lines=compose(run);(run.wrap_width.unwrap_or_else(||lines.iter().map(|l|l.width).fold(0.,f32::max)),lines.iter().map(|l|l.height).sum())
}
pub fn caret_pt(run:&TypeRun,char_idx:usize)->Pt {
    if let Some((point,_)) = crate::text_geometry::caret_frame(run,char_idx) { return point; }
    let lines=compose(run);let idx=char_idx.min(run.content.chars().count());
    let line=&lines[lines.iter().rposition(|l|l.start<=idx).unwrap_or(0)];
    Pt::new(run.origin.x+line.caret_x(idx),run.origin.y+line.baseline-character_metrics(run,idx.min(run.content.chars().count().saturating_sub(1))).baseline_shift.unwrap())
}
pub fn hit_char(run:&TypeRun,p:Pt)->usize {
    if let Some(index) = crate::text_geometry::hit_char(run,p) { return index; }
    let lines=compose(run);
    lines.iter().flat_map(|line|line.carets.iter().map(move |(index,x)| {
        let style=character_metrics(run,(*index).min(run.content.chars().count().saturating_sub(1)));
        let y=run.origin.y+line.baseline-style.baseline_shift.unwrap();let height=run.px*style.vscale.unwrap()/100.;
        let dy=if p.y<y-height*0.9 {y-height*0.9-p.y}else if p.y>y+height*0.25{p.y-y-height*0.25}else{0.};
        let dx=run.origin.x+x-p.x;(*index,dx*dx+dy*dy)
    })).min_by(|(_,a),(_,b)|a.total_cmp(b)).map_or(0,|(index,_)|index)
}
pub fn selection_rects(run:&TypeRun,a:usize,b:usize)->Vec<(Pt,Pt)> {
    let lo=a.min(b);let hi=a.max(b);let mut rects=Vec::new();
    for line in compose(run).iter() {
        let mut from=lo.max(line.start);let end=hi.min(line.end);
        while from<end {
            let style=character_metrics(run,from);let mut to=from+1;
            while to<end {let next=character_metrics(run,to);if next.baseline_shift!=style.baseline_shift||next.vscale!=style.vscale{break;}to+=1;}
            let y=run.origin.y+line.baseline-style.baseline_shift.unwrap();let px=run.px*style.vscale.unwrap()/100.;
            rects.push((Pt::new(run.origin.x+line.caret_x(from),y-px*0.9),Pt::new(run.origin.x+line.caret_x(to),y+px*0.25)));from=to;
        }
    }
    rects
}
pub fn caret_height(run:&TypeRun,index:usize)->f32 {run.px*character_metrics(run,index.min(run.content.chars().count().saturating_sub(1))).vscale.unwrap()/100.}
pub fn char_to_byte(s:&str,char_idx:usize)->usize {s.char_indices().nth(char_idx).map(|(i,_)|i).unwrap_or(s.len())}
pub fn paragraph_spacing_css(run: &TypeRun, style: &crate::geom::ParagraphStyle) -> String {
    // CSS word spacing adds to the font's space advance; the inspector stores
    // a percentage of that advance. Measure the same space as the composer.
    let space_run = TypeRun { content: " ".into(), wrap_width: None, paragraphs: vec![], spans: vec![], ..run.clone() };
    let space = compose_plain(&space_run)[0].glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
    let untracked = TypeRun { tracking: 0., ..space_run };
    let base_space = compose_plain(&untracked)[0].glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
    let px = run.px.max(1.);
    format!("--oma-paragraph-letter-spacing:{}em;letter-spacing:calc({}em + var(--oma-paragraph-letter-spacing));word-spacing:{}em;", style.letter_spacing[1] / 100., (space - base_space) / px, space * (style.word_spacing[1] / 100. - 1.) / px)
}
pub fn character_css(run:&TypeRun,style:&crate::geom::CharSpan)->String {
    use std::fmt::Write;
    let mut css=String::new();
    if let Some(value)=style.tracking{let _=write!(css,"letter-spacing:calc({}em + var(--oma-paragraph-letter-spacing, 0em));",value/1000.);}
    if let Some(value)=style.leading{let _=write!(css,"line-height:{}px;",value.pixels(run.px));}
    if let Some(mode)=style.kerning{let _=write!(css,"font-kerning:{};",if mode==crate::geom::KernMode::Metrics{"normal"}else{"none"});}
    if let Some(value)=style.baseline_shift{let _=write!(css,"position:relative;top:{}px;",-value);}
    if style.hscale.is_some()||style.vscale.is_some(){let _=write!(css,"display:inline-block;transform-origin:left baseline;transform:scale({},{});",style.hscale.unwrap_or(100.)/100.,style.vscale.unwrap_or(100.)/100.);}
    css
}
pub fn styled_html(run:&TypeRun,start:usize,text:&str)->String {
    let escape=|s:&str|s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;");
    let mut out=String::new();let mut from=0;let chars:Vec<_>=text.chars().collect();
    while from<chars.len() {
        let style=run.character_style(start+from);let mut to=from+1;
        while to<chars.len()&&run.character_style(start+to)==style {to+=1;}
        let mut features=vec![(*b"kern",u32::from(run.kern)),(*b"liga",u32::from(run.liga)),(*b"clig",u32::from(run.liga)),(*b"tnum",u32::from(run.tnum)),(*b"smcp",u32::from(run.smcp)),(*b"c2sc",u32::from(run.smcp))];
        for &(tag,value) in run.features.iter().chain(&style.features) {features.retain(|(t,_)|*t!=tag);features.push((tag,value));}
        if let Some(mode)=style.kerning {features.retain(|(tag,_)|*tag!=*b"kern");features.push((*b"kern",u32::from(mode==crate::geom::KernMode::Metrics)));}
        out.push_str(&format!("<span style=\"font-feature-settings:{};{}\">{}</span>",feature_css(&features),format!("{}{}",if style.no_break{"white-space:pre;"}else{""},character_css(run,&style)),escape(&chars[from..to].iter().collect::<String>())));
        from=to;
    }
    out
}
pub fn paragraph_html(run:&TypeRun)->String {
    use crate::geom::{TextAlign,LastLine,BreakMode};
    let mut base=0;let mut html=String::new();
    for text in run.content.split('\n') {
        let p=paragraph_style(run,base);
        let (align,last)=match p.align {TextAlign::Start=>("left","auto"),TextAlign::Center=>("center","auto"),TextAlign::End=>("right","auto"),TextAlign::Justify{last}=>("justify",match last{LastLine::Start=>"left",LastLine::Center=>"center",LastLine::End=>"right",LastLine::Justify=>"justify"})};
        html.push_str(&format!("<span style=\"display:block;min-height:1em;text-align:{align};text-align-last:{last};{}white-space:{};word-break:{};overflow-wrap:{};hyphens:{}\">{}</span>",paragraph_spacing_css(run,&p),if p.break_mode==BreakMode::KeepAll{"pre"}else{"pre-wrap"},if p.break_mode==BreakMode::BreakAll{"break-all"}else{"normal"},if p.overflow_wrap{"anywhere"}else{"normal"},if p.hyphenate{"auto"}else{"manual"},styled_html(run,base,text)));
        base+=text.chars().count()+1;
    }
    html
}
#[cfg(test)]
mod paragraph_export_spacing_tests {
    use super::*;
    #[test]
    fn paragraph_export_preserves_desired_spacing_and_adds_tracking() {
        let mut run = TypeRun { content: "one two\nthree four".into(), font: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/fonts/EBGaramond.ttf").into(), px: 20., tracking: 2., ..Default::default() };
        run.update_paragraphs(0, 0, |p| { p.word_spacing=[80.,150.,200.]; p.letter_spacing=[0.,10.,20.]; });
        run.update_paragraphs(8, 8, |p| { p.word_spacing=[50.,75.,100.]; p.letter_spacing=[-10.,-5.,0.]; });
        let html=paragraph_html(&run);
        assert!(html.contains("--oma-paragraph-letter-spacing:0.1em;letter-spacing:calc(0.1em + var(--oma-paragraph-letter-spacing))"));
        assert!(html.contains("--oma-paragraph-letter-spacing:-0.05em;"));
        for paragraph in &run.paragraphs {
            let mut probe=run.clone(); probe.content="x x".into(); probe.paragraphs=vec![crate::geom::ParagraphStyle { start:0, ..paragraph.clone() }];
            let laid=compose(&probe); let actual=laid[0].glyphs.iter().find(|g|g.is_space).unwrap().advance;
            probe.paragraphs.clear(); let normal=compose(&probe)[0].glyphs.iter().find(|g|g.is_space).unwrap().advance;
            let word_extra=(actual-normal-run.px*paragraph.letter_spacing[1]/100.)/run.px;
            let css=paragraph_spacing_css(&run,paragraph); let emitted=css.split("word-spacing:").nth(1).unwrap().trim_end_matches("em;").parse::<f32>().unwrap();
            assert!((emitted-word_extra).abs()<0.00001,"HTML spacing {emitted} differs from composed {word_extra}");
        }
    }
}
pub fn glyph_count(run:&TypeRun)->usize {compose(run).iter().map(|l|l.glyphs.len()).sum()}

/// Scan ~/Projects for `next/font/google` imports and return the most frequent
/// family (e.g. “Inter”). Underscores in the identifier are turned into spaces
/// so `Noto_Sans_Mono` → “Noto Sans Mono”.
pub fn detect_max_font_family() -> Option<String> {
    detect_max_font_family_in(&projects_root())
}

fn projects_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/Projects"))
}

fn detect_max_font_family_in(root: &Path) -> Option<String> {
    if !root.exists() {
        return None;
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    walk_for_font_imports(root, &mut counts, 0);
    if counts.is_empty() {
        return None;
    }
    // Prefer the family with highest count; tie-break by preferring sans over mono,
    // then alphabetically – this makes Inter (sans) win over Noto_Sans_Mono (mono)
    // when counts are equal, matching hustlelaunch.com's primary sans.
    let mut best: Option<(String, usize)> = None;
    for (fam, cnt) in counts {
        let entry = best.get_or_insert((fam.clone(), cnt));
        if cnt > entry.1 || (cnt == entry.1 && is_preferred_over(&fam, &entry.0)) {
            *entry = (fam, cnt);
        }
    }
    best.map(|(f, _)| f)
}

fn is_preferred_over(a: &str, b: &str) -> bool {
    // Sans is preferred over mono for a design default.
    let a_mono = a.to_ascii_lowercase().contains("mono");
    let b_mono = b.to_ascii_lowercase().contains("mono");
    if a_mono != b_mono {
        return !a_mono && b_mono;
    }
    a.to_ascii_lowercase() < b.to_ascii_lowercase()
}

fn walk_for_font_imports(dir: &Path, counts: &mut HashMap<String, usize>, depth: u8) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            // Skip heavy dirs
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(
                name,
                "node_modules" | ".git" | "target" | ".next" | "dist" | "build"
            ) {
                continue;
            }
            walk_for_font_imports(&p, counts, depth + 1);
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "ts" | "tsx" | "js" | "jsx") {
            continue;
        }
        // Only inspect files that could contain next/font
        if let Ok(text) = std::fs::read_to_string(&p) {
            if !text.contains("next/font/google") {
                continue;
            }
            // Extract `import { Inter, Foo } from "next/font/google"` identifiers.
            // We do a light parse: find the substring between "import {" and "} from".
            for import in extract_google_imports(&text) {
                for ident in import.split(',') {
                    let raw = ident
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches(|c| c == '{' || c == '}');
                    // Handle `Inter as MyInter` – take before “as”.
                    let base = raw.split(" as ").next().unwrap_or(raw).trim();
                    if base.is_empty() || base.starts_with("/*") {
                        continue;
                    }
                    // Convert `Noto_Sans_Mono` → “Noto Sans Mono”
                    let family = base.replace('_', " ").trim().to_string();
                    if !family.is_empty() {
                        *counts.entry(family).or_insert(0) += 1;
                    }
                }
            }
        }
    }
}

fn extract_google_imports(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = text[pos..].find("next/font/google") {
        let abs = pos + start;
        // Walk backwards to find the opening “{” of the import.
        let before = &text[..abs];
        if let Some(brace_start) = before.rfind("import") {
            let segment = &before[brace_start..abs];
            if let Some(l) = segment.find('{')
                && let Some(r) = segment[l..].find('}')
            {
                out.push(segment[l + 1..l + r].to_string());
            }
        }
        pos = abs + "next/font/google".len();
    }
    out
}

/// Resolve the preferred default font file by looking at the max font from web
/// apps, then omarchy font, then Inter, then the first system font.
pub fn preferred_default_path() -> Option<PathBuf> {
    // 1. Custom max font from uncap.us (max95 for headings/type tool)
    if let Some(p) = find_installed_for_family("max") {
        return Some(p);
    }
    // 2. Max font from Projects
    if let Some(fam) = detect_max_font_family()
        && let Some(p) = find_installed_for_family(&fam)
    {
        return Some(p);
    }
    // If the family is a Google Font but not installed, still return None so
    // the caller can suggest a download. The UI will show “Inter not installed”.
    // 3. Omarchy desktop font (e.g. "JetBrainsMono Nerd Font") – try to find its file.
    if let Ok(out) = std::process::Command::new("omarchy")
        .args(["font", "current"])
        .output()
        && out.status.success()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty()
            && let Some(p) = find_installed_for_family(&name)
        {
            return Some(p);
        }
    }
    // 4. Hard-coded Inter – the most common branding sans in the user's repos.
    if let Some(p) = find_installed_for_family("Inter") {
        return Some(p);
    }
    // 5. First system font
    default_path()
}

fn find_installed_for_family(family: &str) -> Option<PathBuf> {
    let q = family.to_ascii_lowercase();
    // Prefer exact family name match, then substring.
    let all = all_fonts_cached();
    if let Some(f) = all.iter().find(|f| f.name.to_ascii_lowercase() == q) {
        return Some(f.path.clone());
    }
    if let Some(f) = all
        .iter()
        .find(|f| f.name.to_ascii_lowercase().contains(&q))
    {
        return Some(f.path.clone());
    }
    // Also try file-stem contains.
    for f in all.iter() {
        let stem = f
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if stem.contains(&q.replace(' ', "").to_ascii_lowercase())
            || stem.contains(&q.replace(' ', "_").to_ascii_lowercase())
            || q.contains(&stem.replace('-', " "))
        {
            return Some(f.path.clone());
        }
    }
    None
}

pub fn preferred_default_family_name() -> Option<String> {
    detect_max_font_family().or_else(|| {
        // fallback to omarchy font name for display
        std::process::Command::new("omarchy")
            .args(["font", "current"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
                None
            })
    })
}

pub fn available() -> bool {
    default_path().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Geom;

    fn run(content: &str) -> TypeRun {
        TypeRun {
            content: content.into(),
            px: 48.0,
            ..TypeRun::default()
        }
    }

    #[test]
    fn shapes_when_font_exists() {
        let r = run("Ag");
        let sub = shape(&r);
        assert!(!sub.is_empty());
        for sp in &sub {
            assert!(sp.len() >= 3);
        }
        let (w, h) = measure(&r);
        assert!(w > 10.0 && h > 10.0);
    }

    #[test]
    fn fill_contours_writes_cache() {
        let mut g = Geom::Text(run("Type"));
        fill_contours(&mut g);
        let Geom::Text(t) = g else { panic!("text") };
        assert!(!t.contours.is_empty());
    }

    #[test]
    fn caret_walks_forward() {
        let r = run("Hi");
        let a = caret_pt(&r, 0);
        let b = caret_pt(&r, 2);
        assert!(b.x > a.x);
        assert_eq!(hit_char(&r, a), 0);
    }

    #[test]
    fn wrapped_paragraph_keeps_source_and_caret_positions() {
        let mut r = run("A quiet place to make excellent work.\nCafé 東京");
        r.px = 20.0;
        r.wrap_width = Some(110.0);
        let rows = compose(&r);
        assert!(rows.len() > 2);
        assert_eq!(
            rows.iter().map(|l| l.text.as_str()).collect::<String>(),
            r.content.replace('\n', "")
        );
        for line in rows.iter() {
            let p = caret_pt(&r, line.start);
            assert_eq!(hit_char(&r, p), line.start);
        }
        let before = r.content.clone();
        let h = measure(&r).1;
        r.wrap_width = Some(220.0);
        assert!(measure(&r).1 < h);
        assert_eq!(r.content, before);
        assert_eq!(r.px, 20.0);
    }

    #[test]
    fn liga_reduces_fi_when_font_supports_it() {
        let on = TypeRun {
            liga: true,
            ..run("fi")
        };
        let off = TypeRun {
            liga: false,
            ..run("fi")
        };
        let a = glyph_count(&on);
        let b = glyph_count(&off);
        assert!(a >= 1 && b >= 1);
        // If the face has an `fi` ligature, on < off; otherwise they match.
        assert!(a <= b);
    }

    #[test]
    fn old_json_defaults_kern_liga_on() {
        let g: Geom = serde_json::from_str(
            r#"{"Text":{"origin":{"x":0.0,"y":0.0},"content":"Hi","px":24.0,"tracking":0.0}}"#,
        )
        .unwrap();
        let Geom::Text(t) = g else { panic!("text") };
        assert!(t.kern && t.liga);
        assert!(!t.tnum && !t.smcp);
        assert!(t.font.is_empty());
    }

    #[test]
    fn max_font_detection_prefers_inter() {
        let tmp = std::env::temp_dir().join(format!("omadesign-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proj/app")).unwrap();
        std::fs::write(
            tmp.join("proj/app/layout.tsx"),
            r#"import { Inter, Noto_Sans_Mono } from "next/font/google";"#,
        )
        .unwrap();
        std::fs::write(
            tmp.join("proj/app/page.tsx"),
            r#"import { Inter } from "next/font/google";"#,
        )
        .unwrap();
        let fam = detect_max_font_family_in(&tmp).unwrap();
        assert_eq!(fam, "Inter", "Inter appears twice, Noto once");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
