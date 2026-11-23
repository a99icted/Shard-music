use crate::localized_format;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

enum WriteCommand {
    Save(Preferences),
    Flush(std::sync::mpsc::Sender<()>),
}

/// One ordered writer keeps fsync and atomic replacement away from input handlers.
#[derive(Clone)]
pub struct PreferencesWriter {
    sender: std::sync::mpsc::Sender<WriteCommand>,
    error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}
impl PreferencesWriter {
    pub fn new(root: std::path::PathBuf) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let error = std::sync::Arc::new(std::sync::Mutex::new(None));
        let failures = error.clone();
        std::thread::Builder::new()
            .name("shard-preferences".into())
            .spawn(move || {
                let mut pending = None;
                while let Some(command) = pending.take().or_else(|| receiver.recv().ok()) {
                    match command {
                        WriteCommand::Save(mut preferences) => {
                            // Rapid slider/wheel input only needs the latest value on disk.
                            // A flush remains a barrier and must not be overtaken by a save.
                            while let Ok(next) = receiver.try_recv() {
                                match next {
                                    WriteCommand::Save(latest) => preferences = latest,
                                    flush => {
                                        pending = Some(flush);
                                        break;
                                    }
                                }
                            }
                            if let Err(error) = preferences.save(&root) {
                                *failures.lock().unwrap() = Some(localized_format!(
                                    "Не удалось сохранить настройки: {error:#}",
                                    "Could not save settings: {error:#}"
                                ));
                            }
                        }
                        WriteCommand::Flush(reply) => {
                            let _ = reply.send(());
                        }
                    }
                }
            })
            .expect("preferences worker");
        Self { sender, error }
    }
    pub fn save(&self, preferences: Preferences) {
        let _ = self.sender.send(WriteCommand::Save(preferences));
    }
    pub fn flush_and_wait(&self) {
        let (sender, receiver) = std::sync::mpsc::channel();
        if self.sender.send(WriteCommand::Flush(sender)).is_ok() {
            let _ = receiver.recv_timeout(std::time::Duration::from_secs(5));
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub sidebar_collapsed: bool,
    pub sync_warn_duplicates: bool,
    pub sync_ignore_duplicates: bool,
    pub sync_enabled: bool,
    pub sync_device_id: String,
    pub sync_device_name: String,
    pub language: crate::i18n::Language,
    pub volume: f32,
    pub audio_output: Option<String>,
    pub audio_monitoring: bool,
    pub obs_stream: bool,
    pub refresh_inactive: bool,
    pub progress_pet: crate::pets::ProgressPet,
    pub cat_color: crate::pets::PetColor,
    pub bird_color: crate::pets::PetColor,
    pub dancing_cats_color: crate::pets::PetColor,
    pub frog_variant: crate::pets::PetColor,
    pub pig_variant: crate::pets::PetColor,
    pub cursor_pet: crate::pets::CursorPet,
    pub cursor_pet_speed: u16,
    pub shuffle: bool,
    pub queue_visible: bool,
    pub autoplay: bool,
    pub theme: crate::theme::Theme,
    pub repeat: crate::queue::RepeatMode,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            sync_warn_duplicates: true,
            sync_ignore_duplicates: false,
            sync_enabled: false,
            sync_device_id: uuid::Uuid::new_v4().to_string(),
            sync_device_name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Shard".into()),
            language: crate::i18n::Language::Russian,
            volume: 0.7,
            audio_output: None,
            audio_monitoring: true,
            obs_stream: false,
            refresh_inactive: false,
            progress_pet: Default::default(),
            cat_color: Default::default(),
            bird_color: Default::default(),
            dancing_cats_color: Default::default(),
            frog_variant: Default::default(),
            pig_variant: Default::default(),
            cursor_pet: Default::default(),
            cursor_pet_speed: 50,
            shuffle: false,
            queue_visible: true,
            autoplay: false,
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
        if preferences.autoplay {
            preferences.shuffle = true;
        }
        preferences.volume = if preferences.volume.is_finite() {
            preferences.volume.clamp(0., 1.)
        } else {
            0.7
        };
        preferences.cursor_pet_speed = preferences.cursor_pet_speed.clamp(20, 150);
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
    fn background_saves_keep_latest_settings_and_flush_on_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let writer = PreferencesWriter::new(root.path().to_path_buf());
        for volume in [0.1, 0.2, 0.3] {
            writer.save(Preferences {
                volume,
                ..Default::default()
            });
        }
        writer.flush_and_wait();
        assert_eq!(Preferences::load(root.path()).volume, 0.3);
        assert!(writer.take_error().is_none());
    }
    #[test]
    fn preferences_survive_restart_and_handle_invalid_files() {
        let root = tempfile::tempdir().unwrap();
        let defaults = Preferences::load(root.path());
        assert!(!defaults.sidebar_collapsed);
        assert!(defaults.sync_warn_duplicates);
        assert!(!defaults.sync_ignore_duplicates);
        let preferences = Preferences {
            language: crate::i18n::Language::English,
            sidebar_collapsed: true,
            volume: 0.32,
            audio_output: Some("Headphones".into()),
            audio_monitoring: false,
            obs_stream: true,
            refresh_inactive: true,
            progress_pet: crate::pets::ProgressPet::Frog,
            cat_color: crate::pets::PetColor::Grooving,
            bird_color: crate::pets::PetColor::White,
            dancing_cats_color: crate::pets::PetColor::Gray,
            frog_variant: crate::pets::PetColor::Melancholy,
            cursor_pet: crate::pets::CursorPet::Frog,
            cursor_pet_speed: 35,
            shuffle: true,
            queue_visible: false,
            theme: crate::theme::Theme::Onyx,
            repeat: crate::queue::RepeatMode::One,
            sync_warn_duplicates: false,
            sync_ignore_duplicates: true,
            ..Default::default()
        };
        preferences.save(root.path()).unwrap();
        preferences.save(root.path()).unwrap();
        let reopened = Preferences::load(root.path());
        assert!(reopened.sidebar_collapsed);
        assert_eq!(reopened.volume, 0.32);
        assert_eq!(reopened.audio_output.as_deref(), Some("Headphones"));
        assert!(!reopened.audio_monitoring);
        assert!(reopened.obs_stream);
        assert_eq!(reopened.language, crate::i18n::Language::English);
        assert!(reopened.refresh_inactive);
        assert_eq!(reopened.progress_pet, crate::pets::ProgressPet::Frog);
        assert_eq!(reopened.cat_color, crate::pets::PetColor::Grooving);
        assert_eq!(reopened.bird_color, crate::pets::PetColor::White);
        assert_eq!(reopened.dancing_cats_color, crate::pets::PetColor::Gray);
        assert_eq!(reopened.frog_variant, crate::pets::PetColor::Melancholy);
        assert_eq!(reopened.cursor_pet, crate::pets::CursorPet::Frog);
        assert_eq!(reopened.cursor_pet_speed, 35);
        assert!(reopened.shuffle);
        assert!(!reopened.queue_visible);
        assert_eq!(reopened.theme, crate::theme::Theme::Onyx);
        assert_eq!(reopened.repeat, crate::queue::RepeatMode::One);
        assert!(!reopened.sync_warn_duplicates);
        assert!(reopened.sync_ignore_duplicates);
        Preferences {
            progress_pet: crate::pets::ProgressPet::Pig,
            cursor_pet: crate::pets::CursorPet::Pig,
            pig_variant: crate::pets::PetColor::Gray,
            ..preferences.clone()
        }
        .save(root.path())
        .unwrap();
        let pig = Preferences::load(root.path());
        assert_eq!(pig.progress_pet, crate::pets::ProgressPet::Pig);
        assert_eq!(pig.cursor_pet, crate::pets::CursorPet::Pig);
        assert_eq!(pig.pig_variant, crate::pets::PetColor::Gray);
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
        let upgraded = Preferences::load(root.path());
        assert!(!upgraded.sidebar_collapsed);
        assert!(upgraded.audio_output.is_none());
        assert!(upgraded.audio_monitoring);
        assert!(!upgraded.obs_stream);
        assert_eq!(upgraded.cursor_pet, crate::pets::CursorPet::None);
        assert_eq!(upgraded.cursor_pet_speed, 50);
        assert_eq!(upgraded.volume, 1.0);
        assert!(upgraded.sync_warn_duplicates);
        assert!(!upgraded.sync_ignore_duplicates);
    }
}
