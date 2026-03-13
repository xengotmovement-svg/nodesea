//! ELF note-based SEA blob injection (Linux).
//!
//! Appends a `PT_NOTE` segment containing the SEA blob to the ELF binary.
//! The note uses the name `NODE_SEA_BLOB\0` and type 0.

use crate::error::{Error, Result};
use crate::inject::Injector;
use scroll::Pwrite;

/// Note name including null terminator.
const NOTE_NAME: &[u8] = b"NODE_SEA_BLOB\0";
/// Note type (arbitrary, just needs to be consistent).
const NOTE_TYPE: u32 = 0;
/// ELF64 program header entry size.
const PHDR_SIZE: usize = 56;

/// Round `value` up to the next multiple of `alignment`.
fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

/// Build an ELF note section with the given name and descriptor (payload).
///
/// Layout: namesz(u32 LE) + descsz(u32 LE) + type(u32 LE) +
///         name(padded to 4-byte align) + desc(padded to 4-byte align)
fn build_elf_note(name: &[u8], desc: &[u8], note_type: u32) -> Vec<u8> {
    let namesz = name.len() as u32;
    let descsz = desc.len() as u32;
    let name_padded = align_up(name.len(), 4);
    let desc_padded = align_up(desc.len(), 4);
    let total = 12 + name_padded + desc_padded;

    let mut buf = vec![0u8; total];
    let offset = &mut 0;
    buf.gwrite_with(namesz, offset, scroll::LE).unwrap();
    buf.gwrite_with(descsz, offset, scroll::LE).unwrap();
    buf.gwrite_with(note_type, offset, scroll::LE).unwrap();
    buf[12..12 + name.len()].copy_from_slice(name);
    buf[12 + name_padded..12 + name_padded + desc.len()].copy_from_slice(desc);

    buf
}

/// Write an ELF64 program header entry at `offset` in `binary`.
#[allow(clippy::too_many_arguments)]
fn write_phdr64(
    binary: &mut [u8],
    offset: usize,
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
) -> std::result::Result<(), scroll::Error> {
    let o = &mut offset.clone();
    binary.gwrite_with(p_type, o, scroll::LE)?;
    binary.gwrite_with(p_flags, o, scroll::LE)?;
    binary.gwrite_with(p_offset, o, scroll::LE)?;
    binary.gwrite_with(p_vaddr, o, scroll::LE)?;
    binary.gwrite_with(p_paddr, o, scroll::LE)?;
    binary.gwrite_with(p_filesz, o, scroll::LE)?;
    binary.gwrite_with(p_memsz, o, scroll::LE)?;
    binary.gwrite_with(p_align, o, scroll::LE)?;
    Ok(())
}

/// Read a little-endian u16 from `binary` at `offset`.
fn read_u16_le(binary: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([binary[offset], binary[offset + 1]])
}

/// Read a little-endian u64 from `binary` at `offset`.
fn read_u64_le(binary: &[u8], offset: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&binary[offset..offset + 8]);
    u64::from_le_bytes(buf)
}

