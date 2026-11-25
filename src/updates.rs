//! Windows updates from the stable releases of the official GitHub repository.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const RELEASES_URL: &str = "https://github.com/a99icted/Shard-music/releases";
const LATEST_URL: &str = "https://api.github.com/repos/a99icted/Shard-music/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/a99icted/Shard-music/releases/download/";
const MAX_INSTALLER_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct AvailableUpdate {
    pub version: String,
    pub size: u64,
    pub release_url: String,
    installer_url: String,
    checksum_url: String,
    filename: String,
}

#[cfg(debug_assertions)]
impl AvailableUpdate {
    /// A non-downloadable fixture for the isolated layout preview only.
    pub fn preview() -> Self {
        Self {
            version: "0.1.9".into(),
            size: 150 * 1024 * 1024,
            release_url: RELEASES_URL.into(),
            installer_url: String::new(),
            checksum_url: String::new(),
            filename: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: u64,
}

pub struct DownloadedUpdate {
    directory: tempfile::TempDir,
    installer: PathBuf,
    pub version: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}

fn version(value: &str) -> Result<[u64; 3]> {
    let parts: Vec<_> = value
        .strip_prefix('v')
        .unwrap_or(value)
        .split('.')
        .collect();
    ensure!(parts.len() == 3, "Invalid release version");
    let mut numbers = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        ensure!(
            !part.is_empty()
                && part.bytes().all(|c| c.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0')),
            "Invalid release version"
        );
        numbers[index] = part.parse().context("Invalid release version")?;
    }
    Ok(numbers)
}

fn parse_release(json: &str, current: &str) -> Result<Option<AvailableUpdate>> {
    let release: Release = serde_json::from_str(json).context("Invalid GitHub release response")?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let next = version(&release.tag_name)?;
    if next <= version(current)? {
        return Ok(None);
    }
    let name = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let filename = format!("Shard-{name}-Setup-x64.exe");
    let find = |name: &str| -> Result<&Asset> {
        let mut matches = release.assets.iter().filter(|asset| asset.name == name);
        let asset = matches
            .next()
            .context("Release has no Windows installer or SHA-256 file")?;
        ensure!(matches.next().is_none(), "Duplicate release asset");
        ensure!(
            asset.browser_download_url == format!("{DOWNLOAD_PREFIX}{}/{name}", release.tag_name),
            "Update asset does not belong to the official release"
        );
        Ok(asset)
    };
    let installer = find(&filename)?;
    let checksum = find(&format!("{filename}.sha256"))?;
    ensure!(
        installer.size > 0 && installer.size <= MAX_INSTALLER_BYTES,
        "Invalid installer size"
    );
    ensure!(
        checksum.size > 0 && checksum.size <= 4096,
        "Invalid checksum size"
    );
    Ok(Some(AvailableUpdate {
        version: name.into(),
        size: installer.size,
        release_url: format!("{RELEASES_URL}/tag/{}", release.tag_name),
        installer_url: installer.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        filename,
    }))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent()
}

pub fn check(current: &str) -> Result<Option<AvailableUpdate>> {
    let mut response = match agent(Duration::from_secs(25))
        .get(LATEST_URL)
        .header("User-Agent", concat!("Shard/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
    {
        // A repository with no public stable releases has nothing to offer.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        response => response.context("Could not check GitHub Releases")?,
    };
    let json = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()?;
    parse_release(&json, current)
}

fn parse_checksum(contents: &str, filename: &str) -> Result<[u8; 32]> {
    let entries: Vec<_> = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    ensure!(entries.len() == 1, "Invalid update checksum file");
    let fields: Vec<_> = entries[0].split_whitespace().collect();
    ensure!(
        fields.len() == 2 && fields[1].trim_start_matches('*') == filename,
        "Checksum filename mismatch"
    );
    let hex = fields[0];
    ensure!(
        hex.len() == 64 && hex.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid SHA-256 checksum"
    );
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)?;
    }
    Ok(digest)
}

fn receive_installer(
    reader: &mut impl Read,
    file: &mut File,
    expected_size: u64,
    expected_hash: [u8; 32],
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    let mut hash = Sha256::new();
    let mut received = 0;
    let mut buffer = [0; 128 * 1024];
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Update download cancelled");
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        received += count as u64;
        ensure!(
            received <= expected_size && received <= MAX_INSTALLER_BYTES,
            "Update exceeds advertised size"
        );
        hash.update(&buffer[..count]);
        file.write_all(&buffer[..count])?;
        progress(DownloadProgress {
            received,
            total: expected_size,
        });
    }
    ensure!(received == expected_size, "Incomplete update download");
    ensure!(
        <[u8; 32]>::from(hash.finalize()) == expected_hash,
        "Update SHA-256 verification failed"
    );
    file.sync_all()?;
    Ok(())
}

pub fn download(
    update: &AvailableUpdate,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<DownloadedUpdate> {
    ensure!(!cancel.load(Ordering::Relaxed), "Update download cancelled");
    let client = agent(Duration::from_secs(20 * 60));
    let checksum = client
        .get(&update.checksum_url)
        .header("User-Agent", "Shard-updater")
        .call()?
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_string()?;
    let expected_hash = parse_checksum(&checksum, &update.filename)?;
    // The temporary directory is private to this attempt and is removed on
    // failure/cancellation; no installer can launch before complete validation.
    let directory = tempfile::Builder::new().prefix("shard-update-").tempdir()?;
    let installer = directory.path().join(&update.filename);
    let mut file = File::create(&installer)?;
    let mut response = client
        .get(&update.installer_url)
        .header("User-Agent", "Shard-updater")
        .call()?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_INSTALLER_BYTES + 1)
        .reader();
    receive_installer(
        &mut reader,
        &mut file,
        update.size,
        expected_hash,
        cancel,
        &mut progress,
    )?;
    drop(file);
    Ok(DownloadedUpdate {
        directory,
        installer,
        version: update.version.clone(),
    })
}

/// Only the stable installed edition can replace a stable installation.
pub fn installation_directory() -> Result<PathBuf> {
    ensure!(
        !crate::app_identity::IS_TEST,
        "Automatic updates are disabled in the isolated test edition"
    );
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{5CBCE71D-68EF-4E41-9565-401586E1F370}_is1"
        ).context("Install Shard using its Windows installer before using automatic updates")?;
        let directory = PathBuf::from(key.get_value::<String, _>("InstallLocation")?);
        let executable = std::env::current_exe()?.canonicalize()?;
        ensure!(
            directory.join("shard.exe").canonicalize()? == executable,
            "Running executable does not match the installed Shard edition"
        );
        ensure!(
            directory.join("unins000.exe").is_file(),
            "Shard uninstaller is missing"
        );
        Ok(directory)
    }
    #[cfg(not(windows))]
    anyhow::bail!("Automatic installation is currently available on Windows only")
}

