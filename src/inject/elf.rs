//! ELF note-based SEA blob injection (Linux).
//!
//! Injects the SEA blob by appending data to the end of the ELF file
//! and creating a new PT_LOAD + PT_NOTE pair at a virtual address above
//! all existing segments. The phdr table is also relocated into the new
//! PT_LOAD so that it's mapped and accessible via AT_PHDR.
//!
//! Key constraints:
//! - Existing program headers are NOT modified (avoids BSS corruption)
//! - `p_offset % p_align == p_vaddr % p_align` for the new PT_LOAD
//! - The combined phdr table is within the new PT_LOAD for AT_PHDR
//!
//! At runtime, Node.js uses postject's `dl_iterate_phdr`-based lookup,
//! which walks PT_NOTE segments and accesses note data via
//! `dlpi_addr + p_vaddr`.

use crate::error::{Error, Result};
use crate::inject::Injector;
use scroll::Pwrite;

/// Note name including null terminator.
const NOTE_NAME: &[u8] = b"NODE_SEA_BLOB\0";
/// Note type (arbitrary, just needs to be consistent).
const NOTE_TYPE: u32 = 0;
/// ELF64 program header entry size.
const PHDR_SIZE: usize = 56;
/// Page size for alignment.
const PAGE_SIZE: u64 = 0x1000;

/// PT_LOAD type constant.
const PT_LOAD: u32 = 1;
/// PT_NOTE type constant.
const PT_NOTE: u32 = 4;
/// PF_R (readable) flag.
const PF_R: u32 = 4;

/// Round `value` up to the next multiple of `alignment`.
fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn align_up_u64(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

/// Build an ELF note with the given name and descriptor (payload).
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

fn read_u16_le(binary: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([binary[offset], binary[offset + 1]])
}

fn read_u32_le(binary: &[u8], offset: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&binary[offset..offset + 4]);
    u32::from_le_bytes(buf)
}

fn read_u64_le(binary: &[u8], offset: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&binary[offset..offset + 8]);
    u64::from_le_bytes(buf)
}

