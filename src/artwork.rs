//! Local, bounded artwork cache. Missing artwork never prevents importing audio.
use anyhow::{Result, ensure};
use image::{ImageReader, imageops::FilterType};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn track_path(root: &Path, track: &crate::models::Track) -> PathBuf {
    let stem = Path::new(&track.file_path).file_stem().unwrap_or_default();
    root.join("artwork").join(stem).with_extension("png")
}

/// Small controls use a separate 96px file; the 640px original stays available.
pub fn thumbnail_path(original: &Path) -> PathBuf {
    original
        .parent()
        .unwrap_or(Path::new("."))
        .join("thumbnails")
        .join(original.file_name().unwrap_or_default())
        .with_extension("png")
}

pub fn ensure_thumbnail(original: &Path) -> Result<()> {
    let destination = thumbnail_path(original);
    let source = std::fs::metadata(original)?;
    if std::fs::metadata(&destination)
        .is_ok_and(|cached| cached.modified().ok() >= source.modified().ok())
    {
        return Ok(());
    }
    let image = ImageReader::open(original)?
        .with_guessed_format()?
        .decode()?
        .resize_to_fill(96, 96, FilterType::Triangle);
    std::fs::create_dir_all(destination.parent().unwrap())?;
    let mut staged = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
    image.write_to(staged.as_file_mut(), image::ImageFormat::Png)?;
    staged.persist(destination)?;
    Ok(())
}

/// Inspect files without decoding images. Library metadata must be available
/// before the potentially slow thumbnail migration starts.
#[derive(Default)]
pub struct PreviewPlan {
    pub ready: std::collections::HashSet<PathBuf>,
    pub pending: Vec<PathBuf>,
}

impl PreviewPlan {
    pub fn inspect(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut plan = Self::default();
        let mut seen = std::collections::HashSet::new();
        for path in paths {
            if !seen.insert(path.clone()) {
                continue;
            }
            let Ok(source) = std::fs::metadata(&path) else {
                continue;
            };
            let cached = std::fs::metadata(thumbnail_path(&path)).ok();
            if cached.as_ref().is_some_and(|file| file.is_file()) {
                plan.ready.insert(path.clone());
            }
            if !cached.is_some_and(|file| {
                file.is_file()
                    && matches!((file.modified(), source.modified()), (Ok(cached), Ok(source)) if cached >= source)
            }) {
                plan.pending.push(path);
            }
        }
        plan
    }
}

pub(crate) fn download(url: &str) -> Result<Vec<u8>> {
    let url = url::Url::parse(url)?;
    ensure!(url.scheme() == "https", "Artwork requires HTTPS");
    Ok(ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .new_agent()
        .get(url.as_str())
        .call()?
        .body_mut()
        .with_config()
        .limit(10 * 1024 * 1024)
        .read_to_vec()?)
}

pub(crate) fn save(destination: &Path, bytes: &[u8]) -> Result<()> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (w, h) = reader.into_dimensions()?;
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 40_000_000,
        "Artwork is too large"
    );
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?
        .resize_to_fill(640, 640, FilterType::Lanczos3);
    let parent = destination.parent().unwrap();
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    image.write_to(staged.as_file_mut(), image::ImageFormat::Png)?;
    staged.persist(destination)?;
    ensure_thumbnail(destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspecting_previews_never_decodes_or_delays_library_metadata() {
        let root = tempfile::tempdir().unwrap();
        let cover = root.path().join("cover.png");
        image::RgbImage::new(640, 640).save(&cover).unwrap();
        let broken = root.path().join("broken.png");
        std::fs::write(&broken, b"invalid image").unwrap();
        let missing = root.path().join("missing.png");
        let plan = PreviewPlan::inspect([cover.clone(), broken.clone(), cover.clone(), missing]);
        assert!(plan.ready.is_empty());
        assert_eq!(plan.pending, [cover.clone(), broken]);
        assert!(!thumbnail_path(&cover).exists());
        ensure_thumbnail(&cover).unwrap();
        let plan = PreviewPlan::inspect([cover.clone()]);
        assert!(plan.pending.is_empty());
        assert!(plan.ready.contains(&cover));
    }
    #[test]
    fn previews_keep_the_full_cover_and_upgrade_existing_files() {
        let root = tempfile::tempdir().unwrap();
        let full = root.path().join("cover.png");
        image::RgbImage::new(640, 640).save(&full).unwrap();
        ensure_thumbnail(&full).unwrap();
        let preview = thumbnail_path(&full);
        assert_eq!(image::image_dimensions(&full).unwrap(), (640, 640));
        assert_eq!(image::image_dimensions(&preview).unwrap(), (96, 96));
        let modified = std::fs::metadata(&preview).unwrap().modified().unwrap();
        ensure_thumbnail(&full).unwrap();
        assert_eq!(
            std::fs::metadata(&preview).unwrap().modified().unwrap(),
            modified
        );
        std::fs::remove_file(&preview).unwrap();
        ensure_thumbnail(&full).unwrap();
        assert_eq!(image::image_dimensions(&preview).unwrap(), (96, 96));
    }
}