impl DownloadedUpdate {
    pub fn launch(self, install_directory: &Path) -> Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // InitializeSetup in this installer waits for our PID to exit.
            // Installation and restart happen only after normal quit hooks flush
            // settings and release the library and the single-instance mutex.
            let log = self.directory.path().join("install.log");
            std::process::Command::new(&self.installer)
                .args([
                    "/VERYSILENT",
                    "/SUPPRESSMSGBOXES",
                    "/NORESTART",
                    "/SP-",
                    "/CURRENTUSER",
                    "/SHARDUPDATE=1",
                ])
                .arg(format!("/SHARDPID={}", std::process::id()))
                .arg(format!("/DIR={}", install_directory.display()))
                .arg(format!("/LOG={}", log.display()))
                .creation_flags(0x08000000)
                .spawn()
                .context("Could not start the update installer")?;
            // The installer and log must outlive this process.
            let _ = self.directory.keep();
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = install_directory;
            anyhow::bail!("Automatic installation is currently available on Windows only")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> serde_json::Value {
        let v = tag.trim_start_matches('v');
        let name = format!("Shard-{v}-Setup-x64.exe");
        serde_json::json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":[
            {"name":name,"size":123,"browser_download_url":format!("{DOWNLOAD_PREFIX}{tag}/{name}")},
            {"name":format!("{name}.sha256"),"size":100,"browser_download_url":format!("{DOWNLOAD_PREFIX}{tag}/{name}.sha256")}
        ]})
    }

    #[test]
    fn selects_only_new_stable_complete_official_windows_releases() {
        assert_eq!(
            parse_release(&release("v0.1.10").to_string(), "0.1.9")
                .unwrap()
                .unwrap()
                .version,
            "0.1.10"
        );
        for tag in ["v0.1.8", "0.1.7"] {
            assert!(
                parse_release(&release(tag).to_string(), "0.1.8")
                    .unwrap()
                    .is_none()
            );
        }
        for flag in ["draft", "prerelease"] {
            let mut data = release("v0.2.0");
            data[flag] = true.into();
            assert!(parse_release(&data.to_string(), "0.1.8").unwrap().is_none());
        }
        let mut data = release("v0.2.0");
        data["assets"][0]["browser_download_url"] = "https://evil.example/setup.exe".into();
        assert!(parse_release(&data.to_string(), "0.1.8").is_err());
        let mut data = release("v0.2.0");
        data["assets"].as_array_mut().unwrap().pop();
        assert!(parse_release(&data.to_string(), "0.1.8").is_err());
        for tag in [
            "v0.2.0-test",
            "../0.2.0",
            "v00.2.0",
            "v0.2",
            "v18446744073709551616.0.0",
        ] {
            assert!(parse_release(&release(tag).to_string(), "0.1.8").is_err());
        }
    }

    #[test]
    fn verifies_download_size_digest_and_cancellation_before_installation() {
        let bytes = b"installer fixture";
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        let cancel = AtomicBool::new(false);
        let mut file = tempfile::tempfile().unwrap();
        let mut progress = Vec::new();
        receive_installer(
            &mut &bytes[..],
            &mut file,
            bytes.len() as u64,
            hash,
            &cancel,
            &mut |p| progress.push(p.received),
        )
        .unwrap();
        assert_eq!(progress.last(), Some(&(bytes.len() as u64)));
        for (size, digest) in [
            (bytes.len() as u64 + 1, hash),
            (2, hash),
            (bytes.len() as u64, [0; 32]),
        ] {
            assert!(
                receive_installer(
                    &mut &bytes[..],
                    &mut file,
                    size,
                    digest,
                    &cancel,
                    &mut |_| {}
                )
                .is_err()
            );
        }
        cancel.store(true, Ordering::Relaxed);
        assert!(
            receive_installer(
                &mut &bytes[..],
                &mut file,
                bytes.len() as u64,
                hash,
                &cancel,
                &mut |_| {}
            )
            .is_err()
        );
        let hex = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(
            parse_checksum(&format!("{hex}  setup.exe\n"), "setup.exe").unwrap(),
            hash
        );
        assert!(parse_checksum(&format!("{hex}  other.exe"), "setup.exe").is_err());
        assert!(parse_checksum("garbage  setup.exe", "setup.exe").is_err());
    }
}
