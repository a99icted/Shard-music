use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

enum WriteCommand {
    Save(Preferences),
    Flush(std::sync::mpsc::Sender<()>),
}

/// One ordered writer keeps fsync and atomic replacement away from input handlers.
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
                                *failures.lock().unwrap() =
                                    Some(format!("Не удалось сохранить настройки: {error:#}"));
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
