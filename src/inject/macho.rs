//! Mach-O SEA blob injection (macOS).
//!
//! Injects a SEA blob into a Mach-O binary by:
//! 1. Removing `LC_CODE_SIGNATURE` load command and truncating signature data
//! 2. Shrinking `__LINKEDIT` to exclude the removed signature
//! 3. Appending the blob (page-aligned) after the trimmed file
//! 4. Appending the `__LINKEDIT` data after the blob at a new offset
//! 5. Writing a new `LC_SEGMENT_64` + `Section64` in the header padding
//!
//! This ensures `__LINKEDIT` remains the last segment in the file, which
//! is required for `codesign` to succeed.

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

/// Read a u32 LE from binary at the given offset.
fn read_u32(binary: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([binary[off], binary[off + 1], binary[off + 2], binary[off + 3]])
}

/// Read a u64 LE from binary at the given offset.
fn read_u64(binary: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        binary[off],
        binary[off + 1],
        binary[off + 2],
        binary[off + 3],
        binary[off + 4],
        binary[off + 5],
        binary[off + 6],
        binary[off + 7],
    ])
}

fn pwrite_u32(binary: &mut [u8], value: u32, off: usize) -> Result<()> {
    binary
        .pwrite_with(value, off, scroll::LE)
        .map_err(|e| Error::GoblinError(e.to_string()))?;
    Ok(())
}

fn pwrite_u64(binary: &mut [u8], value: u64, off: usize) -> Result<()> {
    binary
        .pwrite_with(value, off, scroll::LE)
        .map_err(|e| Error::GoblinError(e.to_string()))?;
    Ok(())
}

/// Info about the __LINKEDIT segment.
struct LinkeditInfo {
    /// Offset of the load command in the file.
    cmd_offset: usize,
    /// fileoff field value.
    fileoff: u64,
    /// filesize field value.
    filesize: u64,
}

/// Find the __LINKEDIT segment load command.
fn find_linkedit(binary: &[u8], ncmds: u32) -> Option<LinkeditInfo> {
    let mut off = MACH_HEADER_64_SIZE;
    for _ in 0..ncmds {
        if off + 8 > binary.len() {
            break;
        }
        let cmd = read_u32(binary, off);
        let cmdsize = read_u32(binary, off + 4) as usize;

        if cmd == LC_SEGMENT_64 && cmdsize >= SEGMENT_COMMAND_64_SIZE {
            let segname = &binary[off + 8..off + 8 + 16];
            if segname.starts_with(b"__LINKEDIT\0") {
                return Some(LinkeditInfo {
                    cmd_offset: off,
                    fileoff: read_u64(binary, off + 40),
                    filesize: read_u64(binary, off + 48),
                });
            }
        }
        off += cmdsize;
    }
    None
}

