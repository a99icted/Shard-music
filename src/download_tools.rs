use crate::i18n::tr;
use anyhow::{Result, bail};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// Absolute executable paths keep downloads independent of the launcher's PATH.
#[derive(Debug)]
pub struct DownloadTools {
    pub yt_dlp: PathBuf,
    pub ffmpeg_directory: PathBuf,
    javascript: Option<(String, PathBuf)>,
}

impl DownloadTools {
    pub fn discover(data_directory: &Path) -> Result<Self> {
        let mut directories = Vec::new();
        if let Some(directory) =
            env::var_os("SHARD_TOOLS_DIR").or_else(|| env::var_os("LOZEX_TOOLS_DIR"))
        {
            directories.push(PathBuf::from(directory));
        }
        directories.push(data_directory.join("tools"));
        if let Ok(executable) = env::current_exe()
            && let Some(parent) = executable.parent()
        {
            directories.push(parent.join("tools"));
            directories.push(parent.to_path_buf());
            // `cargo run` and integration tests live under target/{debug,release}[/deps].
            let profile = if parent
                .file_name()
                .is_some_and(|name| name == "deps" || name == "examples")
            {
                parent.parent()
            } else {
                Some(parent)
            };
            if let Some(target) = profile.and_then(Path::parent)
                && target.file_name().is_some_and(|name| name == "target")
                && let Some(project) = target.parent()
            {
                directories.push(project.join("tools"));
            }
        }
        if let Some(paths) = env::var_os("PATH") {
            directories.extend(env::split_paths(&paths).filter(|p| !p.as_os_str().is_empty()));
        }
        // Standard MSI installs of Node need not be in an already running app's PATH.
        #[cfg(windows)]
        if let Some(program_files) = env::var_os("ProgramFiles") {
            directories.push(PathBuf::from(program_files).join("nodejs"));
        }
        Self::from_directories(&directories)
    }

    fn from_directories(directories: &[PathBuf]) -> Result<Self> {
        let Some(yt_dlp) = find_executable("yt-dlp", directories) else {
            bail!(tr(
                "Не найден компонент загрузки yt-dlp. Запустите scripts/setup-youtube.ps1 из папки Shard или положите yt-dlp рядом с приложением в папку tools"
            ));
        };
        let Some(ffmpeg) = find_executable("ffmpeg", directories) else {
            bail!(tr(
                "Не найден компонент обработки аудио FFmpeg. Запустите scripts/setup-youtube.ps1 из папки Shard или положите FFmpeg в папку tools"
            ));
        };
        let javascript = directories.iter().find_map(|directory| {
            ["deno", "node"].into_iter().find_map(|name| {
                find_executable(name, std::slice::from_ref(directory))
                    .map(|path| (name.to_owned(), path))
            })
        });
        Ok(Self {
            yt_dlp,
            ffmpeg_directory: ffmpeg
                .parent()
                .expect("executable has parent")
                .to_path_buf(),
            javascript,
        })
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.yt_dlp);
        command.arg("--ffmpeg-location").arg(&self.ffmpeg_directory);
        if let Some((runtime, path)) = &self.javascript {
            let mut argument = OsString::from(format!("{runtime}:"));
            argument.push(path);
            command.arg("--js-runtimes").arg(argument);
        }
        command
    }
}

fn find_executable(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    let filename = format!("{name}{}", env::consts::EXE_SUFFIX);
    directories.iter().find_map(|directory| {
        let candidate = directory.join(&filename);
        candidate
            .is_file()
            .then(|| std::fs::canonicalize(candidate).ok())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn executable(directory: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(directory).unwrap();
        let path = directory.join(format!("{name}{}", env::consts::EXE_SUFFIX));
        fs::write(&path, b"fixture").unwrap();
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn finds_local_components_without_path_and_preserves_paths_with_spaces() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("Shard tools с пробелами");
        let yt_dlp = executable(&directory, "yt-dlp");
        let ffmpeg = executable(&directory, "ffmpeg");
        let node = executable(&directory, "node");
        let tools = DownloadTools::from_directories(&[directory]).unwrap();
        let command = tools.command();
        assert_eq!(Path::new(command.get_program()), yt_dlp);
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments[0], "--ffmpeg-location");
        assert_eq!(Path::new(arguments[1]), ffmpeg.parent().unwrap());
        assert_eq!(arguments[2], "--js-runtimes");
        let mut expected = OsString::from("node:");
        expected.push(node);
        assert_eq!(arguments[3], expected);
    }

    #[test]
    fn prefers_bundled_tools_to_path_and_checks_ffmpeg_before_downloading() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local");
        let path = root.path().join("path");
        let preferred = executable(&local, "yt-dlp");
        executable(&path, "yt-dlp");
        let directories = [local, path.clone()];
        assert!(
            format!(
                "{:#}",
                DownloadTools::from_directories(&directories).unwrap_err()
            )
            .contains("FFmpeg")
        );
        executable(&path, "ffmpeg");
        assert_eq!(
            DownloadTools::from_directories(&directories)
                .unwrap()
                .yt_dlp,
            preferred
        );
    }

    #[test]
    fn missing_downloader_has_an_actionable_error() {
        let error = DownloadTools::from_directories(&[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("yt-dlp"));
        assert!(error.contains("setup-youtube.ps1"));
    }

    #[test]
    fn bundled_node_is_preferred_to_a_deno_elsewhere_on_path() {
        let root = tempfile::tempdir().unwrap();
        let bundled = root.path().join("application/tools");
        let external = root.path().join("system");
        executable(&bundled, "yt-dlp");
        executable(&bundled, "ffmpeg");
        let node = executable(&bundled, "node");
        executable(&external, "deno");
        let tools = DownloadTools::from_directories(&[bundled, external]).unwrap();
        assert_eq!(tools.javascript, Some(("node".into(), node)));
    }
}
