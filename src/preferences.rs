use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub volume: f32,
    pub shuffle: bool,
    pub queue_visible: bool,
    pub theme: crate::theme::Theme,
    pub repeat: crate::queue::RepeatMode,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 0.7,
            shuffle: false,
            queue_visible: true,
            theme: crate::theme::Theme::Forest,
            repeat: crate::queue::RepeatMode::Off,
        }
    }
}

impl Preferences {
    pub fn load(root: &Path) -> Self {
        let mut preferences: Self = fs::read(root.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        preferences.volume = if preferences.volume.is_finite() {
            preferences.volume.clamp(0., 1.)
        } else {
            0.7
        };
        preferences
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(root)?;
        serde_json::to_writer_pretty(file.as_file_mut(), self)?;
        file.as_file().sync_all()?;
        file.persist(root.join("settings.json"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_survive_restart_and_handle_invalid_files() {
        let root = tempfile::tempdir().unwrap();
        let preferences = Preferences {
            volume: 0.32,
            shuffle: true,
            queue_visible: false,
            theme: crate::theme::Theme::Onyx,
            repeat: crate::queue::RepeatMode::One,
        };
        preferences.save(root.path()).unwrap();
        preferences.save(root.path()).unwrap();
        let reopened = Preferences::load(root.path());
        assert_eq!(reopened.volume, 0.32);
        assert!(reopened.shuffle);
        assert!(!reopened.queue_visible);
        assert_eq!(reopened.theme, crate::theme::Theme::Onyx);
        assert_eq!(reopened.repeat, crate::queue::RepeatMode::One);
        for theme in [crate::theme::Theme::PickMe, crate::theme::Theme::Cosmic] {
            Preferences {
                theme,
                ..preferences.clone()
            }
            .save(root.path())
            .unwrap();
            assert_eq!(Preferences::load(root.path()).theme, theme);
        }
        fs::write(root.path().join("settings.json"), "broken").unwrap();
        assert_eq!(Preferences::load(root.path()).volume, 0.7);
        fs::write(root.path().join("settings.json"), r#"{"volume":4}"#).unwrap();
        assert_eq!(Preferences::load(root.path()).volume, 1.0);
    }
}
