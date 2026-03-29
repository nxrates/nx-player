use serde::Deserialize;
use std::path::{Path, PathBuf};

// --- iTunes ---

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

// --- MusicBrainz + Cover Art Archive ---

#[derive(Debug, Deserialize)]
struct MbSearchResponse {
    recordings: Option<Vec<MbRecording>>,
    releases: Option<Vec<MbRelease>>,
}

#[derive(Debug, Deserialize)]
struct MbRecording {
    releases: Option<Vec<MbRelease>>,
}

#[derive(Debug, Deserialize)]
struct MbRelease {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CaaResponse {
    images: Vec<CaaImage>,
}

#[derive(Debug, Deserialize)]
struct CaaImage {
    front: Option<bool>,
    thumbnails: Option<CaaThumbnails>,
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CaaThumbnails {
    #[serde(rename = "1200")]
    large: Option<String>,
    #[serde(rename = "500")]
    medium: Option<String>,
    #[serde(rename = "250")]
    small: Option<String>,
}

// --- Deezer ---

#[derive(Debug, Deserialize)]
struct DeezerSearchResponse {
    data: Option<Vec<DeezerTrack>>,
}

#[derive(Debug, Deserialize)]
struct DeezerTrack {
    album: Option<DeezerAlbum>,
}

#[derive(Debug, Deserialize)]
struct DeezerAlbum {
    cover_xl: Option<String>,
}

/// Fetch cover art with multi-source fallback:
///   1. iTunes (best for mainstream)
///   2. MusicBrainz / Cover Art Archive (best for niche/electronic)
///   3. Deezer (broad catalog, no auth)
///
/// Returns the path to the saved full-size cover if any source succeeds.
pub async fn fetch_cover(
    artist: &str,
    title: &str,
    track_id: &str,
    covers_dir: &Path,
) -> Option<PathBuf> {
    let full_dest = covers_dir.join(format!("{}_full.jpg", track_id));
    let thumb_dest = covers_dir.join(format!("{}_thumb.jpg", track_id));

    if full_dest.exists() && thumb_dest.exists() {
        return Some(full_dest);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("NXPlayer/0.1 (https://github.com/nxrates/nx-player)")
        .build()
        .ok()?;

    // Try each source in order until one succeeds
    let img_bytes = try_itunes(&client, artist, title).await
        .or_async(try_musicbrainz(&client, artist, title)).await
        .or_async(try_deezer(&client, artist, title)).await?;

    match image::load_from_memory(&img_bytes) {
        Ok(img) => {
            let full = img.resize(1024, 1024, image::imageops::FilterType::Lanczos3);
            let thumb = img.resize(256, 256, image::imageops::FilterType::Lanczos3);
            save_jpeg(&full, &full_dest, 85);
            save_jpeg(&thumb, &thumb_dest, 75);
        }
        Err(_) => {
            std::fs::write(&full_dest, &img_bytes).ok()?;
        }
    }

    Some(full_dest)
}

/// Helper trait for chaining async Option fallbacks.
trait OrAsync {
    async fn or_async(self, fallback: impl std::future::Future<Output = Self>) -> Self;
}

impl<T> OrAsync for Option<T> {
    async fn or_async(self, fallback: impl std::future::Future<Output = Option<T>>) -> Option<T> {
        match self {
            Some(v) => Some(v),
            None => fallback.await,
        }
    }
}

// --- Source implementations ---

async fn try_itunes(client: &reqwest::Client, artist: &str, title: &str) -> Option<Vec<u8>> {
    let query = format!("{} {}", artist, title);
    let resp = client
        .get("https://itunes.apple.com/search")
        .query(&[("term", query.as_str()), ("entity", "song"), ("limit", "3")])
        .send().await.ok()?;

    let itunes: ItunesResponse = resp.json().await.ok()?;
    let artist_lower = artist.to_lowercase();

    let best = itunes.results.iter()
        .find(|r| r.artist_name.as_ref()
            .map(|a| a.to_lowercase().contains(&artist_lower))
            .unwrap_or(false))
        .or(itunes.results.first());

    let artwork_url = best?
        .artwork_url100.as_ref()?
        .replace("100x100bb", "1024x1024bb");

    let bytes = client.get(&artwork_url).send().await.ok()?.bytes().await.ok()?;
    if bytes.len() > 1000 { Some(bytes.to_vec()) } else { None }
}

async fn try_musicbrainz(client: &reqwest::Client, artist: &str, title: &str) -> Option<Vec<u8>> {
    // MusicBrainz requires a User-Agent; we set one on the client.
    // Search for recordings matching artist + title
    let query = format!("recording:\"{}\" AND artist:\"{}\"", title, artist);
    let resp = client
        .get("https://musicbrainz.org/ws/2/recording")
        .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "3")])
        .send().await.ok()?;