impl Injector for MachoInjector {
    fn inject(&self, binary: &mut Vec<u8>, blob: &[u8]) -> Result<()> {
        // Step 1: Validate it's a single-arch Mach-O
        let mach = Mach::parse(binary).map_err(|e| Error::GoblinError(e.to_string()))?;
        match &mach {
            Mach::Binary(_) => {}
            Mach::Fat(_) => {
                return Err(Error::UnsupportedFormat(
                    "fat/universal Mach-O binaries are not yet supported".into(),
                ));
            }
        }

        let cputype = read_u32(binary, CPUTYPE_OFFSET);
        let mut ncmds = read_u32(binary, NCMDS_OFFSET);
        let mut sizeofcmds = read_u32(binary, SIZEOFCMDS_OFFSET);

        let page_size: usize = if cputype == CPU_TYPE_ARM64 {
            0x4000
        } else {
            0x1000
        };

        // Step 2: Remove LC_CODE_SIGNATURE and note its data location
        let mut codesig_dataoff: Option<u64> = None;
        {
            let mut cmd_offset = MACH_HEADER_64_SIZE;
            let cmds_end = MACH_HEADER_64_SIZE + sizeofcmds as usize;
            let mut i = 0u32;
            while i < ncmds {
                if cmd_offset + 8 > binary.len() {
                    break;
                }
                let cmd = read_u32(binary, cmd_offset);
                let cmdsize = read_u32(binary, cmd_offset + 4);

                if cmd == LC_CODE_SIGNATURE {
                    // LC_CODE_SIGNATURE has: cmd(4) + cmdsize(4) + dataoff(4) + datasize(4)
                    if cmdsize as usize >= 16 {
                        codesig_dataoff = Some(read_u32(binary, cmd_offset + 8) as u64);
                    }

                    let cs_size = cmdsize as usize;
                    // Shift subsequent load commands down to fill the gap
                    let after = cmd_offset + cs_size;
                    if after < cmds_end {
                        binary.copy_within(after..cmds_end, cmd_offset);
                    }
                    // Zero out freed space
                    let new_end = cmds_end - cs_size;
                    for b in &mut binary[new_end..cmds_end] {
                        *b = 0;
                    }

                    ncmds -= 1;
                    sizeofcmds -= cmdsize;
                    pwrite_u32(binary, ncmds, NCMDS_OFFSET)?;
                    pwrite_u32(binary, sizeofcmds, SIZEOFCMDS_OFFSET)?;
                    continue;
                }

                cmd_offset += cmdsize as usize;
                i += 1;
            }
        }

        // Step 3: Truncate the file to remove the code signature DATA,
        // and shrink __LINKEDIT accordingly.
        if let Some(dataoff) = codesig_dataoff {
            // Truncate the file at the code signature data offset
            binary.truncate(dataoff as usize);

            // Update __LINKEDIT's filesize
            if let Some(linkedit) = find_linkedit(binary, ncmds) {
                let new_filesize = dataoff - linkedit.fileoff;
                // filesize is at cmd_offset + 48
                pwrite_u64(binary, new_filesize, linkedit.cmd_offset + 48)?;
                // Also update vmsize to match (page-aligned)
                let new_vmsize = align_up(new_filesize as usize, page_size) as u64;
                pwrite_u64(binary, new_vmsize, linkedit.cmd_offset + 32)?;
            }
        }

        // Step 4: Now the file ends at __LINKEDIT data. We need to:
        // a) Save the __LINKEDIT data
        // b) Truncate the file to just before __LINKEDIT
        // c) Append our blob (page-aligned)
        // d) Re-append __LINKEDIT data (page-aligned)
        // e) Update __LINKEDIT's fileoff
        //
        // This ensures __LINKEDIT is the LAST segment data in the file.
        let linkedit = find_linkedit(binary, ncmds).ok_or_else(|| {
            Error::GoblinError("__LINKEDIT segment not found".into())
        })?;

        let linkedit_cmd_offset = linkedit.cmd_offset;
        let old_linkedit_fileoff = linkedit.fileoff as usize;
        let old_linkedit_filesize = linkedit.filesize as usize;

        // Save __LINKEDIT data
        let linkedit_data = binary[old_linkedit_fileoff..old_linkedit_fileoff + old_linkedit_filesize].to_vec();

        // Truncate to just before __LINKEDIT
        binary.truncate(old_linkedit_fileoff);

        // Page-align, then append our blob
        let blob_file_offset = align_up(binary.len(), page_size);
        binary.resize(blob_file_offset, 0);
        binary.extend_from_slice(blob);

        // Page-align, then re-append __LINKEDIT data
        let new_linkedit_fileoff = align_up(binary.len(), page_size);
        binary.resize(new_linkedit_fileoff, 0);
        binary.extend_from_slice(&linkedit_data);

        // No final page padding — __LINKEDIT data extends to EOF.
        // codesign will append its own signature after __LINKEDIT.

        // Update __LINKEDIT fileoff and filesize.
        // After the shift below, __LINKEDIT's load command will be at
        // linkedit_cmd_offset + LC_ENTRY_SIZE, but we update it now at
        // linkedit_cmd_offset since the shift hasn't happened yet.
        let new_linkedit_filesize = linkedit_data.len() as u64;
        pwrite_u64(binary, new_linkedit_fileoff as u64, linkedit_cmd_offset + 40)?;
        pwrite_u64(binary, new_linkedit_filesize, linkedit_cmd_offset + 48)?;
        let new_linkedit_vmsize = align_up(linkedit_data.len(), page_size) as u64;
        pwrite_u64(binary, new_linkedit_vmsize, linkedit_cmd_offset + 32)?;

        // We also need to update __LINKEDIT's vmaddr since we moved it
        // Find max vm_end from all segments to place __LINKEDIT + our segment
        let mut max_vm_end: u64 = 0;
        {
            let mut off = MACH_HEADER_64_SIZE;
            for _ in 0..ncmds {
                if off + 8 > binary.len() {
                    break;
                }
                let cmd = read_u32(binary, off);
                let cmdsize = read_u32(binary, off + 4) as usize;
                if cmd == LC_SEGMENT_64 && cmdsize >= SEGMENT_COMMAND_64_SIZE {
                    let segname = &binary[off + 8..off + 8 + 16];
                    // Skip __LINKEDIT since we're recalculating its vmaddr
                    if !segname.starts_with(b"__LINKEDIT\0") {
                        let vmaddr = read_u64(binary, off + 24);
                        let vmsize = read_u64(binary, off + 32);
                        let vm_end = vmaddr.saturating_add(vmsize);
                        if vm_end > max_vm_end {
                            max_vm_end = vm_end;
                        }
                    }
                }
                off += cmdsize;
            }
        }

        // Our NODE_SEA segment goes at max_vm_end (page-aligned)
        let seg_vmaddr = align_up(max_vm_end as usize, page_size) as u64;
        let seg_vmsize = align_up(blob.len(), page_size) as u64;

        // __LINKEDIT vmaddr goes after our segment
        let new_linkedit_vmaddr = seg_vmaddr + seg_vmsize;
        pwrite_u64(binary, new_linkedit_vmaddr, linkedit_cmd_offset + 24)?;

        // Step 5: Check header space for our new load command.
        // We must INSERT the NODE_SEA load command BEFORE __LINKEDIT,
        // because codesign requires __LINKEDIT to be the last LC_SEGMENT_64.
        let cmds_end = MACH_HEADER_64_SIZE + sizeofcmds as usize;

        // Find the earliest section offset to know where actual data begins.
        // Load commands must not extend past this point.
        let mut first_data_offset: usize = binary.len();
        {
            let mut off = MACH_HEADER_64_SIZE;
            for _ in 0..ncmds {
                if off + 8 > binary.len() {
                    break;
                }
                let cmd = read_u32(binary, off);
                let cmdsize = read_u32(binary, off + 4) as usize;
                if cmd == LC_SEGMENT_64 && cmdsize >= SEGMENT_COMMAND_64_SIZE {
                    let nsects = read_u32(binary, off + 64) as usize;
                    let mut sect_off = off + SEGMENT_COMMAND_64_SIZE;
                    for _ in 0..nsects {
                        if sect_off + SECTION_64_SIZE > binary.len() {
                            break;
                        }
                        // Section64.offset is at byte 48 within the section
                        let sect_file_offset = read_u32(binary, sect_off + 48) as usize;
                        if sect_file_offset > 0 && sect_file_offset < first_data_offset {
                            first_data_offset = sect_file_offset;
                        }
                        sect_off += SECTION_64_SIZE;
                    }
                    // Also check segment fileoff for segments without sections
                    if nsects == 0 {
                        let fileoff = read_u64(binary, off + 40) as usize;
                        if fileoff > 0 && fileoff < first_data_offset {
                            first_data_offset = fileoff;
                        }
                    }
                }
                off += cmdsize;
            }
        }

        let available_space = first_data_offset.saturating_sub(cmds_end);
        if available_space < LC_ENTRY_SIZE {
            return Err(Error::InsufficientHeaderSpace {
                need: LC_ENTRY_SIZE,
                have: available_space,
            });
        }

        // Step 6: Insert LC_SEGMENT_64 + Section64 for NODE_SEA before __LINKEDIT.
        // Shift __LINKEDIT and all subsequent load commands forward by LC_ENTRY_SIZE.
        {
            let shift_start = linkedit_cmd_offset;
            let shift_end = cmds_end;
            // Make room by shifting forward
            binary.copy_within(shift_start..shift_end, shift_start + LC_ENTRY_SIZE);
            // Zero the space we just created (for safety)
            for b in &mut binary[shift_start..shift_start + LC_ENTRY_SIZE] {
                *b = 0;
            }
        }

        let mut off = linkedit_cmd_offset;
        let segname = padded_name_16("NODE_SEA");

        // segment_command_64 (72 bytes)
        pwrite_u32(binary, LC_SEGMENT_64, off)?; off += 4;          // cmd
        pwrite_u32(binary, LC_ENTRY_SIZE as u32, off)?; off += 4;   // cmdsize
        binary[off..off + 16].copy_from_slice(&segname); off += 16; // segname
        pwrite_u64(binary, seg_vmaddr, off)?; off += 8;             // vmaddr
        pwrite_u64(binary, seg_vmsize, off)?; off += 8;             // vmsize
        pwrite_u64(binary, blob_file_offset as u64, off)?; off += 8; // fileoff
        let seg_filesize = align_up(blob.len(), page_size) as u64;
        pwrite_u64(binary, seg_filesize, off)?; off += 8;           // filesize
        pwrite_u32(binary, 1, off)?; off += 4;                      // maxprot (VM_PROT_READ)
        pwrite_u32(binary, 1, off)?; off += 4;                      // initprot (VM_PROT_READ)
        pwrite_u32(binary, 1, off)?; off += 4;                      // nsects
        pwrite_u32(binary, 0, off)?; off += 4;                      // flags

        // section_64 (80 bytes)
        let sectname = padded_name_16("__NODE_SEA_BLOB");
        binary[off..off + 16].copy_from_slice(&sectname); off += 16; // sectname
        binary[off..off + 16].copy_from_slice(&segname); off += 16;  // segname
        pwrite_u64(binary, seg_vmaddr, off)?; off += 8;              // addr
        pwrite_u64(binary, blob.len() as u64, off)?; off += 8;       // size
        pwrite_u32(binary, blob_file_offset as u32, off)?; off += 4;  // offset
        pwrite_u32(binary, 0, off)?; off += 4;                        // align
        pwrite_u32(binary, 0, off)?; off += 4;                        // reloff
        pwrite_u32(binary, 0, off)?; off += 4;                        // nreloc
        pwrite_u32(binary, 0, off)?; off += 4;                        // flags
        pwrite_u32(binary, 0, off)?; off += 4;                        // reserved1
        pwrite_u32(binary, 0, off)?; off += 4;                        // reserved2
        pwrite_u32(binary, 0, off)?;                                   // reserved3

        // Step 7: Update mach_header_64
        ncmds += 1;
        sizeofcmds += LC_ENTRY_SIZE as u32;
        pwrite_u32(binary, ncmds, NCMDS_OFFSET)?;
        pwrite_u32(binary, sizeofcmds, SIZEOFCMDS_OFFSET)?;

        // Step 8: We also need to fix up any load commands that reference
        // offsets into __LINKEDIT data (LC_DYLD_INFO, LC_SYMTAB, etc.)
        // by adding the delta between old and new __LINKEDIT fileoff.
        //
        // Actually, since we moved __LINKEDIT data wholesale, all the internal
        // offsets (symoff, stroff, etc.) are relative to the file start and need
        // to be adjusted by the delta.
        let linkedit_delta = new_linkedit_fileoff as i64 - old_linkedit_fileoff as i64;
        if linkedit_delta != 0 {
            fixup_linkedit_references(binary, ncmds, old_linkedit_fileoff, linkedit_delta)?;
        }

        Ok(())
    }
}

