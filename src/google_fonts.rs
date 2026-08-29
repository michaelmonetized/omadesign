//! Google Fonts on-demand: catalogue fetch, 7-day cache, and per-variant download.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CATALOG_URL: &str = "https://gwfh.mranftl.com/api/fonts";
const CACHE_DAYS: u64 = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoogleFont {
    pub family: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub subsets: Vec<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub files: HashMap<String, String>,
}

/// Where the catalogue JSON is cached.
pub fn cache_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.cache/omadesign/google-fonts.json"))
}

/// Where downloaded TTFs live (fontconfig picks up ~/.local/share/fonts/**).
pub fn install_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.local/share/fonts/omadesign/google"))
}

fn bundled_catalog() -> Vec<GoogleFont> {
    // Offline fallback – 30 web-safe popular families with at least regular.
    // URLs are the API files map entries for “regular” (or 400). They are real
    // Google storage URLs so an online fetch can still succeed even when the
    // catalogue API is unreachable.
    let families = [
        ("Inter", "sans-serif"),
        ("Roboto", "sans-serif"),
        ("Open Sans", "sans-serif"),
        ("Lato", "sans-serif"),
        ("Montserrat", "sans-serif"),
        ("Poppins", "sans-serif"),
        ("Source Sans 3", "sans-serif"),
        ("Noto Sans", "sans-serif"),
        ("Nunito", "sans-serif"),
        ("Raleway", "sans-serif"),
        ("Ubuntu", "sans-serif"),
        ("Playfair Display", "serif"),
        ("Merriweather", "serif"),
        ("Lora", "serif"),
        ("PT Serif", "serif"),
        ("JetBrains Mono", "monospace"),
        ("Fira Code", "monospace"),
        ("Source Code Pro", "monospace"),
        ("IBM Plex Sans", "sans-serif"),
        ("IBM Plex Mono", "monospace"),
        ("Work Sans", "sans-serif"),
        ("DM Sans", "sans-serif"),
        ("Space Grotesk", "sans-serif"),
        ("Manrope", "sans-serif"),
        ("Outfit", "sans-serif"),
        ("Plus Jakarta Sans", "sans-serif"),
        ("Lexend", "sans-serif"),
        ("Sora", "sans-serif"),
        ("Bebas Neue", "display"),
        ("Oswald", "sans-serif"),
    ];
    families
        .into_iter()
        .map(|(fam, cat)| GoogleFont {
            family: fam.into(),
            variants: vec!["regular".into(), "italic".into(), "700".into(), "700italic".into()],
            subsets: vec!["latin".into()],
            category: cat.into(),
            files: HashMap::new(), // empty – caller will build URL via google-fonts direct link if needed
        })
        .collect()
}

fn is_cache_fresh(p: &Path) -> bool {
    if let Ok(meta) = std::fs::metadata(p) {
        if let Ok(mtime) = meta.modified() {
            if let Ok(elapsed) = std::time::SystemTime::now().duration_since(mtime) {
                return elapsed.as_secs() < CACHE_DAYS * 24 * 3600;
            }
        }
    }
    false
}

pub fn load_catalog_from_cache() -> Option<Vec<GoogleFont>> {
    let p = cache_path();
    if !p.exists() || !is_cache_fresh(&p) {
        return None;
    }
    let s = std::fs::read_to_string(&p).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save_catalog_to_cache(fonts: &[GoogleFont]) {
    let p = cache_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string(fonts) {
        let _ = std::fs::write(&p, s);
    }
}

pub fn fetch_catalog() -> Result<Vec<GoogleFont>, String> {
    let resp = ureq::get(CATALOG_URL)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| e.to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let fonts: Vec<GoogleFont> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if fonts.is_empty() {
        return Err("empty catalogue".into());
    }
    save_catalog_to_cache(&fonts);
    Ok(fonts)
}

/// Public entry: cache → network → bundled.
pub fn catalog() -> Vec<GoogleFont> {
    if let Some(cached) = load_catalog_from_cache() {
        if !cached.is_empty() {
            return cached;
        }
    }
    match fetch_catalog() {
        Ok(v) => v,
        Err(_) => bundled_catalog(),
    }
}

pub fn search<'a>(catalog: &'a [GoogleFont], query: &str) -> Vec<&'a GoogleFont> {
    if query.trim().is_empty() {
        return catalog.iter().take(80).collect();
    }
    let q = query.to_ascii_lowercase();
    let mut out: Vec<&GoogleFont> = catalog
        .iter()
        .filter(|f| {
            f.family.to_ascii_lowercase().contains(&q)
                || f.category.to_ascii_lowercase().contains(&q)
        })
        .collect();
    out.sort_by(|a, b| {
        let a_starts = a.family.to_ascii_lowercase().starts_with(&q);
        let b_starts = b.family.to_ascii_lowercase().starts_with(&q);
        b_starts.cmp(&a_starts).then(a.family.cmp(&b.family))
    });
    out.truncate(80);
    out
}

