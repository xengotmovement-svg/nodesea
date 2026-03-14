//! Error types for the nodesea library.
//!
//! Uses `thiserror` for structured, typed errors in the library layer.
//! The CLI (`main.rs`) wraps these with `anyhow` for ergonomic error reporting.

use thiserror::Error;

/// All errors that can occur in the nodesea library.
#[derive(Debug, Error)]
pub enum Error {
    /// The Node.js binary does not contain the SEA fuse sentinel.
    #[error("SEA fuse not found — is this a Node.js binary?")]
    FuseNotFound,

    /// The SEA fuse has already been flipped to enabled.
    #[error("SEA fuse is already enabled")]
    FuseAlreadyEnabled,

    /// The Node.js version is too old to support SEA.
    #[error("Node.js {0} does not support SEA (requires >= 20.0.0)")]
    UnsupportedNodeVersion(String),

    /// Failed to detect the Node.js version from the binary.
    #[error("could not detect Node.js version: {0}")]
    VersionDetectionFailed(String),

    /// The binary format is not supported or not recognized.
    #[error("unsupported binary format: {0}")]
    UnsupportedFormat(String),

    /// Not enough space in the binary header for injection.
    #[error("not enough header space for injection: need {need} bytes, have {have}")]
    InsufficientHeaderSpace { need: usize, have: usize },

    /// SEA config validation error.
    #[error("invalid sea-config.json: {0}")]
    InvalidConfig(String),

    /// Blob serialization error.
    #[error("blob serialization error: {0}")]
    BlobError(String),

    /// Code signing failed.
    #[error("code signing failed: {0}")]
    CodesignFailed(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Node.js binary download failed.
    #[error("download failed: {0}")]
    DownloadFailed(String),

    /// Binary parsing error from goblin.
    #[error("binary parse error: {0}")]
    GoblinError(String),
}

/// Result type alias using the nodesea `Error`.
pub type Result<T> = std::result::Result<T, Error>;
