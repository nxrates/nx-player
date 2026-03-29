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

/// Search iTunes for cover art, download and save both full (600px) and thumb (256px).
/// Returns the path to the full-size cover if successful.
pub async fn fetch_cover(
    artist: &str,
    title: &str,
    track_id: &str,
    covers_dir: &Path,
) -> Option<PathBuf> {
    let full_dest = covers_dir.join(format!("{}_full.jpg", track_id));
    let thumb_dest = covers_dir.join(format!("{}_thumb.jpg", track_id));

    // Already cached
    if full_dest.exists() && thumb_dest.exists() {
        return Some(full_dest);
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
        .replace("100x100bb", "600x600bb");

    // Download the image
    let img_bytes = client
        .get(&artwork_url)
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;

    match image::load_from_memory(&img_bytes) {
        Ok(img) => {
            let full = img.resize(600, 600, image::imageops::FilterType::Lanczos3);
            let thumb = img.resize(256, 256, image::imageops::FilterType::Lanczos3);
            save_jpeg(&full, &full_dest, 85);
            save_jpeg(&thumb, &thumb_dest, 75);
        }
        Err(_) => {
            // Fallback: save raw bytes as full
            std::fs::write(&full_dest, &img_bytes).ok()?;
        }
    }

    Some(full_dest)
}

fn save_jpeg(img: &image::DynamicImage, path: &Path, quality: u8) -> bool {
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        quality,
    );
    encoder.encode_image(img).is_ok()
}
