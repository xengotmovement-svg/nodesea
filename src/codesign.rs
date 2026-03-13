//! macOS ad-hoc code signing.

use std::path::Path;

use crate::error::{Error, Result};

/// Ad-hoc sign a binary so macOS will allow it to run after modification.
///
/// Runs `codesign --sign - --force <path>`.
#[cfg(target_os = "macos")]
pub fn codesign_adhoc(path: &Path) -> Result<()> {
    let output = std::process::Command::new("codesign")
        .args(["--sign", "-", "--force"])
        .arg(path)
        .output()
        .map_err(|e| Error::CodesignFailed(format!("failed to run codesign: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::CodesignFailed(stderr.into_owned()));
    }

    Ok(())
}

/// Verify the code signature of a binary.
///
/// Returns `Ok(true)` if the signature is valid, `Ok(false)` if invalid.
#[cfg(target_os = "macos")]
pub fn verify_signature(path: &Path) -> Result<bool> {
    let output = std::process::Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(path)
        .output()
        .map_err(|e| Error::CodesignFailed(format!("failed to run codesign: {e}")))?;

    Ok(output.status.success())
}

/// Stub: ad-hoc signing is only available on macOS.
#[cfg(not(target_os = "macos"))]
pub fn codesign_adhoc(_path: &Path) -> Result<()> {
    Err(Error::CodesignFailed(
        "code signing is only supported on macOS".to_string(),
    ))
}

/// Stub: signature verification is only available on macOS.
#[cfg(not(target_os = "macos"))]
pub fn verify_signature(_path: &Path) -> Result<bool> {
    Err(Error::CodesignFailed(
        "code signing is only supported on macOS".to_string(),
    ))
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_codesign_and_verify() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let dest = dir.path().join("true_copy");

        // Copy /usr/bin/true to a temp file.
        fs::copy("/usr/bin/true", &dest).expect("failed to copy /usr/bin/true");

        // The copy retains Apple's signature. Strip it first so we start
        // with an unsigned binary, then remove the extended attributes that
        // macOS may attach.
        let strip = std::process::Command::new("codesign")
            .args(["--remove-signature"])
            .arg(&dest)
            .output()
            .expect("failed to strip signature");
        assert!(strip.status.success(), "failed to strip signature");

        // After stripping, the binary should fail strict verification.
        let valid_before = verify_signature(&dest).expect("verify_signature failed");
        assert!(!valid_before, "signature should be invalid after stripping");

        // Re-sign with ad-hoc signature.
        codesign_adhoc(&dest).expect("codesign_adhoc failed");

        // Verify the new signature is valid.
        let valid = verify_signature(&dest).expect("verify_signature failed");
        assert!(valid, "signature should be valid after ad-hoc signing");
    }
}