/// Fix up load commands that reference offsets within __LINKEDIT.
/// These offsets are absolute file offsets, and since we moved __LINKEDIT,
/// they all need to be shifted by `delta`.
fn fixup_linkedit_references(
    binary: &mut [u8],
    ncmds: u32,
    old_linkedit_fileoff: usize,
    delta: i64,
) -> Result<()> {
    let mut off = MACH_HEADER_64_SIZE;
    for _ in 0..ncmds {
        if off + 8 > binary.len() {
            break;
        }
        let cmd = read_u32(binary, off);
        let cmdsize = read_u32(binary, off + 4) as usize;

        match cmd {
            // LC_SYMTAB = 0x2
            0x2 if cmdsize >= 24 => {
                fixup_u32_offset(binary, off + 8, old_linkedit_fileoff, delta)?; // symoff
                fixup_u32_offset(binary, off + 16, old_linkedit_fileoff, delta)?; // stroff
            }
            // LC_DYSYMTAB = 0xB
            0xB if cmdsize >= 80 => {
                // tocoff, modtaboff, extrefsymoff, indirectsymoff, extreloff, locreloff
                for field_off in [32, 40, 48, 56, 64, 72] {
                    fixup_u32_offset(binary, off + field_off, old_linkedit_fileoff, delta)?;
                }
            }
            // LC_DYLD_INFO / LC_DYLD_INFO_ONLY = 0x22 / 0x80000022
            0x22 | 0x8000_0022 if cmdsize >= 48 => {
                // rebase_off, bind_off, weak_bind_off, lazy_bind_off, export_off
                for field_off in [8, 16, 24, 32, 40] {
                    fixup_u32_offset(binary, off + field_off, old_linkedit_fileoff, delta)?;
                }
            }
            // LC_FUNCTION_STARTS = 0x26, LC_DATA_IN_CODE = 0x29,
            // LC_DYLD_EXPORTS_TRIE = 0x80000033, LC_DYLD_CHAINED_FIXUPS = 0x80000034
            0x26 | 0x29 | 0x8000_0033 | 0x8000_0034 if cmdsize >= 16 => {
                fixup_u32_offset(binary, off + 8, old_linkedit_fileoff, delta)?; // dataoff
            }
            _ => {}
        }

        off += cmdsize;
    }
    Ok(())
}

