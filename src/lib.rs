pub mod appearance;
pub mod artwork;
#[cfg(feature = "desktop")]
pub mod background_video;
pub mod backgrounds;
#[cfg(feature = "desktop")]
pub mod branding;
pub mod db;
#[cfg(feature = "desktop")]
pub mod desktop_audio;
mod download_duplicates;
pub mod download_tools;
pub mod favorites;
pub mod i18n;
pub mod ignored;
pub mod mobile;
pub mod models;
mod music_metadata;
pub mod pets;
pub mod preferences;
mod process_tree;
pub mod queue;
pub mod repositories;
pub mod services;
pub mod spotify;
pub mod statistics;
pub mod storage;
pub mod sync;
pub mod theme;
pub mod volume;
pub mod yandex;
pub mod youtube;
