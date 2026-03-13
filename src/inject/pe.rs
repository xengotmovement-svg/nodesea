//! PE resource-based SEA blob injection (Windows).

use crate::error::{Error, Result};
use crate::inject::Injector;

/// Injector for the PE binary format.
pub struct PeInjector;

impl Injector for PeInjector {
    fn inject(&self, _binary: &mut Vec<u8>, _blob: &[u8]) -> Result<()> {
        Err(Error::UnsupportedFormat(
            "PE injection not yet implemented".into(),
        ))
    }
}
