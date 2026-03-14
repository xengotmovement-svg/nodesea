//! # nodesea
//!
//! Pure Rust Node.js Single Executable Application (SEA) builder.
//!
//! Takes a JavaScript source file and a Node.js binary, produces a standalone
//! executable that Node.js's SEA runtime recognizes and runs — without requiring
//! Node.js at build time.
//!
//! ## Pipeline
//!
//! 1. Parse `sea-config.json` for build configuration
//! 2. Read JavaScript source and optional assets
//! 3. Detect Node.js version from the target binary
//! 4. Serialize the SEA blob in the correct binary format
//! 5. Copy the Node binary and inject the blob (ELF/Mach-O/PE)
//! 6. Flip the SEA fuse sentinel to activate SEA mode
//! 7. Re-sign on macOS (ad-hoc codesign)

pub mod blob;
pub mod bundle;
pub mod codesign;
pub mod config;
pub mod error;
pub mod fuse;
pub mod inject;
pub mod node;
pub mod version;
