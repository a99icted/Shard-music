use crate::db::Database;
use crate::i18n::tr;
use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageReader};
use rusqlite::params;
use std::{fs, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Background {
    pub scope: i64,
    pub original_path: String,
    pub display_path: String,
    pub blur: u8,
}

#[derive(Clone)]
pub struct BackgroundService {
    db: Database,
}
impl BackgroundService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    pub fn list(&self) -> Result<Vec<Background>> {
        let conn = self.db.connect()?;
        let mut query =
            conn.prepare("SELECT scope_id,original_path,display_path,blur FROM backgrounds")?;
        Ok(query
            .query_map([], |r| {
                Ok(Background {
                    scope: r.get(0)?,
                    original_path: r.get(1)?,
                    display_path: r.get(2)?,
                    blur: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    fn load_image(path: &Path) -> Result<DynamicImage> {
        ensure!(
            fs::metadata(path)?.len() <= 25 * 1024 * 1024,
            tr("Выберите изображение до 25 МБ")
        );
        let (w, h) = ImageReader::open(path)?
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 40_000_000,
            tr("Изображение должно содержать не больше 40 миллионов пикселей")
        );
        let image = ImageReader::open(path)?
            .with_guessed_format()?
            .decode()
            .context(tr("Поддерживаются PNG, JPEG и WebP"))?;
        Ok(if image.width() > 1600 || image.height() > 1600 {
            image.thumbnail(1600, 1600)
        } else {
            image
        })
    }
    fn write(&self, image: &DynamicImage) -> Result<String> {
        let directory = self.db.root().join("backgrounds");
        fs::create_dir_all(&directory)?;
        let relative = format!("backgrounds/{}.png", uuid::Uuid::new_v4());
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        image.write_to(file.as_file_mut(), image::ImageFormat::Png)?;
        file.persist(self.db.root().join(&relative))?;
        Ok(relative)
    }
    fn clean(&self, path: &str) {
        let path = Path::new(path);
        if path.parent() == Some(Path::new("backgrounds")) {
            let _ = fs::remove_file(self.db.root().join(path));
        }
    }
    fn render(&self, image: &DynamicImage, blur: u8) -> Result<String> {
        if blur == 0 {
            self.write(image)
        } else {
            self.write(&image.blur(f32::from(blur)))
        }
    }
    pub fn set_image(&self, scope: i64, source: &Path) -> Result<()> {
        ensure!(scope >= 0, tr("Некорректная область фона"));
        let previous = self.list()?.into_iter().find(|b| b.scope == scope);
        let blur = previous.as_ref().map(|b| b.blur).unwrap_or(12);
        let image = Self::load_image(source)?;
        let conn = self.db.connect()?;
        let original = self.write(&image)?;
        let display = match self.render(&image, blur) {
            Ok(path) => path,
            Err(e) => {
                self.clean(&original);
                return Err(e);
            }
        };
        if let Err(e)=conn.execute("INSERT INTO backgrounds(scope_id,playlist_id,original_path,display_path,blur) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(scope_id) DO UPDATE SET original_path=excluded.original_path,display_path=excluded.display_path,blur=excluded.blur",params![scope,(scope>0).then_some(scope),original,display,blur]) {
            self.clean(&original);self.clean(&display);return Err(e.into());
        }
        if let Some(old) = previous {
            self.clean(&old.original_path);
            self.clean(&old.display_path);
        }
        Ok(())
    }
    pub fn set_blur(&self, scope: i64, blur: u8) -> Result<()> {
        ensure!(blur <= 40, tr("Размытие должно быть от 0 до 40"));
        let old = self
            .list()?
            .into_iter()
            .find(|b| b.scope == scope)
            .context(tr("Сначала выберите фон"))?;
        let image = Self::load_image(&self.db.root().join(&old.original_path))?;
        let conn = self.db.connect()?;
        let display = self.render(&image, blur)?;
        if let Err(e) = conn.execute(
            "UPDATE backgrounds SET display_path=?2,blur=?3 WHERE scope_id=?1",
            params![scope, display, blur],
        ) {
            self.clean(&display);
            return Err(e.into());
        }
        self.clean(&old.display_path);
        Ok(())
    }
    pub fn remove(&self, scope: i64) -> Result<()> {
        let previous = self.list()?.into_iter().find(|b| b.scope == scope);
        self.db
            .connect()?
            .execute("DELETE FROM backgrounds WHERE scope_id=?1", [scope])?;
        if let Some(old) = previous {
            self.clean(&old.original_path);
            self.clean(&old.display_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backgrounds_persist_blur_independently_and_reject_invalid_replacements() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let playlist = crate::repositories::PlaylistRepository::new(db.clone())
            .create("Music")
            .unwrap();
        let service = BackgroundService::new(db.clone());
        let source = root.path().join("source.png");
        image::RgbImage::from_fn(100, 60, |x, _| {
            image::Rgb(if x < 50 { [255, 0, 0] } else { [0, 0, 255] })
        })
        .save(&source)
        .unwrap();
        service.set_image(0, &source).unwrap();
        service.set_image(playlist, &source).unwrap();
        service.set_blur(playlist, 0).unwrap();
        let all = BackgroundService::new(Database::open(root.path()).unwrap())
            .list()
            .unwrap();
        assert_eq!(all.len(), 2);
        let global = all.iter().find(|b| b.scope == 0).unwrap();
        assert_eq!(global.blur, 12);
        assert_eq!(all.iter().find(|b| b.scope == playlist).unwrap().blur, 0);
        let original = image::open(root.path().join(&global.original_path))
            .unwrap()
            .to_rgb8();
        let blurred = image::open(root.path().join(&global.display_path))
            .unwrap()
            .to_rgb8();
        assert_eq!(original.dimensions(), (100, 60));
        assert_ne!(original.get_pixel(49, 30), blurred.get_pixel(49, 30));
        assert!(service.set_blur(0, 41).is_err());
        fs::write(&source, b"bad image").unwrap();
        assert!(service.set_image(0, &source).is_err());
        assert_eq!(all, service.list().unwrap());
        let old = global.display_path.clone();
        service.remove(0).unwrap();
        assert!(!root.path().join(old).exists());
        assert!(source.exists());
        assert_eq!(service.list().unwrap().len(), 1);
    }
}
