//! Node.js version detection from binary.

use std::path::Path;
use std::process::Command;

use semver::Version;

use crate::blob::BlobVersion;
use crate::error::{Error, Result};

/// Minimum Node.js version that supports Single Executable Applications.
const MIN_SEA_VERSION: Version = Version::new(20, 0, 0);

/// First Node.js version that uses the V2 blob format.
const V2_BLOB_VERSION: Version = Version::new(24, 6, 0);

/// Run `node --version` and parse the output into a `semver::Version`.
///
/// The node binary prints something like `v22.12.0\n`. We strip the leading
/// `v` (or `V`) and any surrounding whitespace, then parse with `semver`.
pub fn detect_version(node_path: &Path) -> Result<Version> {
    let output = Command::new(node_path)
        .arg("--version")
        .output()
        .map_err(|e| {
            Error::VersionDetectionFailed(format!("failed to run {}: {e}", node_path.display()))
        })?;

    if !output.status.success() {
        return Err(Error::VersionDetectionFailed(format!(
            "{} --version exited with {}",
            node_path.display(),
            output.status,
        )));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    parse_version_string(&raw)
}

/// Parse a version string like `"v22.12.0"` or `"v22.12.0\n"` into a `semver::Version`.
fn parse_version_string(raw: &str) -> Result<Version> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    Version::parse(trimmed)
        .map_err(|e| Error::VersionDetectionFailed(format!("invalid version string {raw:?}: {e}")))
}

/// Map a Node.js version to the appropriate `BlobVersion`.
///
/// Returns an error if the version is too old to support SEA.
pub fn blob_version(version: &Version) -> Result<BlobVersion> {
    if *version < MIN_SEA_VERSION {
        return Err(Error::UnsupportedNodeVersion(version.to_string()));
    }
    if *version >= V2_BLOB_VERSION {
        Ok(BlobVersion::V2)
    } else {
        Ok(BlobVersion::V1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_v22() {
        let v = parse_version_string("v22.12.0").unwrap();
        assert_eq!(v, Version::new(22, 12, 0));
    }

    #[test]
    fn parse_version_with_newline() {
        let v = parse_version_string("v22.12.0\n").unwrap();
        assert_eq!(v, Version::new(22, 12, 0));
    }

    #[test]
    fn parse_version_invalid() {
        assert!(parse_version_string("not-a-version").is_err());
    }

    #[test]
    fn blob_version_error_for_v18() {
        let v = Version::new(18, 0, 0);
        let err = blob_version(&v).unwrap_err();
        assert!(matches!(err, Error::UnsupportedNodeVersion(_)));
    }

    #[test]
    fn blob_version_v1_for_v20() {
        let v = Version::new(20, 0, 0);
        assert_eq!(blob_version(&v).unwrap(), BlobVersion::V1);
    }

    #[test]
    fn blob_version_v1_for_v22() {
        let v = Version::new(22, 12, 0);
        assert_eq!(blob_version(&v).unwrap(), BlobVersion::V1);
    }

    #[test]
    fn blob_version_v1_for_v24_5() {
        let v = Version::new(24, 5, 0);
        assert_eq!(blob_version(&v).unwrap(), BlobVersion::V1);
    }

    #[test]
    fn blob_version_v2_for_v24_6() {
        let v = Version::new(24, 6, 0);
        assert_eq!(blob_version(&v).unwrap(), BlobVersion::V2);
    }

    #[test]
    fn blob_version_v2_for_v25() {
        let v = Version::new(25, 0, 0);
        assert_eq!(blob_version(&v).unwrap(), BlobVersion::V2);
    }
}