/// Adjust a u32 file offset field if it points into the old __LINKEDIT region.
fn fixup_u32_offset(
    binary: &mut [u8],
    field_off: usize,
    old_linkedit_start: usize,
    delta: i64,
) -> Result<()> {
    let val = read_u32(binary, field_off) as usize;
    if val >= old_linkedit_start && val > 0 {
        let new_val = (val as i64 + delta) as u32;
        pwrite_u32(binary, new_val, field_off)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Find a Mach-O binary with enough header space for injection.
    /// Prefers the Node.js binary (which has ample space), falls back to
    /// `which node`, then /usr/bin/true. Returns None if no suitable
    /// single-arch binary is available.
    #[cfg(target_os = "macos")]
    fn find_test_binary() -> Option<Vec<u8>> {
        let candidates = [
            "/tmp/node-v22.16.0-darwin-arm64/bin/node",
        ];

        // Also try `which node`
        let which_node = which::which("node").ok();
        let all_paths: Vec<&std::path::Path> = candidates
            .iter()
            .map(std::path::Path::new)
            .chain(which_node.as_deref())
            .collect();

        for path in all_paths {
            if let Ok(data) = std::fs::read(path) {
                let binary = match Mach::parse(&data) {
                    Ok(Mach::Binary(_)) => data,
                    Ok(Mach::Fat(fat)) => {
                        if let Some(arch) = fat.iter_arches().flatten().next() {
                            let s = arch.offset as usize;
                            data[s..s + arch.size as usize].to_vec()
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                return Some(binary);
            }
        }
        None
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn inject_blob_into_macho_binary() {
        let mut binary = match find_test_binary() {
            Some(b) => b,
            None => {
                eprintln!("skipping: no suitable Mach-O binary found");
                return;
            }
        };

        let blob = b"test_sea_blob_data";

        MachoInjector
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
                    let sect_name = section.name().expect("section name");
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

        let offset = section_offset.expect("section offset");
        let size = section_size.expect("section size");
        assert_eq!(size, blob.len(), "section size mismatch");
        assert_eq!(&binary[offset..offset + size], blob, "blob data mismatch");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn injected_binary_can_be_codesigned() {
        let mut binary = match find_test_binary() {
            Some(b) => b,
            None => {
                eprintln!("skipping: no suitable Mach-O binary found");
                return;
            }
        };

        let blob = b"test_sea_blob_data_for_codesign";
        MachoInjector.inject(&mut binary, blob).expect("injection failed");

        // Write to a temp file and try to codesign it
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_binary");
        std::fs::write(&path, &binary).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let output = std::process::Command::new("codesign")
            .args(["--sign", "-", "--force"])
            .arg(&path)
            .output()
            .expect("failed to run codesign");

        assert!(
            output.status.success(),
            "codesign failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn insufficient_header_space_detected() {
        // /usr/bin/true is a small binary with minimal header space
        let original = std::fs::read("/usr/bin/true").expect("failed to read /usr/bin/true");
        let mut binary = match Mach::parse(&original).expect("failed to parse") {
            Mach::Binary(_) => original.clone(),
            Mach::Fat(fat) => {
                let arch = fat.iter_arches().flatten().next().unwrap();
                let s = arch.offset as usize;
                original[s..s + arch.size as usize].to_vec()
            }
        };

        let blob = b"test_blob";
        let result = MachoInjector.inject(&mut binary, blob);
        assert!(
            matches!(result, Err(crate::error::Error::InsufficientHeaderSpace { .. })),
            "expected InsufficientHeaderSpace, got: {result:?}"
        );
    }
}
