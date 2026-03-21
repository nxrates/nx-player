use image::imageops::FilterType;
use image::ImageReader;
use lofty::prelude::*;
use std::io::Cursor;
use std::path::Path;

pub struct CoversDir(pub std::path::PathBuf);

pub fn extract_cover(track_path: &Path, track_id: &str, covers_dir: &Path) -> bool {
    let cover_path = covers_dir.join(format!("{}.jpg", track_id));
    if cover_path.exists() {
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

    let picture = &pictures[0];
    let data = picture.data();

    // Try to decode and resize the image
    match ImageReader::new(Cursor::new(data)).with_guessed_format() {
        Ok(reader) => match reader.decode() {
            Ok(img) => {
                let resized = img.resize(400, 400, FilterType::Lanczos3);
                match std::fs::File::create(&cover_path) {
                    Ok(f) => {
                        let mut buf = std::io::BufWriter::new(f);
                        resized.write_to(&mut buf, image::ImageFormat::Jpeg).is_ok()
                    }
                    Err(_) => false,
                }
            }
            Err(_) => {
                // If we can't decode, try saving raw data as jpg
                std::fs::write(&cover_path, data).is_ok()
            }
        },
        Err(_) => {
            std::fs::write(&cover_path, data).is_ok()
        }
    }
}

pub fn get_cover_path(track_id: &str, covers_dir: &Path) -> Option<String> {
    let cover_path = covers_dir.join(format!("{}.jpg", track_id));
    if cover_path.exists() {
        Some(cover_path.to_string_lossy().to_string())
    } else {
        None
    }
}
