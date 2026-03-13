//! SEA fuse sentinel scanner and flipper.
//!
//! Node.js embeds a fuse sentinel in every binary. When the last byte is `0`,
//! the binary runs normally. Flipping it to `1` tells the Node.js runtime to
//! look for an injected SEA blob instead of executing `node` as usual.

use crate::error::{Error, Result};

/// The fuse sentinel in its default (disabled) state.
pub const FUSE_SENTINEL: &[u8] = b"NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2:0";

/// The fuse sentinel after being flipped (enabled).
pub const FUSE_ENABLED: &[u8] = b"NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2:1";

/// The common prefix shared by both fuse states.
const FUSE_PREFIX: &[u8] = b"NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2:";

/// Find the byte offset of the fuse sentinel in `binary`.
///
/// Returns `Some(offset)` if either the disabled (`:0`) or enabled (`:1`)
/// sentinel is found, `None` otherwise.
pub fn find_fuse(binary: &[u8]) -> Option<usize> {
    let prefix_len = FUSE_PREFIX.len();
    if binary.len() < prefix_len + 1 {
        return None;
    }
    for i in 0..=binary.len() - (prefix_len + 1) {
        if binary[i..i + prefix_len] == *FUSE_PREFIX {
            let state = binary[i + prefix_len];
            if state == b'0' || state == b'1' {
                return Some(i);
            }
        }
    }
    None
}

/// Flip the SEA fuse from disabled (`:0`) to enabled (`:1`).
///
/// # Errors
///
/// - [`Error::FuseNotFound`] if the sentinel is not present.
/// - [`Error::FuseAlreadyEnabled`] if the fuse has already been flipped.
pub fn flip_fuse(binary: &mut [u8]) -> Result<()> {
    let offset = find_fuse(binary).ok_or(Error::FuseNotFound)?;
    let state_offset = offset + FUSE_PREFIX.len();
    if binary[state_offset] == b'1' {
        return Err(Error::FuseAlreadyEnabled);
    }
    binary[state_offset] = b'1';
    Ok(())
}

/// Check whether the SEA fuse is already enabled (`:1`).
pub fn is_fuse_enabled(binary: &[u8]) -> bool {
    match find_fuse(binary) {
        Some(offset) => binary[offset + FUSE_PREFIX.len()] == b'1',
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic binary: random-ish padding, fuse, more padding.
    fn make_binary(fuse: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0x42, 0x13]);
        buf.extend_from_slice(fuse);
        buf.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
        buf
    }

    #[test]
    fn find_fuse_in_synthetic_binary() {
        let binary = make_binary(FUSE_SENTINEL);
        let offset = find_fuse(&binary);
        assert_eq!(offset, Some(8));
    }

    #[test]
    fn flip_fuse_changes_zero_to_one() {
        let mut binary = make_binary(FUSE_SENTINEL);
        assert!(flip_fuse(&mut binary).is_ok());
        assert_eq!(&binary[8..8 + FUSE_ENABLED.len()], FUSE_ENABLED,);
    }

    #[test]
    fn detect_already_enabled_fuse() {
        let mut binary = make_binary(FUSE_ENABLED);
        let err = flip_fuse(&mut binary).unwrap_err();
        assert!(
            matches!(err, Error::FuseAlreadyEnabled),
            "expected FuseAlreadyEnabled, got {err:?}",
        );
    }

    #[test]
    fn fuse_not_found_on_empty_binary() {
        let mut binary = Vec::new();
        let err = flip_fuse(&mut binary).unwrap_err();
        assert!(
            matches!(err, Error::FuseNotFound),
            "expected FuseNotFound, got {err:?}",
        );
    }

    #[test]
    fn is_fuse_enabled_after_flip() {
        let mut binary = make_binary(FUSE_SENTINEL);
        assert!(!is_fuse_enabled(&binary));
        flip_fuse(&mut binary).unwrap();
        assert!(is_fuse_enabled(&binary));
    }
}
