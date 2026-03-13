//! ELF note-based SEA blob injection (Linux).

use crate::error::{Error, Result};
use crate::inject::Injector;

/// Injector for the ELF binary format.
pub struct ElfInjector;

impl Injector for ElfInjector {
    fn inject(&self, _binary: &mut Vec<u8>, _blob: &[u8]) -> Result<()> {
        Err(Error::UnsupportedFormat(
            "ELF injection not yet implemented".into(),
        ))
    }
}