/// Write a little-endian u16 to `binary` at `offset`.
fn write_u16_le(binary: &mut [u8], offset: usize, value: u16) {
    binary[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

/// Write a little-endian u64 to `binary` at `offset`.
fn write_u64_le(binary: &mut [u8], offset: usize, value: u64) {
    binary[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Injector for the ELF binary format.
pub struct ElfInjector;

impl Injector for ElfInjector {
    fn inject(&self, binary: &mut Vec<u8>, blob: &[u8]) -> Result<()> {
        // Parse to validate this is actually an ELF binary.
        let elf = match goblin::Object::parse(binary) {
            Ok(goblin::Object::Elf(elf)) => elf,
            Ok(_) => return Err(Error::UnsupportedFormat("binary is not ELF format".into())),
            Err(e) => return Err(Error::GoblinError(e.to_string())),
        };

        if !elf.is_64 {
            return Err(Error::UnsupportedFormat(
                "only 64-bit ELF binaries are supported".into(),
            ));
        }

        // Read ELF header fields we need.
        let e_phoff = read_u64_le(binary, 0x20) as usize;
        let e_phentsize = read_u16_le(binary, 0x36) as usize;
        let e_phnum = read_u16_le(binary, 0x38) as usize;

        if e_phentsize != PHDR_SIZE {
            return Err(Error::UnsupportedFormat(format!(
                "unexpected phdr entry size: expected {PHDR_SIZE}, got {e_phentsize}"
            )));
        }

        // Build the ELF note.
        let note = build_elf_note(NOTE_NAME, blob, NOTE_TYPE);

        // Determine where the new phdr entry goes.
        let phdr_table_end = e_phoff + e_phnum * PHDR_SIZE;
        let new_phdr_slot = phdr_table_end;

        // Check if there's space after the existing phdr table for one more entry.
        // The slot must be within file bounds and contain only zeroed bytes.
        let has_space = if new_phdr_slot + PHDR_SIZE <= binary.len() {
            binary[new_phdr_slot..new_phdr_slot + PHDR_SIZE]
                .iter()
                .all(|&b| b == 0)
        } else {
            false
        };

        let actual_new_phdr_offset;

        if has_space {
            // There is space after the existing phdr entries — use it in place.
            actual_new_phdr_offset = new_phdr_slot;
        } else {
            // No space — relocate the entire phdr table to end of file.
            let new_phoff = binary.len();
            // Copy existing phdr table to end of file.
            let existing_phdrs = binary[e_phoff..phdr_table_end].to_vec();
            binary.extend_from_slice(&existing_phdrs);
            // Add space for the new entry (will be written below).
            binary.resize(binary.len() + PHDR_SIZE, 0);

            actual_new_phdr_offset = new_phoff + e_phnum * PHDR_SIZE;

            // Update e_phoff in the ELF header.
            write_u64_le(binary, 0x20, new_phoff as u64);
        }

        // Append the note to the end of the file.
        let note_offset = binary.len();
        binary.extend_from_slice(&note);

        // Write the new PT_NOTE program header entry.
        let note_size = note.len() as u64;
        write_phdr64(
            binary,
            actual_new_phdr_offset,
            4,                  // PT_NOTE
            4,                  // PF_R
            note_offset as u64, // p_offset
            0,                  // p_vaddr
            0,                  // p_paddr
            note_size,          // p_filesz
            note_size,          // p_memsz
            4,                  // p_align
        )
        .map_err(|e| Error::BlobError(format!("failed to write phdr: {e}")))?;

        // Update e_phnum.
        write_u16_le(binary, 0x38, (e_phnum + 1) as u16);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_elf_note_layout() {
        let payload = b"hello";
        let note = build_elf_note(NOTE_NAME, payload, NOTE_TYPE);

        // Header: 3 x u32 = 12 bytes
        let namesz = u32::from_le_bytes(note[0..4].try_into().unwrap());
        let descsz = u32::from_le_bytes(note[4..8].try_into().unwrap());
        let ntype = u32::from_le_bytes(note[8..12].try_into().unwrap());

        assert_eq!(namesz, NOTE_NAME.len() as u32); // 14
        assert_eq!(descsz, payload.len() as u32); // 5
        assert_eq!(ntype, NOTE_TYPE); // 0

        // Name starts at offset 12, padded to 4-byte alignment.
        let name_padded = align_up(NOTE_NAME.len(), 4); // 14 -> 16
        assert_eq!(name_padded, 16);
        assert_eq!(&note[12..12 + NOTE_NAME.len()], NOTE_NAME);
        // Padding bytes should be zero.
        assert_eq!(
            note[12 + NOTE_NAME.len()..12 + name_padded]
                .iter()
                .all(|&b| b == 0),
            true
        );

        // Descriptor starts after padded name.
        let desc_start = 12 + name_padded;
        assert_eq!(&note[desc_start..desc_start + payload.len()], payload);

        // Descriptor is padded to 4-byte alignment.
        let desc_padded = align_up(payload.len(), 4); // 5 -> 8
        assert_eq!(desc_padded, 8);
        assert_eq!(note.len(), 12 + name_padded + desc_padded); // 12 + 16 + 8 = 36
    }

    #[test]
    fn build_elf_note_empty_payload() {
        let note = build_elf_note(NOTE_NAME, b"", NOTE_TYPE);
        let namesz = u32::from_le_bytes(note[0..4].try_into().unwrap());
        let descsz = u32::from_le_bytes(note[4..8].try_into().unwrap());
        assert_eq!(namesz, NOTE_NAME.len() as u32);
        assert_eq!(descsz, 0);
        assert_eq!(note.len(), 12 + align_up(NOTE_NAME.len(), 4));
    }

    /// Build a minimal valid ELF64 binary with one PT_LOAD phdr and zero-filled
    /// space after the phdr table so the injector can use the in-place path.
    fn make_minimal_elf64(extra_phdr_space: bool) -> Vec<u8> {
        let phdr_offset: u64 = 64; // Right after the ELF header.
        let phdr_count: u16 = 1;
        let phdr_entsize: u16 = PHDR_SIZE as u16;
        let phdr_table_end = phdr_offset as usize + PHDR_SIZE;

        // Total size: ELF header (64) + 1 phdr (56) + optional space for one more phdr (56).
        let total = if extra_phdr_space {
            phdr_table_end + PHDR_SIZE
        } else {
            phdr_table_end
        };

        let mut binary = vec![0u8; total];

        // ELF magic.
        binary[0..4].copy_from_slice(b"\x7fELF");
        binary[4] = 2; // ELFCLASS64
        binary[5] = 1; // ELFDATA2LSB
        binary[6] = 1; // EV_CURRENT
        // e_type = ET_EXEC (2)
        binary[16..18].copy_from_slice(&2u16.to_le_bytes());
        // e_machine = EM_X86_64 (62)
        binary[18..20].copy_from_slice(&62u16.to_le_bytes());
        // e_version
        binary[20..24].copy_from_slice(&1u32.to_le_bytes());
        // e_phoff at 0x20
        write_u64_le(&mut binary, 0x20, phdr_offset);
        // e_ehsize at 0x34
        binary[0x34..0x36].copy_from_slice(&64u16.to_le_bytes());
        // e_phentsize at 0x36
        write_u16_le(&mut binary, 0x36, phdr_entsize);
        // e_phnum at 0x38
        write_u16_le(&mut binary, 0x38, phdr_count);

        // Write one PT_LOAD phdr (type = 1) at the phdr offset.
        write_phdr64(
            &mut binary,
            phdr_offset as usize,
            1, // PT_LOAD
            5, // PF_R | PF_X
            0,
            0,
            0,
            total as u64,
            total as u64,
            0x1000,
        )
        .unwrap();

        binary
    }

    #[test]
    fn inject_with_space_for_new_phdr() {
        let mut binary = make_minimal_elf64(true);
        let blob = b"test-sea-blob";
        let original_len = binary.len();

        let injector = ElfInjector;
        injector.inject(&mut binary, blob).unwrap();

        // e_phnum should be incremented from 1 to 2.
        let e_phnum = read_u16_le(&binary, 0x38);
        assert_eq!(e_phnum, 2);

        // e_phoff should remain unchanged (in-place path).
        let e_phoff = read_u64_le(&binary, 0x20);
        assert_eq!(e_phoff, 64);

        // The note should be appended at the original end of the file.
        let note_offset = original_len;
        let expected_note = build_elf_note(NOTE_NAME, blob, NOTE_TYPE);
        assert_eq!(
            &binary[note_offset..note_offset + expected_note.len()],
            &expected_note[..]
        );

        // Verify the new phdr entry (second phdr at offset 64 + 56 = 120).
        let new_phdr_offset = 64 + PHDR_SIZE;
        let p_type = u32::from_le_bytes(
            binary[new_phdr_offset..new_phdr_offset + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(p_type, 4); // PT_NOTE

        // Verify p_offset points to the note.
        let p_offset = read_u64_le(&binary, new_phdr_offset + 8);
        assert_eq!(p_offset, note_offset as u64);

        // Verify p_filesz matches note size.
        let p_filesz = read_u64_le(&binary, new_phdr_offset + 32);
        assert_eq!(p_filesz, expected_note.len() as u64);
    }

    #[test]
    fn inject_relocates_phdr_table_when_no_space() {
        let mut binary = make_minimal_elf64(false);
        let blob = b"relocated-blob";
        let original_len = binary.len();

        let injector = ElfInjector;
        injector.inject(&mut binary, blob).unwrap();

        // e_phnum should be 2.
        let e_phnum = read_u16_le(&binary, 0x38);
        assert_eq!(e_phnum, 2);

        // e_phoff should have been relocated to end of original file.
        let e_phoff = read_u64_le(&binary, 0x20) as usize;
        assert_eq!(e_phoff, original_len);

        // The relocated first phdr should match PT_LOAD (type=1).
        let p_type = u32::from_le_bytes(binary[e_phoff..e_phoff + 4].try_into().unwrap());
        assert_eq!(p_type, 1); // PT_LOAD

        // The second phdr (new PT_NOTE) comes right after.
        let new_phdr_offset = e_phoff + PHDR_SIZE;
        let p_type2 = u32::from_le_bytes(
            binary[new_phdr_offset..new_phdr_offset + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(p_type2, 4); // PT_NOTE

        // The note itself comes after the relocated phdr table.
        let note_offset = e_phoff + 2 * PHDR_SIZE;
        let expected_note = build_elf_note(NOTE_NAME, blob, NOTE_TYPE);
        assert_eq!(
            &binary[note_offset..note_offset + expected_note.len()],
            &expected_note[..]
        );

        // Verify p_offset in the new phdr points to the note.
        let p_offset = read_u64_le(&binary, new_phdr_offset + 8);
        assert_eq!(p_offset, note_offset as u64);
    }

    #[test]
    fn inject_rejects_non_elf() {
        let mut binary = vec![0u8; 64];
        let injector = ElfInjector;
        let err = injector.inject(&mut binary, b"data").unwrap_err();
        assert!(
            err.to_string().contains("parse error")
                || err.to_string().contains("not ELF")
                || err.to_string().contains("unsupported")
        );
    }
}
