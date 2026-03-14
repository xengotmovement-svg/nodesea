//! JavaScript bundling via rolldown.
//!
//! Bundles a JavaScript entry point and all its imports/requires into a single
//! file suitable for embedding in a Node.js SEA binary.
//!
//! When the source uses top-level `await` (ESM-only feature), rolldown cannot
//! output CJS directly. We work around this with a two-pass approach:
//!
//!   Pass 1 (ESM): Bundle everything into a single file (TLA allowed in ESM).
//!   Wrap:         Insert `(async () => {` / `})();` around the body so `await`
//!                 is no longer at the top level.
//!   Pass 2 (CJS): Re-bundle the wrapped file as CJS. Rolldown natively handles
//!                 `import` → `require()`, `import.meta` polyfills, and interop.
//!
//! This avoids any manual ESM→CJS string manipulation — rolldown does it all.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Bundle a JavaScript entry point into a single file, resolving all imports.
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
        // Try CJS directly first.
        match try_bundle(&relative_entry, &cwd, rolldown::OutputFormat::Cjs).await {
            Ok(code) => Ok(code),
            Err(e) if e.contains("Top-level await") => {
                eprintln!("[nodesea] Top-level await detected, using two-pass bundling...");
                two_pass_bundle(&relative_entry, &cwd).await
            }
            Err(e) => Err(Error::BlobError(format!("rolldown bundle failed: {e}"))),
        }
    })
}

/// Two-pass bundling for code with top-level await.
///
/// Pass 1: ESM bundle → single file with only external imports remaining.
/// Wrap:   Move `await` into an async IIFE so it's no longer top-level.
/// Pass 2: CJS bundle → rolldown converts imports, polyfills import.meta, etc.
async fn two_pass_bundle(entry: &str, cwd: &PathBuf) -> Result<Vec<u8>> {
    // Pass 1: Bundle as ESM (TLA is allowed).
    let esm_code = try_bundle(entry, cwd, rolldown::OutputFormat::Esm)
        .await
        .map_err(|e| Error::BlobError(format!("rolldown ESM bundle failed: {e}")))?;

    // Wrap the body in an async IIFE so `await` is inside a function scope.
    let wrapped = wrap_tla_in_async_iife(&esm_code);

    // Write to a temp .mjs file for pass 2.
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| Error::BlobError(format!("failed to create temp dir: {e}")))?;
    let tmp_file = tmp_dir.path().join("__nodesea_bundle.mjs");
    std::fs::write(&tmp_file, &wrapped)
        .map_err(|e| Error::BlobError(format!("failed to write temp file: {e}")))?;

    // Pass 2: Re-bundle as CJS. Rolldown natively handles:
    // - `import` → `require()` conversion
    // - `import.meta.url` → `require('url').pathToFileURL(__filename).href`
    // - `import.meta.dirname` / `import.meta.filename` → `__dirname` / `__filename`
    // - CJS/ESM interop (`__toESM`, `__commonJS`)
    // No TLA error because `await` is now inside a function.
    let cjs_code = try_bundle(
        "./__nodesea_bundle.mjs",
        &tmp_dir.path().to_path_buf(),
        rolldown::OutputFormat::Cjs,
    )
    .await
    .map_err(|e| Error::BlobError(format!("rolldown CJS bundle failed: {e}")))?;

    Ok(cjs_code)
}

