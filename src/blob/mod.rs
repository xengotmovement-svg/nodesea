//! SEA blob serialization.
//!
//! The blob is the binary payload injected into the Node.js executable.
//! Node's runtime deserializes it at startup using its internal `BlobDeserializer`.
//!
//! Two format versions exist:
//! - **V1** (Node 20.x–24.5.x): 8-byte header (magic + flags)
//! - **V2** (Node 24.6.0+): 9-byte header (magic + flags + exec_argv_extension)

pub mod v1;
pub mod v2;

use bitflags::bitflags;

use crate::error::Result;

/// SEA blob magic number: `0x143da20` (little-endian u32).
/// From `node_sea.h` in the Node.js source.
pub const SEA_MAGIC: u32 = 0x143da20;

/// Blob format version, determined by the target Node.js version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobVersion {
    /// Node 20.x–24.5.x: 8-byte header.
    V1,
    /// Node 24.6.0+: 9-byte header with exec_argv_extension.
    V2,
}

bitflags! {
    /// SEA blob flags bitfield.
    /// These control Node.js runtime behavior when loading the SEA.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SeaFlags: u32 {
        /// No flags set — default behavior.
        const DEFAULT = 0;
        /// Suppress the "ExperimentalWarning: Single executable application" message.
        const DISABLE_EXPERIMENTAL_SEA_WARNING = 1 << 0;
        /// Main payload is a V8 heap snapshot, not JavaScript source.
        const USE_SNAPSHOT = 1 << 1;
        /// A V8 code cache follows the main payload in the blob.
        const USE_CODE_CACHE = 1 << 2;
        /// An assets map is included in the blob (Node 21+).
        const INCLUDE_ASSETS = 1 << 3;
        /// An exec_argv list is included in the blob (Node 24.6+).
        const INCLUDE_EXEC_ARGV = 1 << 4;
    }
}

/// Writes a length-prefixed string/byte slice ("StringView" in Node's terminology).
/// Length is u64 little-endian (64-bit platforms only).
pub fn write_string_view(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u64).to_le_bytes());
    buf.extend_from_slice(data);
}

/// Serialize a SEA blob for the given version.
///
/// # Arguments
/// * `version` — Target blob format version (V1 or V2).
/// * `code_path` — Virtual path for the embedded script (e.g. "/sea/main.js").
/// * `main_code` — JavaScript source bytes.
/// * `flags` — SEA flags bitfield.
/// * `code_cache` — Optional V8 code cache bytes (requires `USE_CODE_CACHE` flag).
/// * `assets` — Optional map of asset name → asset bytes (requires `INCLUDE_ASSETS` flag).
/// * `exec_argv` — Optional exec argv list (V2 only, requires `INCLUDE_EXEC_ARGV` flag).
pub fn serialize(
    version: BlobVersion,
    code_path: &str,
    main_code: &[u8],
    flags: SeaFlags,
    code_cache: Option<&[u8]>,
    assets: Option<&[(&str, &[u8])]>,
    exec_argv: Option<&[&str]>,
) -> Result<Vec<u8>> {
    match version {
        BlobVersion::V1 => v1::serialize(code_path, main_code, flags, code_cache, assets),
        BlobVersion::V2 => {
            v2::serialize(code_path, main_code, flags, code_cache, assets, exec_argv)
        }
    }
}
