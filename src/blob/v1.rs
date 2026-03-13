//! SEA blob V1 serializer for Node 20.x–24.5.x.
//!
//! Header layout (8 bytes):
//! ```text
//! Offset  Type        Field
//! 0       u32 (LE)    magic = 0x143da20
//! 4       u32 (LE)    flags (bitfield)
//! ```
//!
//! Body (each field is u64 LE length prefix + raw bytes):
//! - code_path
//! - main_code (JS source or V8 snapshot)
//! - code_cache (only if USE_CODE_CACHE flag)
//! - assets map (only if INCLUDE_ASSETS flag): u64 count + (key + value) pairs

use super::{SEA_MAGIC, SeaFlags, write_string_view};
use crate::error::Result;

/// V1 header size in bytes: magic(4) + flags(4).
pub const V1_HEADER_SIZE: usize = 8;

/// Serialize a V1 SEA blob.
pub fn serialize(
    code_path: &str,
    main_code: &[u8],
    flags: SeaFlags,
    code_cache: Option<&[u8]>,
    assets: Option<&[(&str, &[u8])]>,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    // Header: magic + flags
    buf.extend_from_slice(&SEA_MAGIC.to_le_bytes());
    buf.extend_from_slice(&flags.bits().to_le_bytes());

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
        // Write count as u64 LE
        buf.extend_from_slice(&(assets.len() as u64).to_le_bytes());
        for (key, value) in assets {
            write_string_view(&mut buf, key.as_bytes());
            write_string_view(&mut buf, value);
        }
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_minimal_blob() {
        let blob = serialize(
            "/sea/main.js",
            b"console.log('hi')",
            SeaFlags::empty(),
            None,
            None,
        )
        .unwrap();

        // Header: 8 bytes
        assert_eq!(&blob[0..4], &SEA_MAGIC.to_le_bytes()); // magic
        assert_eq!(&blob[4..8], &0u32.to_le_bytes()); // flags = 0

        // code_path: u64 len + bytes
        let code_path = b"/sea/main.js";
        assert_eq!(&blob[8..16], &(code_path.len() as u64).to_le_bytes());
        assert_eq!(&blob[16..16 + code_path.len()], code_path.as_slice());

        // main_code: u64 len + bytes
        let main_code = b"console.log('hi')";
        let offset = 16 + code_path.len();
        assert_eq!(
            &blob[offset..offset + 8],
            &(main_code.len() as u64).to_le_bytes()
        );
        assert_eq!(
            &blob[offset + 8..offset + 8 + main_code.len()],
            main_code.as_slice()
        );

        // Total size: 8 (header) + 8+12 (code_path) + 8+17 (main_code) = 53
        assert_eq!(blob.len(), 8 + 8 + 12 + 8 + 17);
    }

    #[test]
    fn serialize_with_disable_warning_flag() {
        let blob = serialize(
            "/sea/main.js",
            b"code",
            SeaFlags::DISABLE_EXPERIMENTAL_SEA_WARNING,
            None,
            None,
        )
        .unwrap();

        assert_eq!(&blob[4..8], &1u32.to_le_bytes());
    }

    #[test]
    fn serialize_with_code_cache() {
        let cache = b"fake_cache_data";
        let blob = serialize(
            "/sea/main.js",
            b"code",
            SeaFlags::USE_CODE_CACHE,
            Some(cache.as_slice()),
            None,
        )
        .unwrap();

        // After header + code_path + main_code, code_cache should follow
        let offset = 8 + (8 + 12) + (8 + 4); // header + code_path + main_code
        assert_eq!(
            &blob[offset..offset + 8],
            &(cache.len() as u64).to_le_bytes()
        );
        assert_eq!(
            &blob[offset + 8..offset + 8 + cache.len()],
            cache.as_slice()
        );
    }

    #[test]
    fn serialize_with_assets() {
        let assets: &[(&str, &[u8])] = &[("config.json", b"{}")];
        let blob = serialize(
            "/sea/main.js",
            b"code",
            SeaFlags::INCLUDE_ASSETS,
            None,
            Some(assets),
        )
        .unwrap();

        // After header + code_path + main_code, assets map should follow
        let offset = 8 + (8 + 12) + (8 + 4);
        // Count = 1
        assert_eq!(&blob[offset..offset + 8], &1u64.to_le_bytes());
        // Key: "config.json"
        let key_offset = offset + 8;
        assert_eq!(&blob[key_offset..key_offset + 8], &(11u64).to_le_bytes());
        assert_eq!(&blob[key_offset + 8..key_offset + 8 + 11], b"config.json");
    }

    #[test]
    fn flags_bitfield_values() {
        assert_eq!(SeaFlags::empty().bits(), 0);
        assert_eq!(SeaFlags::DISABLE_EXPERIMENTAL_SEA_WARNING.bits(), 1);
        assert_eq!(SeaFlags::USE_SNAPSHOT.bits(), 2);
        assert_eq!(SeaFlags::USE_CODE_CACHE.bits(), 4);
        assert_eq!(SeaFlags::INCLUDE_ASSETS.bits(), 8);
        assert_eq!(SeaFlags::INCLUDE_EXEC_ARGV.bits(), 16);
    }
}
