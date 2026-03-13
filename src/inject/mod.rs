//! Binary injection engine.
//!
//! Handles injecting the SEA blob into platform-specific executable formats.
//! Detects the binary format (ELF, Mach-O, PE) and dispatches to the
//! appropriate injector implementation.

pub mod elf;
pub mod macho;
pub mod pe;

use crate::error::{Error, Result};

/// Supported binary executable formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormat {
    /// ELF — used on Linux and other Unix-like systems.
    Elf,
    /// Mach-O — used on macOS and iOS.
    MachO,
    /// PE — used on Windows.
    Pe,
}

/// Detects the binary format of the given bytes by parsing with goblin.
pub fn detect_format(binary: &[u8]) -> Result<BinaryFormat> {
    match goblin::Object::parse(binary) {
        Ok(goblin::Object::Elf(_)) => Ok(BinaryFormat::Elf),
        Ok(goblin::Object::Mach(_)) => Ok(BinaryFormat::MachO),
        Ok(goblin::Object::PE(_)) => Ok(BinaryFormat::Pe),
        Ok(_) => Err(Error::UnsupportedFormat(
            "unknown or unsupported binary format".into(),
        )),
        Err(e) => Err(Error::GoblinError(e.to_string())),
    }
}

/// Trait for format-specific SEA blob injection.
pub trait Injector {
    /// Inject the SEA `blob` into `binary`, modifying it in place.
    fn inject(&self, binary: &mut Vec<u8>, blob: &[u8]) -> Result<()>;
}

/// Detect the binary format and inject the SEA blob using the appropriate injector.
pub fn inject(binary: &mut Vec<u8>, blob: &[u8]) -> Result<()> {
    let format = detect_format(binary)?;
    let injector: Box<dyn Injector> = match format {
        BinaryFormat::Elf => Box::new(elf::ElfInjector),
        BinaryFormat::MachO => Box::new(macho::MachoInjector),
        BinaryFormat::Pe => Box::new(pe::PeInjector),
    };
    injector.inject(binary, blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn detect_format_macho_from_system_binary() {
        let binary = std::fs::read("/usr/bin/true").expect("failed to read /usr/bin/true");
        let format = detect_format(&binary).expect("failed to detect format");
        assert_eq!(format, BinaryFormat::MachO);
    }

    #[test]
    fn stub_elf_injector_returns_error() {
        let injector = elf::ElfInjector;
        let mut binary = Vec::new();
        let blob = b"test";
        let err = injector.inject(&mut binary, blob).unwrap_err();
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[test]
    fn stub_macho_injector_returns_error() {
        let injector = macho::MachoInjector;
        let mut binary = Vec::new();
        let blob = b"test";
        let err = injector.inject(&mut binary, blob).unwrap_err();
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[test]
    fn stub_pe_injector_returns_error() {
        let injector = pe::PeInjector;
        let mut binary = Vec::new();
        let blob = b"test";
        let err = injector.inject(&mut binary, blob).unwrap_err();
        assert!(err.to_string().contains("not yet implemented"));
    }
}
