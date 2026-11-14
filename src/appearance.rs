use crate::i18n::tr;
use crate::{db::Database, repositories::PlaylistRepository};
use anyhow::{Context, Result, ensure};
use image::{ImageReader, imageops::FilterType};
use std::{fs, path::Path};

#[derive(Clone)]
pub struct AppearanceService {
    db: Database,
    repository: PlaylistRepository,
}

impl AppearanceService {
    pub fn new(db: Database) -> Self {
        Self {
            repository: PlaylistRepository::new(db.clone()),
            db,
        }
    }

    fn previous_cover(&self, id: i64) -> Result<Option<String>> {
        self.repository
            .list_playlists()?
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.cover_path)
            .context(tr("Плейлист не найден"))
    }

    fn clean_old_cover(&self, path: Option<String>) {
        if let Some(path) = path {
            let path = Path::new(&path);
            // Only generated assets directly inside artwork belong to this service.
            if path.parent() == Some(Path::new("artwork")) && path.file_name().is_some() {
                let _ = fs::remove_file(self.db.root().join(path));
            }
        }
    }

    pub fn set_cover(&self, id: i64, source: &Path) -> Result<()> {
        let old = self.previous_cover(id)?;
        ensure!(
            fs::metadata(source)?.len() <= 25 * 1024 * 1024,
            tr("Выберите изображение до 25 МБ")
        );
        let (width, height) = ImageReader::open(source)?
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 40_000_000,
            tr("Изображение должно содержать не больше 40 миллионов пикселей")
        );
        let decoded = ImageReader::open(source)?
            .with_guessed_format()?
            .decode()
            .context(tr(
                "Не удалось открыть изображение. Поддерживаются PNG, JPEG и WebP",
            ))?;
        let image = decoded.resize_to_fill(640, 640, FilterType::Lanczos3);
        let color = dominant_header_color(&image.thumbnail(64, 64).to_rgba8());
        fs::create_dir_all(self.db.root().join("artwork"))?;
        let path = format!("artwork/{}.png", uuid::Uuid::new_v4());
        let destination = self.db.root().join(&path);
        let mut staged = tempfile::NamedTempFile::new_in(self.db.root().join("artwork"))?;
        image.write_to(staged.as_file_mut(), image::ImageFormat::Png)?;
        staged.persist(&destination)?;
        if let Err(error) = self.repository.set_cover(id, Some(&path), Some(color)) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        self.clean_old_cover(old);
        Ok(())
    }

    pub fn remove_cover(&self, id: i64) -> Result<()> {
        let old = self.previous_cover(id)?;
        self.repository.set_cover(id, None, None)?;
        self.clean_old_cover(old);
        Ok(())
    }

    pub fn set_header_color(&self, id: i64, color: Option<u32>) -> Result<()> {
        ensure!(
            color.is_none_or(|c| c <= 0xffffff),
            tr("Цвет должен быть в формате #RRGGBB")
        );
        self.repository.set_header_color(id, color)
    }
}

pub fn parse_color(value: &str) -> Result<u32> {
    let value = value.trim().trim_start_matches('#');
    ensure!(
        value.len() == 6 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        tr("Введите цвет в формате #RRGGBB, например #34465E")
    );
    Ok(u32::from_str_radix(value, 16)?)
}

fn dominant_header_color(image: &image::RgbaImage) -> u32 {
    let mut bins = vec![(0u64, [0u64; 3]); 512];
    for pixel in image.pixels() {
        if pixel[3] < 128 {
            continue;
        }
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let max = *rgb.iter().max().unwrap();
        let min = *rgb.iter().min().unwrap();
        let weight = 1 + u64::from(max - min) / 32;
        let index =
            usize::from(rgb[0] / 32) * 64 + usize::from(rgb[1] / 32) * 8 + usize::from(rgb[2] / 32);
        let bin = &mut bins[index];
        bin.0 += weight;
        for (sum, channel) in bin.1.iter_mut().zip(rgb) {
            *sum += u64::from(channel) * weight;
        }
    }
    let (count, channels) = bins.into_iter().max_by_key(|bin| bin.0).unwrap();
    if count == 0 {
        return 0x2b303b;
    }
    channels.into_iter().fold(0, |color, sum| {
        (color << 8) | ((sum / count) as u32 * 3 / 10 + 12)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_existing_library_and_persists_cover_and_color() {
        let root = tempfile::tempdir().unwrap();
        let conn = rusqlite::Connection::open(root.path().join("library.sqlite3")).unwrap();
        conn.execute_batch(
            include_str!("schema.sql")
                .split("CREATE TABLE IF NOT EXISTS playlist_appearance")
                .next()
                .unwrap(),
        )
        .unwrap();
        conn.execute("INSERT INTO playlists (id,name) VALUES (1,'Existing')", [])
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        drop(conn);
        let db = Database::open(root.path()).unwrap();
        let service = AppearanceService::new(db.clone());
        let source = root.path().join("original.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([20, 80, 220, 255]))
            .save(&source)
            .unwrap();
        service.set_cover(1, &source).unwrap();
        let playlist = PlaylistRepository::new(Database::open(root.path()).unwrap())
            .list_playlists()
            .unwrap()
            .remove(0);
        assert_eq!(playlist.cover_color, Some(0x12244e));
        let path = root.path().join(playlist.cover_path.unwrap());
        assert!(path.exists());
        assert_eq!(image::image_dimensions(&path).unwrap(), (640, 640));
        service.set_header_color(1, Some(0x34465e)).unwrap();
        assert_eq!(
            PlaylistRepository::new(db.clone())
                .list_playlists()
                .unwrap()[0]
                .header_color,
            Some(0x34465e)
        );
        service.set_header_color(1, None).unwrap();
        service.remove_cover(1).unwrap();
        assert!(source.exists());
        assert!(!path.exists());
        assert!(
            PlaylistRepository::new(db).list_playlists().unwrap()[0]
                .cover_path
                .is_none()
        );
        assert!(parse_color("oops").is_err());
        assert_eq!(parse_color("#34465E").unwrap(), 0x34465e);
    }

    #[test]
    fn failed_cover_update_keeps_previous_image() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let repo = PlaylistRepository::new(db.clone());
        let id = repo.create("Music").unwrap();
        let service = AppearanceService::new(db);
        let source = root.path().join("source.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 20, 20, 255]))
            .save(&source)
            .unwrap();
        service.set_cover(id, &source).unwrap();
        let before = repo.list_playlists().unwrap().remove(0);
        fs::write(&source, b"invalid").unwrap();
        assert!(service.set_cover(id, &source).is_err());
        assert_eq!(before, repo.list_playlists().unwrap().remove(0));
        assert!(root.path().join(before.cover_path.unwrap()).exists());
    }
}