    let mb: MbSearchResponse = resp.json().await.ok()?;

    // Extract the first release ID from the recording results
    let release_id = mb.recordings?
        .into_iter()
        .flat_map(|r| r.releases.unwrap_or_default())
        .next()?
        .id;

    // Query Cover Art Archive for this release
    let caa_url = format!("https://coverartarchive.org/release/{}", release_id);
    let caa_resp = client.get(&caa_url).send().await.ok()?;

    if !caa_resp.status().is_success() {
        // No cover art for this release — try a release search as fallback
        return try_musicbrainz_release(client, artist, title).await;
    }

    let caa: CaaResponse = caa_resp.json().await.ok()?;

    // Pick the front cover, preferring the largest thumbnail
    let front = caa.images.iter().find(|i| i.front == Some(true)).or(caa.images.first())?;

    let img_url = front.thumbnails.as_ref()
        .and_then(|t| t.large.as_ref().or(t.medium.as_ref()))
        .or(front.image.as_ref())?;

    let bytes = client.get(img_url).send().await.ok()?.bytes().await.ok()?;
    if bytes.len() > 1000 { Some(bytes.to_vec()) } else { None }
}

/// Fallback: search MusicBrainz by release (album) if recording search fails.
async fn try_musicbrainz_release(client: &reqwest::Client, artist: &str, title: &str) -> Option<Vec<u8>> {
    let query = format!("release:\"{}\" AND artist:\"{}\"", title, artist);
    let resp = client
        .get("https://musicbrainz.org/ws/2/release")
        .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "3")])
        .send().await.ok()?;

    let mb: MbSearchResponse = resp.json().await.ok()?;
    let release_id = mb.releases?.into_iter().next()?.id;

    let caa_url = format!("https://coverartarchive.org/release/{}", release_id);
    let caa_resp = client.get(&caa_url).send().await.ok()?;
    if !caa_resp.status().is_success() { return None; }

    let caa: CaaResponse = caa_resp.json().await.ok()?;
    let front = caa.images.iter().find(|i| i.front == Some(true)).or(caa.images.first())?;

    let img_url = front.thumbnails.as_ref()
        .and_then(|t| t.large.as_ref().or(t.medium.as_ref()))
        .or(front.image.as_ref())?;

    let bytes = client.get(img_url).send().await.ok()?.bytes().await.ok()?;
    if bytes.len() > 1000 { Some(bytes.to_vec()) } else { None }
}

async fn try_deezer(client: &reqwest::Client, artist: &str, title: &str) -> Option<Vec<u8>> {
    let query = format!("artist:\"{}\" track:\"{}\"", artist, title);
    let resp = client
        .get("https://api.deezer.com/search")
        .query(&[("q", query.as_str()), ("limit", "3")])
        .send().await.ok()?;

    let deezer: DeezerSearchResponse = resp.json().await.ok()?;

    let cover_url = deezer.data?.into_iter()
        .find_map(|t| t.album?.cover_xl)?;

    let bytes = client.get(&cover_url).send().await.ok()?.bytes().await.ok()?;
    if bytes.len() > 1000 { Some(bytes.to_vec()) } else { None }
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
