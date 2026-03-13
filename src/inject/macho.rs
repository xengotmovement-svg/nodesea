//! Mach-O SEA blob injection (macOS).

use crate::error::{Error, Result};
use crate::inject::Injector;

/// Injector for the Mach-O binary format.
pub struct MachoInjector;

impl Injector for MachoInjector {
    fn inject(&self, _binary: &mut Vec<u8>, _blob: &[u8]) -> Result<()> {
        Err(Error::UnsupportedFormat(
            "Mach-O injection not yet implemented".into(),
        ))
    }
}
