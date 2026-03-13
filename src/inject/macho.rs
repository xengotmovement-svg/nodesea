//! Mach-O SEA blob injection (macOS).
//!
//! Injects a SEA blob into a Mach-O binary by:
//! 1. Removing any existing `LC_CODE_SIGNATURE` load command
//! 2. Appending the blob to the file (page-aligned)
//! 3. Writing a new `LC_SEGMENT_64` + `Section64` in the header padding

use crate::error::{Error, Result};
use crate::inject::Injector;
use goblin::mach::Mach;
use scroll::Pwrite;

// Mach-O constants
const LC_SEGMENT_64: u32 = 0x19;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;

const MACH_HEADER_64_SIZE: usize = 32;
const SEGMENT_COMMAND_64_SIZE: usize = 72;
const SECTION_64_SIZE: usize = 80;
const LC_ENTRY_SIZE: usize = SEGMENT_COMMAND_64_SIZE + SECTION_64_SIZE; // 152

// Header field offsets within mach_header_64
const NCMDS_OFFSET: usize = 16;
const SIZEOFCMDS_OFFSET: usize = 20;
// cputype is at offset 4
const CPUTYPE_OFFSET: usize = 4;

/// Injector for the Mach-O binary format.
pub struct MachoInjector;

/// Make a 16-byte padded name from a string.
fn padded_name_16(name: &str) -> [u8; 16] {
    let mut buf = [0u8; 16];
    let bytes = name.as_bytes();
    let len = bytes.len().min(16);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf
}

/// Round `value` up to the next multiple of `align`.
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

