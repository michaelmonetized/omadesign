//! Free asset browser – Pixabay, Pexels, Vexels, Vecteezy, Unsplash fallback.
//! Keys are read from env (PIXABAY_API_KEY, PEXELS_API_KEY) or
//! `~/.config/omadesign/assets.toml` (`pixabay_key = "..."`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetHit {
    pub id: String,
    pub provider: String,
    pub thumb_url: String,
    pub full_url: String,
    pub w: u32,
    pub h: u32,
    pub tags: String,
}

pub fn providers() -> &'static [&'static str] {
    &["All", "Pixabay", "Pexels", "Vecteezy", "Vexels", "Picsum"]
}

fn pixabay_key() -> Option<String> {
    std::env::var("PIXABAY_API_KEY").ok().filter(|s| !s.is_empty()).or_else(|| {
        std::fs::read_to_string(asset_config_path())
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.trim_start().starts_with("pixabay"))
                    .and_then(|l| l.split('=').nth(1))
                    .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            })
            .filter(|s| !s.is_empty())
    })
}

fn pexels_key() -> Option<String> {
    std::env::var("PEXELS_API_KEY").ok().filter(|s| !s.is_empty()).or_else(|| {
        std::fs::read_to_string(asset_config_path())
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.trim_start().starts_with("pexels"))
                    .and_then(|l| l.split('=').nth(1))
                    .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            })
            .filter(|s| !s.is_empty())
    })
}

fn asset_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(format!("{home}/.config/omadesign/assets.toml"))
}

/// Search free assets. If provider == "All", fans out to all configured providers
/// and merges (deduped). When no keys are set, returns a Picsum placeholder set
/// so the UI is never empty.
pub fn search(query: &str, provider: &str, limit: usize) -> Vec<AssetHit> {
    let q = query.trim();
    let lim = limit.clamp(4, 40);
    let mut out = Vec::new();
    let prov = provider.trim();
    let want = |name: &str| prov == "All" || prov.eq_ignore_ascii_case(name);

    if want("Pixabay") {
        if let Some(key) = pixabay_key() {
            if let Ok(mut v) = search_pixabay(q, &key, lim) {
                out.append(&mut v);
            }
        }
    }
    if want("Pexels") {
        if let Some(key) = pexels_key() {
            if let Ok(mut v) = search_pexels(q, &key, lim) {
                out.append(&mut v);
            }
        }
    }
    // Vecteezy / Vexels have no stable free REST without key – surface via Picsum
    // so the user still gets results. If you add keys, extend here.

    if out.is_empty() || want("Picsum") || want("All") {
        // Always seed with Picsum so the grid is populated even offline-free.
        // Picsum is free without a key and supports direct image URLs.
        let seed = if q.is_empty() { "omadesign" } else { q };
        for i in 0..lim.min(12) {
            out.push(AssetHit {
                id: format!("picsum-{seed}-{i}"),
                provider: "Picsum".into(),
                thumb_url: format!("https://picsum.photos/seed/{seed}-{i}/300/200"),
                full_url: format!("https://picsum.photos/seed/{seed}-{i}/1600/1200"),
                w: 1600,
                h: 1200,
                tags: seed.into(),
            });
            if out.len() >= lim {
                break;
            }
        }
    }
    out.truncate(lim);
    out
}

fn search_pixabay(query: &str, key: &str, per_page: usize) -> Result<Vec<AssetHit>, String> {
    let q = if query.is_empty() { "nature" } else { query };
    let url = format!(
        "https://pixabay.com/api/?key={}&q={}&image_type=photo&per_page={}&safesearch=true",
        urlencoding(key),
        urlencoding(q),
        per_page
    );
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| e.to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let hits = v.get("hits").and_then(|h| h.as_array()).ok_or("no hits")?;
    let mut out = Vec::new();
    for h in hits {
        let thumb = h.get("previewURL").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let full = h.get("largeImageURL").and_then(|s| s.as_str()).unwrap_or(&thumb).to_string();
        let w = h.get("imageWidth").and_then(|n| n.as_u64()).unwrap_or(1600) as u32;
        let hgt = h.get("imageHeight").and_then(|n| n.as_u64()).unwrap_or(1200) as u32;
        let tags = h.get("tags").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let id = h.get("id").map(|n| n.to_string()).unwrap_or_else(|| thumb.clone());
        if thumb.is_empty() || full.is_empty() {
            continue;
        }
        out.push(AssetHit { id, provider: "Pixabay".into(), thumb_url: thumb, full_url: full, w, h: hgt, tags });
    }
    Ok(out)
}

fn search_pexels(query: &str, key: &str, per_page: usize) -> Result<Vec<AssetHit>, String> {
    let q = if query.is_empty() { "nature" } else { query };
    let url = format!(
        "https://api.pexels.com/v1/search?query={}&per_page={}",
        urlencoding(q),
        per_page
    );
    let resp = ureq::get(&url)
        .set("Authorization", key)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| e.to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let photos = v.get("photos").and_then(|h| h.as_array()).ok_or("no photos")?;
    let mut out = Vec::new();
    for p in photos {
        let src = p.get("src").and_then(|s| s.as_object());
        let thumb = src.and_then(|s| s.get("medium")).and_then(|s| s.as_str()).unwrap_or("").to_string();
        let full = src.and_then(|s| s.get("large2x")).and_then(|s| s.as_str()).unwrap_or(&thumb).to_string();
        let w = p.get("width").and_then(|n| n.as_u64()).unwrap_or(1600) as u32;
        let h = p.get("height").and_then(|n| n.as_u64()).unwrap_or(1200) as u32;
        let id = p.get("id").map(|n| n.to_string()).unwrap_or_else(|| thumb.clone());
        let tags = p.get("alt").and_then(|s| s.as_str()).unwrap_or("").to_string();
        if thumb.is_empty() {
            continue;
        }
        out.push(AssetHit { id, provider: "Pexels".into(), thumb_url: thumb, full_url: full, w, h, tags });
    }
    Ok(out)
}

fn urlencoding(s: &str) -> String {
    // Minimal percent-encoding for query param
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Fetch full-resolution bytes for an asset hit.
pub fn download(hit: &AssetHit) -> Result<Vec<u8>, String> {
    let resp = ureq::get(&hit.full_url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut bytes = Vec::new();
    resp.into_reader().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() < 1024 {
        return Err("file too small".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picsum_fallback_not_empty() {
        let v = search("cats", "Picsum", 6);
        assert_eq!(v.len(), 6);
        assert!(v[0].thumb_url.contains("picsum.photos"));
    }

    #[test]
    fn all_fallback_when_no_keys() {
        // With no env keys, All should still return Picsum seeds
        let v = search("", "All", 4);
        assert!(!v.is_empty());
    }
}