fn write_u16_le(binary: &mut [u8], offset: usize, value: u16) {
    binary[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u64_le(binary: &mut [u8], offset: usize, value: u64) {
    binary[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Find the highest virtual address end (vaddr + memsz) across all PT_LOAD segments.
fn find_max_vaddr_end(binary: &[u8], e_phoff: usize, e_phnum: usize) -> u64 {
    let mut max: u64 = 0;
    for i in 0..e_phnum {
        let off = e_phoff + i * PHDR_SIZE;
        if read_u32_le(binary, off) == PT_LOAD {
            let vaddr = read_u64_le(binary, off + 16);
            let memsz = read_u64_le(binary, off + 40);
            let end = vaddr + memsz;
            if end > max {
                max = end;
            }
        }
    }
    max
}

pub struct ElfInjector;

impl Injector for ElfInjector {
    fn inject(&self, binary: &mut Vec<u8>, blob: &[u8]) -> Result<()> {
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

        let e_phoff = read_u64_le(binary, 0x20) as usize;
        let e_phentsize = read_u16_le(binary, 0x36) as usize;
        let e_phnum = read_u16_le(binary, 0x38) as usize;

        if e_phentsize != PHDR_SIZE {
            return Err(Error::UnsupportedFormat(format!(
                "unexpected phdr entry size: expected {PHDR_SIZE}, got {e_phentsize}"
            )));
        }

        // Build the note data.
        let note = build_elf_note(NOTE_NAME, blob, NOTE_TYPE);
        let note_size = note.len() as u64;

        // Choose a virtual address above all existing segments.
        let max_vaddr = find_max_vaddr_end(binary, e_phoff, e_phnum);
        let seg_vaddr = align_up_u64(max_vaddr, PAGE_SIZE);

        // We need the ELF constraint: p_offset % p_align == p_vaddr % p_align.
        // Since seg_vaddr is page-aligned (% PAGE_SIZE == 0), we need
        // p_offset % PAGE_SIZE == 0 as well.
        let seg_file_offset = align_up(binary.len(), PAGE_SIZE as usize);
        // Pad file to the aligned offset.
        binary.resize(seg_file_offset, 0);

        // Layout within our new segment:
        //   [note data] [combined phdr table]
        // The combined table = existing phdrs + PT_LOAD + PT_NOTE (2 new entries).
        let note_offset_in_seg = 0usize;
        let phdr_table_offset_in_seg = note.len();
        let new_phnum = e_phnum + 2;
        let combined_table_size = new_phnum * PHDR_SIZE;
        let seg_total_size = note.len() + combined_table_size;

        // Append note data.
        binary.extend_from_slice(&note);

        // Append existing phdr table entries.
        let existing_phdrs = binary[e_phoff..e_phoff + e_phnum * PHDR_SIZE].to_vec();
        binary.extend_from_slice(&existing_phdrs);

        // Compute addresses for the two new phdr entries.
        let note_file_off = (seg_file_offset + note_offset_in_seg) as u64;
        let note_vaddr = seg_vaddr + note_offset_in_seg as u64;
        let seg_memsz = align_up_u64(seg_total_size as u64, PAGE_SIZE);

        // Append PT_LOAD entry for our new segment.
        let pt_load_offset = binary.len();
        binary.resize(binary.len() + PHDR_SIZE, 0);
        write_phdr64(
            binary,
            pt_load_offset,
            PT_LOAD,
            PF_R,
            seg_file_offset as u64,
            seg_vaddr,
            seg_vaddr,
            seg_total_size as u64,
            seg_memsz,
            PAGE_SIZE,
        )
        .map_err(|e| Error::BlobError(format!("failed to write PT_LOAD: {e}")))?;

        // Append PT_NOTE entry.
        let pt_note_offset = binary.len();
        binary.resize(binary.len() + PHDR_SIZE, 0);
        write_phdr64(
            binary,
            pt_note_offset,
            PT_NOTE,
            PF_R,
            note_file_off,
            note_vaddr,
            note_vaddr,
            note_size,
            note_size,
            4,
        )
        .map_err(|e| Error::BlobError(format!("failed to write PT_NOTE: {e}")))?;

        // Update ELF header.
        let new_phdr_table_file_off = seg_file_offset + phdr_table_offset_in_seg;
        let new_phdr_table_vaddr = seg_vaddr + phdr_table_offset_in_seg as u64;
        write_u64_le(binary, 0x20, new_phdr_table_file_off as u64);
        write_u16_le(binary, 0x38, new_phnum as u16);

        // Update PT_PHDR entry in the combined table (if present) to point
        // to the new phdr table location. Without this, the dynamic linker
        // would use the old PT_PHDR vaddr and miss our new entries.
        let combined_table_start = seg_file_offset + phdr_table_offset_in_seg;
        let combined_table_total = (new_phnum * PHDR_SIZE) as u64;
        for i in 0..new_phnum {
            let off = combined_table_start + i * PHDR_SIZE;
            if read_u32_le(binary, off) == 6 {
                // PT_PHDR = 6
                write_u64_le(binary, off + 8, new_phdr_table_file_off as u64); // p_offset
                write_u64_le(binary, off + 16, new_phdr_table_vaddr); // p_vaddr
                write_u64_le(binary, off + 24, new_phdr_table_vaddr); // p_paddr
                write_u64_le(binary, off + 32, combined_table_total); // p_filesz
                write_u64_le(binary, off + 40, combined_table_total); // p_memsz
            }
        }

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

        let namesz = u32::from_le_bytes(note[0..4].try_into().unwrap());
        let descsz = u32::from_le_bytes(note[4..8].try_into().unwrap());
        let ntype = u32::from_le_bytes(note[8..12].try_into().unwrap());

        assert_eq!(namesz, NOTE_NAME.len() as u32);
        assert_eq!(descsz, payload.len() as u32);
        assert_eq!(ntype, NOTE_TYPE);

        let name_padded = align_up(NOTE_NAME.len(), 4);
        assert_eq!(&note[12..12 + NOTE_NAME.len()], NOTE_NAME);

        let desc_start = 12 + name_padded;
        assert_eq!(&note[desc_start..desc_start + payload.len()], payload);
    }

    #[test]
    fn build_elf_note_empty_payload() {
        let note = build_elf_note(NOTE_NAME, b"", NOTE_TYPE);
        let namesz = u32::from_le_bytes(note[0..4].try_into().unwrap());
        let descsz = u32::from_le_bytes(note[4..8].try_into().unwrap());
        assert_eq!(namesz, NOTE_NAME.len() as u32);
        assert_eq!(descsz, 0);
    }

    /// Build a minimal valid ELF64 binary with PT_LOAD + PT_NOTE.
    fn make_minimal_elf64() -> Vec<u8> {
        let phdr_offset: u64 = 64;
        let phdr_count: u16 = 2;
        let phdrs_end = phdr_offset as usize + (phdr_count as usize) * PHDR_SIZE;

        let existing_note = build_elf_note(b"GNU\0", b"\x01\x00\x00\x00", 3);
        let total = phdrs_end + existing_note.len();

        let mut binary = vec![0u8; total];

        binary[0..4].copy_from_slice(b"\x7fELF");
        binary[4] = 2; // ELFCLASS64
        binary[5] = 1; // ELFDATA2LSB
        binary[6] = 1; // EV_CURRENT
        binary[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        binary[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        binary[20..24].copy_from_slice(&1u32.to_le_bytes());
        write_u64_le(&mut binary, 0x20, phdr_offset);
        binary[0x34..0x36].copy_from_slice(&64u16.to_le_bytes());
        binary[0x36..0x38].copy_from_slice(&(PHDR_SIZE as u16).to_le_bytes());
        binary[0x38..0x3A].copy_from_slice(&phdr_count.to_le_bytes());

        // PT_LOAD covering the file, with BSS (memsz > filesz).
        write_phdr64(
            &mut binary,
            phdr_offset as usize,
            PT_LOAD,
            5,
            0,
            0x400000,
            0x400000,
            total as u64,
            (total + 0x1000) as u64, // memsz > filesz (BSS)
            0x1000,
        )
        .unwrap();

        // PT_NOTE for existing note.
        let note_offset = phdrs_end;
        binary[note_offset..note_offset + existing_note.len()].copy_from_slice(&existing_note);
        write_phdr64(
            &mut binary,
            phdr_offset as usize + PHDR_SIZE,
            PT_NOTE,
            4,
            note_offset as u64,
            0x400000 + note_offset as u64,
            0x400000 + note_offset as u64,
            existing_note.len() as u64,
            existing_note.len() as u64,
            4,
        )
        .unwrap();

        binary
    }

    #[test]
    fn inject_creates_new_segment_above_existing() {
        let mut binary = make_minimal_elf64();
        let blob = b"test-sea-blob";

        let injector = ElfInjector;
        injector.inject(&mut binary, blob).unwrap();

        // e_phnum should be 4 (original 2 + PT_LOAD + PT_NOTE).
        let e_phnum = read_u16_le(&binary, 0x38);
        assert_eq!(e_phnum, 4);

        let e_phoff = read_u64_le(&binary, 0x20) as usize;

        // Find our new entries in the combined table.
        let mut new_load_vaddr = None;
        let mut new_note_vaddr = None;

        for i in 0..4 {
            let off = e_phoff + i * PHDR_SIZE;
            let p_type = read_u32_le(&binary, off);
            let p_vaddr = read_u64_le(&binary, off + 16);

            if p_type == PT_LOAD && p_vaddr > 0x400000 {
                new_load_vaddr = Some(p_vaddr);
                assert_eq!(p_vaddr % PAGE_SIZE, 0);
                let original_end = 0x400000u64 + make_minimal_elf64().len() as u64 + 0x1000;
                assert!(p_vaddr >= align_up_u64(original_end, PAGE_SIZE));
            }
            if p_type == PT_NOTE {
                let p_filesz = read_u64_le(&binary, off + 32) as usize;
                if p_filesz == build_elf_note(NOTE_NAME, blob, NOTE_TYPE).len() {
                    new_note_vaddr = Some(p_vaddr);
                }
            }
        }

        let load_vaddr = new_load_vaddr.expect("new PT_LOAD not found");
        let note_vaddr = new_note_vaddr.expect("new PT_NOTE not found");
        // Note should be at the start of the new segment.
        assert_eq!(note_vaddr, load_vaddr);

        // Verify the note content via the new PT_NOTE's file offset.
        let expected_note = build_elf_note(NOTE_NAME, blob, NOTE_TYPE);
        for i in 0..4 {
            let off = e_phoff + i * PHDR_SIZE;
            if read_u32_le(&binary, off) == PT_NOTE {
                let p_offset = read_u64_le(&binary, off + 8) as usize;
                let p_filesz = read_u64_le(&binary, off + 32) as usize;
                if p_filesz == expected_note.len() {
                    assert_eq!(
                        &binary[p_offset..p_offset + expected_note.len()],
                        &expected_note[..]
                    );
                }
            }
        }
    }

    #[test]
    fn inject_does_not_modify_existing_phdrs() {
        let original = make_minimal_elf64();
        let mut binary = original.clone();
        let blob = b"preserve-test";

        let injector = ElfInjector;
        injector.inject(&mut binary, blob).unwrap();

        // The original file content should be unchanged (except ELF header fields).
        // Check that bytes 0x40 onwards (after ELF header) are preserved.
        assert_eq!(
            &binary[0x40..original.len()],
            &original[0x40..original.len()]
        );
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

    #[test]
    fn file_offset_alignment() {
        let mut binary = make_minimal_elf64();
        // Add some extra bytes to make file size non-page-aligned.
        binary.extend_from_slice(&[0x42; 37]);
        let blob = b"alignment-test";

        let injector = ElfInjector;
        injector.inject(&mut binary, blob).unwrap();

        // The new PT_LOAD's file offset should be page-aligned.
        let e_phoff = read_u64_le(&binary, 0x20) as usize;
        let e_phnum = read_u16_le(&binary, 0x38) as usize;
        for i in 0..e_phnum {
            let off = e_phoff + i * PHDR_SIZE;
            if read_u32_le(&binary, off) == PT_LOAD {
                let p_vaddr = read_u64_le(&binary, off + 16);
                if p_vaddr > 0x400000 {
                    let p_offset = read_u64_le(&binary, off + 8);
                    assert_eq!(p_offset % PAGE_SIZE, 0, "p_offset must be page-aligned");
                    assert_eq!(p_vaddr % PAGE_SIZE, 0, "p_vaddr must be page-aligned");
                }
            }
        }
    }
}
