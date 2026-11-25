//! The test edition has its own installation, profile and instance identity.
use std::ffi::OsString;

pub const IS_TEST: bool = cfg!(feature = "test-build");
pub const NAME: &str = if IS_TEST { "Shard test" } else { "Shard" };
pub const STORAGE_NAME: &str = if IS_TEST { "Shard Test" } else { "Shard" };
pub const HEADER: &str = if IS_TEST { "SHARD TEST" } else { "SHARD" };
pub const VERSION: &str = if IS_TEST {
    concat!(env!("CARGO_PKG_VERSION"), "-test")
} else {
    env!("CARGO_PKG_VERSION")
};

pub fn data_directory_override() -> Option<OsString> {
    data_directory_override_from(|key| std::env::var_os(key))
}

fn data_directory_override_from(
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) -> Option<OsString> {
    if IS_TEST {
        lookup("SHARD_TEST_DATA_DIR")
    } else {
        lookup("SHARD_DATA_DIR").or_else(|| lookup("LOZEX_DATA_DIR"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edition_uses_only_its_own_data_directory_override() {
        let release_only = |key: &str| match key {
            "SHARD_DATA_DIR" => Some(OsString::from("release-library")),
            "LOZEX_DATA_DIR" => Some(OsString::from("legacy-library")),
            _ => None,
        };
        assert_eq!(
            data_directory_override_from(release_only),
            if IS_TEST {
                None
            } else {
                Some("release-library".into())
            }
        );
        assert_eq!(
            data_directory_override_from(|key| {
                (key == "SHARD_TEST_DATA_DIR").then(|| "test-library".into())
            }),
            if IS_TEST {
                Some("test-library".into())
            } else {
                None
            }
        );
        if IS_TEST {
            let release = directories::ProjectDirs::from("", "Shard", "Shard").unwrap();
            assert_ne!(
                crate::storage::config_directory().unwrap(),
                release.data_local_dir()
            );
        }
    }
}