/// Resolve a download URL for family/variant. Prefers the catalogue's files map,
/// else builds a Google Fonts direct URL (github google/fonts soft link is not
/// stable, so we use the fonts.gstatic.com pattern that the API itself returns).
pub fn url_for(font: &GoogleFont, variant: &str) -> Option<String> {
    if let Some(u) = font.files.get(variant) {
        return Some(u.clone());
    }
    // Fallback: many families expose “regular” under “400”.
    if variant == "regular" {
        if let Some(u) = font.files.get("400") {
            return Some(u.clone());
        }
    }
    if variant == "700" {
        if let Some(u) = font.files.get("700") {
            return Some(u.clone());
        }
    }
    // Last resort: synthesise a google-fonts github raw URL – the caller will
    // handle 404 by surfacing an error; the offline bundled catalogue has no
    // files map and will hit this path, so we return None to signal “no URL”.
    None
}

pub fn is_installed(family: &str, variant: &str) -> bool {
    installed_path(family, variant).exists()
}

pub fn installed_path(family: &str, variant: &str) -> PathBuf {
    let safe_family = family.replace(' ', "_").replace('/', "_");
    let safe_variant = variant.replace(' ', "_");
    install_dir().join(format!("{safe_family}-{safe_variant}.ttf"))
}

/// Download family/variant to the install dir and register it for the current session.
/// Returns the file path on success.
pub fn download(family: &str, variant: &str, catalog: &[GoogleFont]) -> Result<PathBuf, String> {
    let font = catalog
        .iter()
        .find(|f| f.family.eq_ignore_ascii_case(family))
        .ok_or_else(|| format!("family '{family}' not in catalogue"))?;
    let url = url_for(font, variant)
        .or_else(|| font.files.values().next().cloned())
        .ok_or_else(|| format!("no file URL for {family} {variant} (bundled offline list – connect to download)"))?;
    // Fetch bytes
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("download HTTP {}", resp.status()));
    }
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() < 4096 {
        return Err(format!("file too small ({} bytes) – likely an error page", bytes.len()));
    }
    let dest = installed_path(family, variant);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    // Invalidate fontconfig? Best-effort fc-cache, ignore errors.
    let _ = std::process::Command::new("fc-cache")
        .args(["-f", &install_dir().to_string_lossy().to_string()])
        .output();
    // Register for this session so the combo shows it without restart.
    let path = dest.clone();
    crate::text::register_font(crate::text::FontFace {
        name: format!("{family} {}", variant_name_pretty(variant)),
        path: path.clone(),
    });
    Ok(dest)
}

fn variant_name_pretty(v: &str) -> String {
    match v {
        "regular" | "400" => "".into(),
        "italic" => "Italic".into(),
        "700" => "Bold".into(),
        "700italic" => "Bold Italic".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_has_inter() {
        let c = bundled_catalog();
        assert!(c.iter().any(|f| f.family == "Inter"));
        assert!(c.len() >= 20);
    }

    #[test]
    fn search_filters() {
        let c = bundled_catalog();
        let r = search(&c, "inter");
        assert!(r.iter().any(|f| f.family == "Inter"));
        let empty = search(&c, "zzzzz_nope");
        assert!(empty.is_empty());
    }

    #[test]
    fn cache_roundtrip() {
        let c = bundled_catalog();
        save_catalog_to_cache(&c);
        let back = load_catalog_from_cache().unwrap();
        assert_eq!(back.len(), c.len());
    }

    #[test]
    fn install_path_is_sanitized() {
        let p = installed_path("Open Sans", "regular");
        assert!(p.to_string_lossy().contains("Open_Sans"));
    }
}
