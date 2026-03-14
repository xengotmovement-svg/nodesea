//! V8 code cache generation.
//!
//! Shells out to the target Node.js binary to compile the bundled JavaScript
//! source and extract V8 code cache bytes. The cache lets V8 skip parsing and
//! compilation on startup, improving cold-start time.
//!
//! Must use `vm.compileFunction()` with CJS wrapper parameters (`exports`,
//! `require`, `module`, `__filename`, `__dirname`) because Node.js SEA
//! deserializes with `ScriptCompiler::CompileFunction()`. Using `vm.Script`
//! would produce cache bytes that V8 rejects at runtime.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Generate V8 code cache for the given JavaScript source.
///
/// # Arguments
/// * `node_path` — Path to the Node.js binary (must be the same binary that
///   will host the SEA, since V8 code cache is version-specific).
/// * `source` — Bundled JavaScript source bytes.
/// * `code_path` — Virtual filename for the embedded script (e.g. "/sea/main.js").
pub fn generate_code_cache(node_path: &Path, source: &[u8], code_path: &str) -> Result<Vec<u8>> {
    let dir = tempfile::tempdir()?;
    let source_file = dir.path().join("source.js");
    std::fs::write(&source_file, source)?;

    let script = r#"
const vm = require('vm');
const fs = require('fs');
const source = fs.readFileSync(process.argv[1], 'utf8');
const compiled = vm.compileFunction(source,
    ['exports', 'require', 'module', '__filename', '__dirname'],
    { filename: process.argv[2], produceCachedData: true }
);
process.stdout.write(compiled.cachedData);
"#;

    let output = Command::new(node_path)
        .arg("-e")
        .arg(script)
        .arg(&source_file)
        .arg(code_path)
        .output()
        .map_err(|e| Error::Io(e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::CodeCacheError(format!(
            "code cache generation failed (exit {}): {}",
            output.status,
            stderr.trim()
        )));
    }

    if output.stdout.is_empty() {
        return Err(Error::CodeCacheError(
            "code cache generation produced no output".into(),
        ));
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_code_cache_simple() {
        // Skip if node is not available.
        let node = match which::which("node") {
            Ok(p) => p,
            Err(_) => return,
        };

        let source = b"console.log('hello');";
        let cache = generate_code_cache(&node, source, "/sea/test.js").unwrap();
        assert!(!cache.is_empty(), "code cache should not be empty");
        // V8 code cache starts with a magic number; just verify we got bytes.
        assert!(cache.len() > 16, "code cache should be non-trivial");
    }

    #[test]
    fn generate_code_cache_invalid_node_path() {
        let result = generate_code_cache(
            Path::new("/nonexistent/node"),
            b"console.log('hello');",
            "/sea/test.js",
        );
        assert!(result.is_err());
    }
}
