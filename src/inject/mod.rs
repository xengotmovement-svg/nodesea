//! Binary injection engine.
//!
//! Handles injecting the SEA blob into platform-specific executable formats.

pub mod elf;
pub mod macho;
pub mod pe;
