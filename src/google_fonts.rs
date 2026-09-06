//! Google Fonts on-demand: catalogue fetch, 7-day cache, and per-variant download.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CATALOG_URL: &str = "https://gwfh.mranftl.com/api/fonts";
const CACHE_DAYS: u64 = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoogleFont {
    #[serde(default)]
    pub id: String,
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
    // Keep family search available offline. Download URLs require a live catalogue.
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
            id: fam.to_ascii_lowercase().replace(' ', "-"),
            family: fam.into(),
            variants: vec![
                "regular".into(),
                "italic".into(),
                "700".into(),
                "700italic".into(),
            ],
            subsets: vec!["latin".into()],
            category: cat.into(),
            files: HashMap::new(),
        })
        .collect()
}

fn is_cache_fresh(p: &Path) -> bool {
    if let Ok(meta) = std::fs::metadata(p)
        && let Ok(mtime) = meta.modified()
        && let Ok(elapsed) = std::time::SystemTime::now().duration_since(mtime)
    {
        return elapsed.as_secs() < CACHE_DAYS * 24 * 3600;
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
    if let Some(cached) = load_catalog_from_cache()
        && !cached.is_empty()
        && cached.iter().all(|font| !font.id.is_empty())
    {
        return cached;
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

/// Resolve an exact variant, accepting 400 as the catalogue's name for regular.
pub fn url_for(font: &GoogleFont, variant: &str) -> Option<String> {
    if let Some(u) = font.files.get(variant) {
        return Some(u.clone());
    }
    // Fallback: many families expose “regular” under “400”.
    if variant == "regular"
        && let Some(u) = font.files.get("400")
    {
        return Some(u.clone());
    }
    None
}

#[derive(Deserialize)]
struct FontDetails {
    variants: Vec<FontVariant>,
}

#[derive(Deserialize)]
struct FontVariant {
    id: String,
    ttf: Option<String>,
}

impl FontDetails {
    fn url_for(&self, variant: &str) -> Option<&str> {
        let variant = if variant == "400" { "regular" } else { variant };
        self.variants
            .iter()
            .find(|item| item.id == variant)
            .and_then(|item| item.ttf.as_deref())
    }
}

fn download_url(font: &GoogleFont, variant: &str) -> Result<String, String> {
    if let Some(url) = url_for(font, variant) {
        return Ok(url);
    }
    // The list endpoint contains names; file URLs live on the family endpoint.
    let id = if font.id.is_empty() {
        font.family.to_ascii_lowercase().replace(' ', "-")
    } else {
        font.id.clone()
    };
    let response = ureq::get(&format!("{CATALOG_URL}/{id}"))
        .query("subsets", &font.subsets.join(","))
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("could not load font variants: {e}"))?;
    let text = response.into_string().map_err(|e| e.to_string())?;
    let details: FontDetails = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    details
        .url_for(variant)
        .map(str::to_owned)
        .ok_or_else(|| format!("{} has no downloadable {variant} variant", font.family))
}

pub fn is_installed(family: &str, variant: &str) -> bool {
    installed_path(family, variant).exists()
}

pub fn installed_path(family: &str, variant: &str) -> PathBuf {
    let safe_family = family.replace([' ', '/', '\\'], "_");
    let safe_variant = variant.replace([' ', '/', '\\'], "_");
    install_dir().join(format!("{safe_family}-{safe_variant}.ttf"))
}

/// Download family/variant to the install dir and register it for the current session.
/// Returns the file path on success.
pub fn download(family: &str, variant: &str, catalog: &[GoogleFont]) -> Result<PathBuf, String> {
    let font = catalog
        .iter()
        .find(|f| f.family.eq_ignore_ascii_case(family))
        .ok_or_else(|| format!("family '{family}' not in catalogue"))?;
    let url = download_url(font, variant)?;
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
    if rustybuzz::Face::from_slice(&bytes, 0).is_none() {
        return Err("download did not contain a valid font".into());
    }
    let dest = installed_path(family, variant);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("fc-cache")
        .arg("-f")
        .arg(install_dir())
        .output();
    // Register for this session so the combo shows it without restart.
    crate::text::register_font(crate::text::FontFace {
        name: format!("{family} {}", variant_name_pretty(variant)),
        path: dest.clone(),
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
    fn search_filters() {
        let c = bundled_catalog();
        let r = search(&c, "inter");
        assert!(r.iter().any(|f| f.family == "Inter"));
        let empty = search(&c, "zzzzz_nope");
        assert!(empty.is_empty());
    }

    #[test]
    fn install_path_is_sanitized() {
        let p = installed_path("Open/Sans", "../regular");
        assert_eq!(p.parent(), Some(install_dir().as_path()));
        assert_eq!(p.file_name().unwrap(), "Open_Sans-.._regular.ttf");
    }

    #[test]
    fn family_response_resolves_exact_ttf_variant() {
        let details: FontDetails = serde_json::from_str(
            r#"{"variants":[
            {"id":"regular","ttf":"https://fonts.gstatic.com/regular.ttf"},
            {"id":"700","ttf":"https://fonts.gstatic.com/bold.ttf"}
        ]}"#,
        )
        .unwrap();
        assert_eq!(
            details.url_for("400"),
            Some("https://fonts.gstatic.com/regular.ttf")
        );
        assert_eq!(
            details.url_for("700"),
            Some("https://fonts.gstatic.com/bold.ttf")
        );
        assert_eq!(details.url_for("italic"), None);
    }
}
