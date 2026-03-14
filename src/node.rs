//! Automatic Node.js binary download and caching.
//!
//! Downloads official Node.js binaries from <https://nodejs.org/dist/> and
//! caches them under `~/.nodesea/cache/`. Subsequent runs reuse the cached
//! binary without re-downloading.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Default Node.js major version to download when none is specified.
const DEFAULT_NODE_MAJOR: &str = "22";

/// URL for the Node.js release index.
const NODE_DIST_INDEX: &str = "https://nodejs.org/dist/index.json";

/// Return the cache directory: `~/.nodesea/cache/`.
fn cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| Error::DownloadFailed("cannot determine home directory".into()))?;
    Ok(PathBuf::from(home).join(".nodesea").join("cache"))
}

/// Platform string for the nodejs.org download URL.
fn platform() -> Result<&'static str> {
    match std::env::consts::OS {
        "macos" => Ok("darwin"),
        "linux" => Ok("linux"),
        "windows" => Ok("win"),
        other => Err(Error::DownloadFailed(format!(
            "unsupported platform: {other}"
        ))),
    }
}

/// Architecture string for the nodejs.org download URL.
fn arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("x64"),
        "x86" => Ok("x86"),
        other => Err(Error::DownloadFailed(format!(
            "unsupported architecture: {other}"
        ))),
    }
}

/// Ensure a Node.js binary is available, downloading if necessary.
///
/// Returns the path to the `node` executable.
///
/// Resolution order:
/// 1. If `explicit` is `Some`, use that path directly.
/// 2. If `node` is found in PATH, use it.
/// 3. Download from nodejs.org and cache.
pub fn resolve_node(
    explicit: Option<&Path>,
    version: Option<&str>,
) -> Result<PathBuf> {
    // 1. Explicit --node flag.
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }

    // 2. Found in PATH.
    if let Ok(p) = which::which("node") {
        return Ok(p);
    }

    // 3. Download.
    let input = version.unwrap_or(DEFAULT_NODE_MAJOR);
    let resolved = resolve_version(input)?;
    download_node(&resolved)
}

/// Resolve a version specifier to an exact version string.
///
/// Accepts:
/// - Full version: `"22.16.0"` → `"22.16.0"`
/// - Major only: `"22"` → latest 22.x.x release
/// - Major.minor: `"22.16"` → latest 22.16.x release
///
/// Queries `https://nodejs.org/dist/index.json` for partial versions.
pub fn resolve_version(input: &str) -> Result<String> {
    let parts: Vec<&str> = input.split('.').collect();

    // Already a full x.y.z version — use as-is.
    if parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return Ok(input.to_string());
    }

    // Need to resolve — fetch the release index.
    eprintln!("[nodesea] Resolving Node.js v{input}...");

    let response = ureq::get(NODE_DIST_INDEX)
        .call()
        .map_err(|e| Error::DownloadFailed(format!("fetch release index: {e}")))?;

    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| Error::DownloadFailed(format!("read release index: {e}")))?;

    let entries: Vec<serde_json::Value> = serde_json::from_str(&body)
        .map_err(|e| Error::DownloadFailed(format!("parse release index: {e}")))?;

    let prefix = format!("v{input}.");

    // The index is sorted newest-first. Find the first entry matching our prefix.
    for entry in &entries {
        if let Some(version) = entry.get("version").and_then(|v| v.as_str()) {
            let matches = match parts.len() {
                // Major only: "22" matches "v22.x.y"
                1 => version.starts_with(&prefix),
                // Major.minor: "22.16" matches "v22.16.y"
                2 => version.starts_with(&prefix),
                _ => false,
            };

            if matches {
                let resolved = version.trim_start_matches('v');
                eprintln!("[nodesea] Resolved to v{resolved}");
                return Ok(resolved.to_string());
            }
        }
    }

    Err(Error::DownloadFailed(format!(
        "no Node.js release found matching v{input}"
    )))
}

/// Download a Node.js binary from nodejs.org and return the path to it.
///
/// The binary is cached at:
///   `~/.nodesea/cache/node-v{version}-{platform}-{arch}/bin/node`
pub fn download_node(version: &str) -> Result<PathBuf> {
    let plat = platform()?;
    let ar = arch()?;
    let dir_name = format!("node-v{version}-{plat}-{ar}");

    let cache = cache_dir()?;
    let node_dir = cache.join(&dir_name);

    let node_bin = if plat == "win" {
        node_dir.join("node.exe")
    } else {
        node_dir.join("bin").join("node")
    };

    // Already cached?
    if node_bin.exists() {
        return Ok(node_bin);
    }

    // Build download URL.
    let (url, is_tar) = if plat == "win" {
        (
            format!(
                "https://nodejs.org/dist/v{version}/{dir_name}.zip"
            ),
            false,
        )
    } else {
        (
            format!(
                "https://nodejs.org/dist/v{version}/{dir_name}.tar.gz"
            ),
            true,
        )
    };

    eprintln!("[nodesea] Downloading Node.js v{version} ({plat}-{ar})...");
    eprintln!("[nodesea] {url}");

    let response = ureq::get(&url)
        .call()
        .map_err(|e| Error::DownloadFailed(format!("{e}")))?;

    // Read body into memory.
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|e| Error::DownloadFailed(format!("read body: {e}")))?;

    eprintln!(
        "[nodesea] Downloaded {} MB",
        body.len() / (1024 * 1024)
    );

    // Extract.
    std::fs::create_dir_all(&cache)
        .map_err(|e| Error::DownloadFailed(format!("create cache dir: {e}")))?;

    if is_tar {
        extract_tar_gz(&body, &cache, &dir_name)?;
    } else {
        return Err(Error::DownloadFailed(
            "Windows .zip extraction not yet implemented".into(),
        ));
    }

    if !node_bin.exists() {
        return Err(Error::DownloadFailed(format!(
            "extraction succeeded but node binary not found at {}",
            node_bin.display()
        )));
    }

    // Ensure executable permission.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&node_bin, std::fs::Permissions::from_mode(0o755));
    }

    eprintln!("[nodesea] Cached at {}", node_bin.display());
    Ok(node_bin)
}

/// Extract a `.tar.gz` archive, keeping only the `bin/node` file and the
/// top-level directory structure.
fn extract_tar_gz(data: &[u8], dest: &Path, expected_prefix: &str) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(data);
    let mut archive = Archive::new(gz);

    for entry in archive
        .entries()
        .map_err(|e| Error::DownloadFailed(format!("tar entries: {e}")))?
    {
        let mut entry = entry.map_err(|e| Error::DownloadFailed(format!("tar entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| Error::DownloadFailed(format!("tar path: {e}")))?
            .to_path_buf();

        let path_str = path.to_string_lossy();

        // Only extract bin/node (the binary we need).
        let is_node_bin = path_str.ends_with("/bin/node")
            && path_str.starts_with(expected_prefix);

        if !is_node_bin {
            continue;
        }

        let out_path = dest.join(&path);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::DownloadFailed(format!("mkdir: {e}")))?;
        }

        entry
            .unpack(&out_path)
            .map_err(|e| Error::DownloadFailed(format!("unpack {}: {e}", path_str)))?;

        return Ok(());
    }

    Err(Error::DownloadFailed(format!(
        "bin/node not found in archive (prefix: {expected_prefix})"
    )))
}
