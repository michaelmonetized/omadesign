//! Photo search from configured Pixabay/Pexels accounts and explicit Picsum samples.
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
    &["All", "Pixabay", "Pexels", "Picsum"]
}

fn api_key(env: &str, config_key: &str) -> Option<String> {
    std::env::var(env)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            let config = std::fs::read_to_string(asset_config_path()).ok()?;
            config.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                if key.trim() != config_key {
                    return None;
                }
                let value = value.trim().trim_matches(['"', '\'']);
                (!value.is_empty()).then(|| value.to_string())
            })
        })
}

fn asset_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(format!("{home}/.config/omadesign/assets.toml"))
}

/// Search configured providers. Picsum samples are available only when requested.
pub fn search(query: &str, provider: &str, limit: usize) -> Result<Vec<AssetHit>, String> {
    let q = query.trim();
    let lim = limit.clamp(4, 40);
    let provider = provider.trim();
    if provider.eq_ignore_ascii_case("Picsum") {
        let seed = urlencoding(if q.is_empty() { "omadesign" } else { q });
        return Ok((0..lim)
            .map(|i| AssetHit {
                id: format!("picsum-{seed}-{i}"),
                provider: "Picsum".into(),
                thumb_url: format!("https://picsum.photos/seed/{seed}-{i}/300/200"),
                full_url: format!("https://picsum.photos/seed/{seed}-{i}/1600/1200"),
                w: 1600,
                h: 1200,
                tags: "Sample photo".into(),
            })
            .collect());
    }
    let all = provider.eq_ignore_ascii_case("All");
    if !all && !provider.eq_ignore_ascii_case("Pixabay") && !provider.eq_ignore_ascii_case("Pexels")
    {
        return Err(format!("Unknown photo provider: {provider}"));
    }

    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut searched = false;
    for (name, env, config_key) in [
        ("Pixabay", "PIXABAY_API_KEY", "pixabay_key"),
        ("Pexels", "PEXELS_API_KEY", "pexels_key"),
    ] {
        if !all && !provider.eq_ignore_ascii_case(name) {
            continue;
        }
        let Some(key) = api_key(env, config_key) else {
            if !all {
                return Err(format!(
                    "Set {env} or {config_key} in assets.toml to search {name}."
                ));
            }
            continue;
        };
        let result = if name == "Pixabay" {
            search_pixabay(q, &key, lim)
        } else {
            search_pexels(q, &key, lim)
        };
        match result {
            Ok(hits) => {
                searched = true;
                out.extend(hits);
            }
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    if !searched {
        return Err(if errors.is_empty() {
            "Add a Pixabay or Pexels API key to search photos, or select Picsum for samples.".into()
        } else {
            errors.join("; ")
        });
    }
    out.truncate(lim);
    Ok(out)
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
        .map_err(|_| "Request failed. Check your connection and API key.".to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let hits = v.get("hits").and_then(|h| h.as_array()).ok_or("no hits")?;
    let mut out = Vec::new();
    for h in hits {
        let thumb = h
            .get("previewURL")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let full = h
            .get("largeImageURL")
            .and_then(|s| s.as_str())
            .unwrap_or(&thumb)
            .to_string();
        let w = h.get("imageWidth").and_then(|n| n.as_u64()).unwrap_or(1600) as u32;
        let hgt = h
            .get("imageHeight")
            .and_then(|n| n.as_u64())
            .unwrap_or(1200) as u32;
        let tags = h
            .get("tags")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let id = h
            .get("id")
            .map(|n| n.to_string())
            .unwrap_or_else(|| thumb.clone());
        if thumb.is_empty() || full.is_empty() {
            continue;
        }
        out.push(AssetHit {
            id,
            provider: "Pixabay".into(),
            thumb_url: thumb,
            full_url: full,
            w,
            h: hgt,
            tags,
        });
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
        .map_err(|_| "Request failed. Check your connection and API key.".to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let photos = v
        .get("photos")
        .and_then(|h| h.as_array())
        .ok_or("no photos")?;
    let mut out = Vec::new();
    for p in photos {
        let src = p.get("src").and_then(|s| s.as_object());
        let thumb = src
            .and_then(|s| s.get("medium"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let full = src
            .and_then(|s| s.get("large2x"))
            .and_then(|s| s.as_str())
            .unwrap_or(&thumb)
            .to_string();
        let w = p.get("width").and_then(|n| n.as_u64()).unwrap_or(1600) as u32;
        let h = p.get("height").and_then(|n| n.as_u64()).unwrap_or(1200) as u32;
        let id = p
            .get("id")
            .map(|n| n.to_string())
            .unwrap_or_else(|| thumb.clone());
        let tags = p
            .get("alt")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        if thumb.is_empty() {
            continue;
        }
        out.push(AssetHit {
            id,
            provider: "Pexels".into(),
            thumb_url: thumb,
            full_url: full,
            w,
            h,
            tags,
        });
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
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() < 1024 {
        return Err("file too small".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picsum_samples_encode_seed_and_respect_limit() {
        let hits = search("cats/dogs & birds", "Picsum", 6).unwrap();
        assert_eq!(hits.len(), 6);
        assert!(hits[0].thumb_url.contains("cats%2Fdogs+%26+birds"));
        assert_eq!(hits[0].tags, "Sample photo");
        assert!(search("cats", "Vexels", 6).is_err());
    }
}