impl Injector for MachoInjector {
    fn inject(&self, binary: &mut Vec<u8>, blob: &[u8]) -> Result<()> {
        // Step 1: Parse to validate it's a Mach-O and get metadata
        let mach = Mach::parse(binary).map_err(|e| Error::GoblinError(e.to_string()))?;
        match &mach {
            Mach::Binary(_) => {} // single-arch, proceed
            Mach::Fat(_) => {
                return Err(Error::UnsupportedFormat(
                    "fat/universal Mach-O binaries are not yet supported".into(),
                ));
            }
        }

        // Read header fields (all little-endian for x86_64 and arm64)
        let cputype = u32::from_le_bytes([
            binary[CPUTYPE_OFFSET],
            binary[CPUTYPE_OFFSET + 1],
            binary[CPUTYPE_OFFSET + 2],
            binary[CPUTYPE_OFFSET + 3],
        ]);
        let ncmds = u32::from_le_bytes([
            binary[NCMDS_OFFSET],
            binary[NCMDS_OFFSET + 1],
            binary[NCMDS_OFFSET + 2],
            binary[NCMDS_OFFSET + 3],
        ]);
        let sizeofcmds = u32::from_le_bytes([
            binary[SIZEOFCMDS_OFFSET],
            binary[SIZEOFCMDS_OFFSET + 1],
            binary[SIZEOFCMDS_OFFSET + 2],
            binary[SIZEOFCMDS_OFFSET + 3],
        ]);

        let page_size: usize = if cputype == CPU_TYPE_ARM64 {
            0x4000
        } else {
            0x1000
        };

        let mut ncmds = ncmds;
        let mut sizeofcmds = sizeofcmds;

        // Step 2: Remove LC_CODE_SIGNATURE if present
        // Walk load commands manually
        let mut cmd_offset = MACH_HEADER_64_SIZE;
        let cmds_end = MACH_HEADER_64_SIZE + sizeofcmds as usize;
        let mut i = 0u32;
        while i < ncmds {
            if cmd_offset + 8 > binary.len() {
                break;
            }
            let cmd = u32::from_le_bytes([
                binary[cmd_offset],
                binary[cmd_offset + 1],
                binary[cmd_offset + 2],
                binary[cmd_offset + 3],
            ]);
            let cmdsize = u32::from_le_bytes([
                binary[cmd_offset + 4],
                binary[cmd_offset + 5],
                binary[cmd_offset + 6],
                binary[cmd_offset + 7],
            ]);

            if cmd == LC_CODE_SIGNATURE {
                let cs_size = cmdsize as usize;
                // Zero out this load command
                for b in &mut binary[cmd_offset..cmd_offset + cs_size] {
                    *b = 0;
                }
                // Shift subsequent load commands down to fill the gap
                let after = cmd_offset + cs_size;
                if after < cmds_end {
                    binary.copy_within(after..cmds_end, cmd_offset);
                    // Zero out the freed space at the end
                    let new_end = cmds_end - cs_size;
                    for b in &mut binary[new_end..cmds_end] {
                        *b = 0;
                    }
                }

                ncmds -= 1;
                sizeofcmds -= cmdsize;

                // Update header
                binary
                    .pwrite_with(ncmds, NCMDS_OFFSET, scroll::LE)
                    .map_err(|e| Error::GoblinError(e.to_string()))?;
                binary
                    .pwrite_with(sizeofcmds, SIZEOFCMDS_OFFSET, scroll::LE)
                    .map_err(|e| Error::GoblinError(e.to_string()))?;

                // Don't advance cmd_offset since we shifted commands down
                // Don't increment i since ncmds decreased
                continue;
            }

            cmd_offset += cmdsize as usize;
            i += 1;
        }

        // Step 3: Check header space for our new load command
        let new_cmd_offset = MACH_HEADER_64_SIZE + sizeofcmds as usize;
        let _header_end = new_cmd_offset + LC_ENTRY_SIZE;

        // Find the first segment's fileoff to know how much header space we have
        let mut first_segment_fileoff: usize = binary.len(); // fallback
        {
            let mut off = MACH_HEADER_64_SIZE;
            for _ in 0..ncmds {
                if off + 8 > binary.len() {
                    break;
                }
                let cmd = u32::from_le_bytes([
                    binary[off],
                    binary[off + 1],
                    binary[off + 2],
                    binary[off + 3],
                ]);
                let cmdsize = u32::from_le_bytes([
                    binary[off + 4],
                    binary[off + 5],
                    binary[off + 6],
                    binary[off + 7],
                ]);

                if cmd == LC_SEGMENT_64 && cmdsize as usize >= SEGMENT_COMMAND_64_SIZE {
                    // fileoff is at offset 40 within segment_command_64
                    let fileoff_off = off + 40;
                    let fileoff = u64::from_le_bytes([
                        binary[fileoff_off],
                        binary[fileoff_off + 1],
                        binary[fileoff_off + 2],
                        binary[fileoff_off + 3],
                        binary[fileoff_off + 4],
                        binary[fileoff_off + 5],
                        binary[fileoff_off + 6],
                        binary[fileoff_off + 7],
                    ]) as usize;
                    // Only consider non-zero fileoff (the __PAGEZERO segment has fileoff=0)
                    if fileoff > 0 && fileoff < first_segment_fileoff {
                        first_segment_fileoff = fileoff;
                    }
                }
                off += cmdsize as usize;
            }
        }

        let available_space = if first_segment_fileoff > new_cmd_offset {
            first_segment_fileoff - new_cmd_offset
        } else {
            0
        };

        if available_space < LC_ENTRY_SIZE {
            return Err(Error::InsufficientHeaderSpace {
                need: LC_ENTRY_SIZE,
                have: available_space,
            });
        }

        // Step 4: Page-align file, append blob, pad to next page
        let file_size_aligned = align_up(binary.len(), page_size);
        binary.resize(file_size_aligned, 0);
        let blob_offset = binary.len();
        binary.extend_from_slice(blob);
        let final_size = align_up(binary.len(), page_size);
        binary.resize(final_size, 0);

        // Step 5: Find a suitable vmaddr for our segment
        // Scan existing segments to find the highest vmaddr + vmsize
        let mut max_vm_end: u64 = 0;
        {
            let mut off = MACH_HEADER_64_SIZE;
            for _ in 0..ncmds {
                if off + 8 > binary.len() {
                    break;
                }
                let cmd = u32::from_le_bytes([
                    binary[off],
                    binary[off + 1],
                    binary[off + 2],
                    binary[off + 3],
                ]);
                let cmdsize = u32::from_le_bytes([
                    binary[off + 4],
                    binary[off + 5],
                    binary[off + 6],
                    binary[off + 7],
                ]);

                if cmd == LC_SEGMENT_64 && cmdsize as usize >= SEGMENT_COMMAND_64_SIZE {
                    let vmaddr_off = off + 24;
                    let vmaddr = u64::from_le_bytes([
                        binary[vmaddr_off],
                        binary[vmaddr_off + 1],
                        binary[vmaddr_off + 2],
                        binary[vmaddr_off + 3],
                        binary[vmaddr_off + 4],
                        binary[vmaddr_off + 5],
                        binary[vmaddr_off + 6],
                        binary[vmaddr_off + 7],
                    ]);
                    let vmsize_off = off + 32;
                    let vmsize = u64::from_le_bytes([
                        binary[vmsize_off],
                        binary[vmsize_off + 1],
                        binary[vmsize_off + 2],
                        binary[vmsize_off + 3],
                        binary[vmsize_off + 4],
                        binary[vmsize_off + 5],
                        binary[vmsize_off + 6],
                        binary[vmsize_off + 7],
                    ]);
                    let vm_end = vmaddr.saturating_add(vmsize);
                    if vm_end > max_vm_end {
                        max_vm_end = vm_end;
                    }
                }
                off += cmdsize as usize;
            }
        }
        let seg_vmaddr = align_up(max_vm_end as usize, page_size) as u64;
        let seg_vmsize = align_up(blob.len(), page_size) as u64;
        let seg_fileoff = blob_offset as u64;
        let seg_filesize = (final_size - blob_offset) as u64;

        // Step 6: Write the LC_SEGMENT_64 load command at new_cmd_offset
        let mut off = new_cmd_offset;

        // segment_command_64
        binary
            .pwrite_with(LC_SEGMENT_64, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        binary
            .pwrite_with(LC_ENTRY_SIZE as u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;

        // segname (16 bytes)
        let segname = padded_name_16("NODE_SEA");
        binary[off..off + 16].copy_from_slice(&segname);
        off += 16;

        // vmaddr (u64)
        binary
            .pwrite_with(seg_vmaddr, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // vmsize (u64)
        binary
            .pwrite_with(seg_vmsize, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // fileoff (u64)
        binary
            .pwrite_with(seg_fileoff, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // filesize (u64)
        binary
            .pwrite_with(seg_filesize, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // maxprot (VM_PROT_READ = 1)
        binary
            .pwrite_with(1u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // initprot (VM_PROT_READ = 1)
        binary
            .pwrite_with(1u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // nsects = 1
        binary
            .pwrite_with(1u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // flags = 0
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;

        // section_64 (80 bytes)
        // sectname (16 bytes)
        let sectname = padded_name_16("__NODE_SEA_BLOB");
        binary[off..off + 16].copy_from_slice(&sectname);
        off += 16;
        // segname (16 bytes)
        binary[off..off + 16].copy_from_slice(&segname);
        off += 16;
        // addr (u64) — same as segment vmaddr
        binary
            .pwrite_with(seg_vmaddr, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // size (u64) — actual blob size
        binary
            .pwrite_with(blob.len() as u64, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 8;
        // offset (u32) — file offset of blob data
        binary
            .pwrite_with(blob_offset as u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // align (u32) — log2 of alignment (0 = byte-aligned)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // reloff (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // nreloc (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // flags (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // reserved1 (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // reserved2 (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        off += 4;
        // reserved3 (u32)
        binary
            .pwrite_with(0u32, off, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        // off += 4; // final field, no need to advance

        // Step 7: Update mach_header_64 ncmds and sizeofcmds
        ncmds += 1;
        sizeofcmds += LC_ENTRY_SIZE as u32;
        binary
            .pwrite_with(ncmds, NCMDS_OFFSET, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;
        binary
            .pwrite_with(sizeofcmds, SIZEOFCMDS_OFFSET, scroll::LE)
            .map_err(|e| Error::GoblinError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn inject_blob_into_macho_binary() {
        // Read a real Mach-O binary
        let original = std::fs::read("/usr/bin/true").expect("failed to read /usr/bin/true");

        // On macOS, /usr/bin/true may be a fat binary — extract the native slice
        let mut binary = match Mach::parse(&original).expect("failed to parse Mach-O") {
            Mach::Binary(_) => original.clone(),
            Mach::Fat(fat) => {
                // Extract the first arch that's a 64-bit binary
                let arch = fat.iter_arches().flatten().next().expect("no arches in fat binary");
                let start = arch.offset as usize;
                let end = start + arch.size as usize;
                original[start..end].to_vec()
            }
        };

        let blob = b"test_sea_blob_data";

        // Inject the blob
        let injector = MachoInjector;
        injector
            .inject(&mut binary, blob)
            .expect("injection failed");

        // Re-parse the result with goblin
        let mach = Mach::parse(&binary).expect("failed to parse modified binary");
        let macho = match mach {
            Mach::Binary(m) => m,
            _ => panic!("expected single-arch Mach-O after injection"),
        };

        // Find our NODE_SEA segment
        let mut found_segment = false;
        let mut found_section = false;
        let mut section_offset: Option<usize> = None;
        let mut section_size: Option<usize> = None;

        for seg in &macho.segments {
            let name = seg.name().expect("segment name");
            if name == "NODE_SEA" {
                found_segment = true;
                for (section, _data) in seg.sections().expect("sections") {
                    let sect_name = section
                        .name()
                        .expect("section name");
                    if sect_name == "__NODE_SEA_BLOB" {
                        found_section = true;
                        section_offset = Some(section.offset as usize);
                        section_size = Some(section.size as usize);
                    }
                }
            }
        }

        assert!(found_segment, "NODE_SEA segment not found");
        assert!(found_section, "__NODE_SEA_BLOB section not found");

        // Verify the section data matches the injected blob
        let offset = section_offset.expect("section offset");
        let size = section_size.expect("section size");
        assert_eq!(size, blob.len(), "section size mismatch");
        assert_eq!(
            &binary[offset..offset + size],
            blob,
            "blob data mismatch"
        );
    }
}
