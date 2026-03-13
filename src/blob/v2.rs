//! SEA blob V2 serializer for Node 24.6.0+.
//!
//! Header layout (9 bytes):
//! ```text
//! Offset  Type        Field
//! 0       u32 (LE)    magic = 0x143da20
//! 4       u32 (LE)    flags (bitfield)
//! 8       u8          exec_argv_extension
//! ```
//!
//! Body is the same as V1, with an additional exec_argv string list
//! appended when the `INCLUDE_EXEC_ARGV` flag is set.

use super::{SEA_MAGIC, SeaFlags, write_string_view};
use crate::error::Result;

/// V2 header size in bytes: magic(4) + flags(4) + exec_argv_extension(1).
pub const V2_HEADER_SIZE: usize = 9;

/// How exec_argv should be extended at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecArgvExtension {
    /// No exec_argv extension.
    None = 0,
    /// Extend via environment variable.
    Env = 1,
    /// Extend via CLI arguments.
    Cli = 2,
}

/// Serialize a V2 SEA blob.
pub fn serialize(
    code_path: &str,
    main_code: &[u8],
    flags: SeaFlags,
    code_cache: Option<&[u8]>,
    assets: Option<&[(&str, &[u8])]>,
    exec_argv: Option<&[&str]>,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    // Determine exec_argv_extension value
    let exec_ext = if flags.contains(SeaFlags::INCLUDE_EXEC_ARGV) {
        ExecArgvExtension::Cli
    } else {
        ExecArgvExtension::None
    };

    // Header: magic + flags + exec_argv_extension
    buf.extend_from_slice(&SEA_MAGIC.to_le_bytes());
    buf.extend_from_slice(&flags.bits().to_le_bytes());
    buf.push(exec_ext as u8);

    // Body: code_path
    write_string_view(&mut buf, code_path.as_bytes());

    // Body: main_code
    write_string_view(&mut buf, main_code);

    // Body: code_cache (only if flag set)
    if flags.contains(SeaFlags::USE_CODE_CACHE) {
        let cache = code_cache.unwrap_or(&[]);
        write_string_view(&mut buf, cache);
    }

    // Body: assets map (only if flag set)
    if flags.contains(SeaFlags::INCLUDE_ASSETS) {
        let assets = assets.unwrap_or(&[]);
        buf.extend_from_slice(&(assets.len() as u64).to_le_bytes());
        for (key, value) in assets {
            write_string_view(&mut buf, key.as_bytes());
            write_string_view(&mut buf, value);
        }
    }

    // Body: exec_argv list (only if flag set)
    if flags.contains(SeaFlags::INCLUDE_EXEC_ARGV) {
        let argv = exec_argv.unwrap_or(&[]);
        buf.extend_from_slice(&(argv.len() as u64).to_le_bytes());
        for arg in argv {
            write_string_view(&mut buf, arg.as_bytes());
        }
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_header_is_9_bytes() {
        let blob = serialize("/sea/main.js", b"code", SeaFlags::empty(), None, None, None).unwrap();
        // magic(4) + flags(4) + exec_argv_extension(1) = 9
        assert_eq!(&blob[0..4], &SEA_MAGIC.to_le_bytes());
        assert_eq!(&blob[4..8], &0u32.to_le_bytes());
        assert_eq!(blob[8], ExecArgvExtension::None as u8);
        // First body field starts at offset 9
        assert_eq!(
            &blob[9..17],
            &(12u64).to_le_bytes() // "/sea/main.js" length
        );
    }

    #[test]
    fn serialize_with_exec_argv() {
        let argv: &[&str] = &["--experimental-vm-modules", "--max-old-space-size=4096"];
        let blob = serialize(
            "/sea/main.js",
            b"code",
            SeaFlags::INCLUDE_EXEC_ARGV,
            None,
            None,
            Some(argv),
        )
        .unwrap();

        // Header should have exec_argv_extension = Cli (2)
        assert_eq!(blob[8], ExecArgvExtension::Cli as u8);

        // After header(9) + code_path(8+12) + main_code(8+4), exec_argv list
        let offset = 9 + (8 + 12) + (8 + 4);
        // Count = 2
        assert_eq!(&blob[offset..offset + 8], &2u64.to_le_bytes());
    }

    #[test]
    fn exec_argv_extension_enum_values() {
        assert_eq!(ExecArgvExtension::None as u8, 0);
        assert_eq!(ExecArgvExtension::Env as u8, 1);
        assert_eq!(ExecArgvExtension::Cli as u8, 2);
    }
}
