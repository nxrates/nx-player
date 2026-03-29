use image::imageops::FilterType;
use image::ImageReader;
use lofty::prelude::*;
use std::io::Cursor;
use std::path::Path;

pub struct CoversDir(pub std::path::PathBuf);

/// Extract cover art from an audio file and save both full (600px) and thumbnail (256px) sizes.
pub fn extract_cover(track_path: &Path, track_id: &str, covers_dir: &Path) -> bool {
    let full_path = covers_dir.join(format!("{}_full.jpg", track_id));
    let thumb_path = covers_dir.join(format!("{}_thumb.jpg", track_id));

    // Legacy single-file path — migrate if it exists
    let legacy_path = covers_dir.join(format!("{}.jpg", track_id));
    if legacy_path.exists() && !full_path.exists() {
        // Promote old file to full, then generate thumb from it
        let _ = std::fs::rename(&legacy_path, &full_path);
        if let Ok(img) = image::open(&full_path) {
            let thumb = img.resize(256, 256, FilterType::Lanczos3);
            let _ = save_jpeg(&thumb, &thumb_path, 75);
        }
    }

    if full_path.exists() && thumb_path.exists() {
        return true;
    }

    let tagged_file = match lofty::read_from_path(track_path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let tag = match tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
        Some(t) => t,
        None => return false,
    };

    let pictures = tag.pictures();
    if pictures.is_empty() {
        return false;
    }

    let data = pictures[0].data();

    match ImageReader::new(Cursor::new(data)).with_guessed_format() {
        Ok(reader) => match reader.decode() {
            Ok(img) => {
                let full = img.resize(600, 600, FilterType::Lanczos3);
                let thumb = img.resize(256, 256, FilterType::Lanczos3);
                let ok_full = save_jpeg(&full, &full_path, 85);
                let ok_thumb = save_jpeg(&thumb, &thumb_path, 75);
                ok_full && ok_thumb
            }
            Err(_) => {
                // Can't decode — save raw as full, skip thumb
                std::fs::write(&full_path, data).is_ok()
            }
        },
        Err(_) => std::fs::write(&full_path, data).is_ok(),
    }
}

/// Get the cover path for a given track and size variant.
pub fn get_cover_path(track_id: &str, covers_dir: &Path, size: &str) -> Option<String> {
    let suffix = if size == "thumb" { "_thumb" } else { "_full" };
    let path = covers_dir.join(format!("{}{}.jpg", track_id, suffix));
    if path.exists() {
        return Some(path.to_string_lossy().to_string());
    }
    // Fallback to legacy single-file path
    let legacy = covers_dir.join(format!("{}.jpg", track_id));
    if legacy.exists() {
        return Some(legacy.to_string_lossy().to_string());
    }
    None
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
