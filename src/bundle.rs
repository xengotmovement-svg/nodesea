//! JavaScript bundling via rolldown.
//!
//! Bundles a JavaScript entry point and all its imports/requires into a single
//! file suitable for embedding in a Node.js SEA binary.

use std::path::Path;

use crate::error::{Error, Result};

/// Bundle a JavaScript entry point into a single file, resolving all imports.
///
/// Uses rolldown to bundle with `platform: node` and `format: cjs` so that
/// Node.js built-in modules (fs, path, etc.) are treated as external and the
/// output is a single self-contained CommonJS file.
///
/// Returns the bundled JavaScript source as bytes.
pub fn bundle(entry: &Path) -> Result<Vec<u8>> {
    let entry_str = entry
        .to_str()
        .ok_or_else(|| Error::BlobError("entry path is not valid UTF-8".into()))?
        .to_string();

    let cwd = entry
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(Error::Io)?;

    // The entry path relative to cwd, or just the filename.
    let relative_entry = entry
        .file_name()
        .and_then(|f| f.to_str())
        .map(|s| format!("./{s}"))
        .unwrap_or(entry_str);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::BlobError(format!("failed to create tokio runtime: {e}")))?;

    rt.block_on(async {
        let mut bundler = rolldown::Bundler::new(rolldown::BundlerOptions {
            input: Some(vec![relative_entry.into()]),
            cwd: Some(cwd),
            platform: Some(rolldown::Platform::Node),
            format: Some(rolldown::OutputFormat::Cjs),
            ..Default::default()
        })
        .map_err(|e| Error::BlobError(format!("rolldown init failed: {e}")))?;

        let output = bundler
            .generate()
            .await
            .map_err(|e| Error::BlobError(format!("rolldown bundle failed: {e}")))?;

        // Find the entry chunk — iterate assets and look for an entry chunk.
        let chunk = output
            .assets
            .iter()
            .find_map(|o| match o {
                rolldown_common::Output::Chunk(c) if c.is_entry => Some(c),
                _ => None,
            })
            .ok_or_else(|| Error::BlobError("rolldown produced no entry chunk".into()))?;

        Ok(chunk.code.as_bytes().to_vec())
    })
}
