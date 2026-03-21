use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesResponse {
    #[allow(dead_code)]
    result_count: u32,
    results: Vec<ItunesResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesResult {
    artist_name: Option<String>,
    #[allow(dead_code)]
    track_name: Option<String>,
    artwork_url100: Option<String>,
}

/// Search iTunes for cover art, download and save it.
/// Returns the path to the saved cover if successful.
pub async fn fetch_cover(
    artist: &str,
    title: &str,
    track_id: &str,
    covers_dir: &Path,
) -> Option<PathBuf> {
    let dest = covers_dir.join(format!("{}.jpg", track_id));

    // Already cached
    if dest.exists() {
        return Some(dest);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    // Search iTunes
    let query = format!("{} {}", artist, title);
    let resp = client
        .get("https://itunes.apple.com/search")
        .query(&[
            ("term", query.as_str()),
            ("entity", "song"),
            ("limit", "3"),
        ])
        .send()
        .await
        .ok()?;

    let itunes: ItunesResponse = resp.json().await.ok()?;

    // Find best match (prefer matching artist)
    let artist_lower = artist.to_lowercase();
    let best = itunes
        .results
        .iter()
        .find(|r| {
            r.artist_name
                .as_ref()
                .map(|a| a.to_lowercase().contains(&artist_lower))
                .unwrap_or(false)
        })
        .or(itunes.results.first());

    let artwork_url = best?
        .artwork_url100
        .as_ref()?
        .replace("100x100bb", "600x600bb"); // Upgrade resolution

    // Download the image
    let img_bytes = client
        .get(&artwork_url)
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;

    // Optimize: resize to 400x400 max and re-encode as JPEG quality 80
    // This typically produces 20-40KB files instead of 50-100KB raw
    match image::load_from_memory(&img_bytes) {
        Ok(img) => {
            let resized = img.resize(400, 400, image::imageops::FilterType::Lanczos3);
            let mut buf = std::io::BufWriter::new(std::fs::File::create(&dest).ok()?);
            resized
                .write_to(&mut buf, image::ImageFormat::Jpeg)
                .ok()?;
        }
        Err(_) => {
            // Fallback: save raw bytes if image processing fails
            std::fs::write(&dest, &img_bytes).ok()?;
        }
    }

    Some(dest)
}