/// Wrap the module body (between imports and exports) in an async IIFE.
///
/// Input (ESM):
/// ```js
/// import fs from 'node:fs';
/// const x = await fetch(...);
/// export { x };
/// ```
///
/// Output:
/// ```js
/// import fs from 'node:fs';
/// (async () => {
/// const x = await fetch(...);
/// })();
/// export { x };
/// ```
fn wrap_tla_in_async_iife(esm_code: &[u8]) -> Vec<u8> {
    let code = String::from_utf8_lossy(esm_code);
    let lines: Vec<&str> = code.lines().collect();

    // Find where imports end (last consecutive import/re-export line from the top).
    let mut import_end = 0;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("import ")
            || trimmed.starts_with("export * from ")
            || trimmed.starts_with("export {") && trimmed.contains(" from ")
        {
            import_end = i + 1;
        } else if !trimmed.starts_with("//") && !trimmed.starts_with('#') {
            break;
        }
    }

    // Find where trailing exports start (first export line from the bottom).
    let mut export_start = lines.len();
    for i in (import_end..lines.len()).rev() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("export ") {
            export_start = i;
        } else {
            break;
        }
    }

    let mut out = String::with_capacity(code.len() + 30);

    // Emit import lines as-is.
    for line in &lines[..import_end] {
        out.push_str(line);
        out.push('\n');
    }

    // Open async IIFE.
    out.push_str("(async () => {\n");

    // Emit body lines.
    for line in &lines[import_end..export_start] {
        out.push_str(line);
        out.push('\n');
    }

    // Close async IIFE.
    out.push_str("})();\n");

    // Emit export lines as-is.
    for line in &lines[export_start..] {
        out.push_str(line);
        out.push('\n');
    }

    out.into_bytes()
}

/// Build the `define` map for compile-time expression replacement.
fn build_define_map() -> rolldown_utils::indexmap::FxIndexMap<String, String> {
    let mut define = rolldown_utils::indexmap::FxIndexMap::default();
    // Strip inline vitest test blocks (dead-code eliminated by the bundler).
    define.insert("import.meta.vitest".into(), "undefined".into());
    define
}

/// Run rolldown with the given output format and return the entry chunk code.
async fn try_bundle(
    entry: &str,
    cwd: &PathBuf,
    format: rolldown::OutputFormat,
) -> std::result::Result<Vec<u8>, String> {
    let mut bundler = rolldown::Bundler::new(rolldown::BundlerOptions {
        input: Some(vec![entry.to_string().into()]),
        cwd: Some(cwd.clone()),
        platform: Some(rolldown::Platform::Node),
        format: Some(format),
        code_splitting: Some(rolldown_common::CodeSplittingMode::Bool(false)),
        define: Some(build_define_map()),
        ..Default::default()
    })
    .map_err(|e| format!("rolldown init failed: {e}"))?;

    let output = bundler.generate().await.map_err(|e| e.to_string())?;

    let chunk = output
        .assets
        .iter()
        .find_map(|o| match o {
            rolldown_common::Output::Chunk(c) if c.is_entry => Some(c),
            _ => None,
        })
        .ok_or_else(|| "rolldown produced no entry chunk".to_string())?;

    Ok(chunk.code.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_tla_simple() {
        let esm = b"import fs from 'node:fs';\nawait fs.readFile('x');\n";
        let wrapped = String::from_utf8(wrap_tla_in_async_iife(esm)).unwrap();
        assert!(wrapped.starts_with("import fs from 'node:fs';\n"));
        assert!(wrapped.contains("(async () => {\n"));
        assert!(wrapped.contains("await fs.readFile('x');"));
        assert!(wrapped.contains("})();\n"));
    }

    #[test]
    fn wrap_tla_with_exports() {
        let esm = b"import fs from 'node:fs';\nconst x = await fetch('/');\nexport { x };\nexport default x;\n";
        let wrapped = String::from_utf8(wrap_tla_in_async_iife(esm)).unwrap();
        // Imports before IIFE.
        assert!(wrapped.starts_with("import fs from 'node:fs';\n(async"));
        // Exports after IIFE.
        assert!(wrapped.contains("})();\nexport { x };\n"));
    }

    #[test]
    fn wrap_tla_no_imports() {
        let esm = b"const x = await fetch('/');\nconsole.log(x);\n";
        let wrapped = String::from_utf8(wrap_tla_in_async_iife(esm)).unwrap();
        assert!(wrapped.starts_with("(async () => {\n"));
        assert!(wrapped.contains("await fetch('/')"));
        assert!(wrapped.ends_with("})();\n"));
    }

    #[test]
    fn wrap_tla_re_exports() {
        let esm = b"import a from 'x';\nexport * from 'y';\nconst z = await a();\n";
        let wrapped = String::from_utf8(wrap_tla_in_async_iife(esm)).unwrap();
        // Both import and re-export should be above the IIFE.
        assert!(wrapped.contains("import a from 'x';\nexport * from 'y';\n(async"));
    }
}
