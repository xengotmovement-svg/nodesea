//! CLI entry point for nodesea.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// Pure Rust Node.js Single Executable Application (SEA) builder.
///
/// Build a standalone executable from a JavaScript source file and a Node.js
/// binary, without requiring Node.js at build time.
#[derive(Parser, Debug)]
#[command(name = "nodesea", version, about)]
struct Cli {
    /// JavaScript file to embed. Derives output name from the file stem
    /// (e.g. hello.js → hello). Mutually exclusive with --config.
    #[arg(value_name = "SCRIPT")]
    script: Option<PathBuf>,

    /// Output executable path (default: script name without extension).
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to sea-config.json (alternative to positional SCRIPT arg).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Path to the Node.js binary. Defaults to `node` in PATH, or
    /// auto-downloads from nodejs.org if not found.
    #[arg(long)]
    node: Option<PathBuf>,

    /// Node.js version to download (e.g. "22", "24", "22.16.0").
    /// Accepts major, major.minor, or exact versions. Only used when
    /// downloading; ignored if --node is set or node is in PATH.
    #[arg(long, default_value = "22")]
    node_version: String,

    /// Skip bundling — embed the script as-is without resolving imports.
    #[arg(long)]
    no_bundle: bool,

    /// Skip macOS ad-hoc code signing.
    #[arg(long)]
    no_sign: bool,

    /// Validate and show what would be done, without modifying files.
    #[arg(long)]
    dry_run: bool,
}

/// Resolve a `SeaConfig` either from --config or from positional args.
fn resolve_config(cli: &Cli) -> Result<(nodesea::config::SeaConfig, PathBuf)> {
    match (&cli.config, &cli.script) {
        (Some(config_path), None) => {
            let config = nodesea::config::from_path(config_path)
                .context("failed to load sea-config.json")?;
            let base_dir = config_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            Ok((config, base_dir))
        }
        (None, Some(script)) => {
            let main = script
                .to_str()
                .context("script path is not valid UTF-8")?
                .to_string();

            let output = match &cli.output {
                Some(o) => o
                    .to_str()
                    .context("output path is not valid UTF-8")?
                    .to_string(),
                None => script
                    .file_stem()
                    .context("cannot derive output name from script")?
                    .to_str()
                    .context("script stem is not valid UTF-8")?
                    .to_string(),
            };

            let config = nodesea::config::SeaConfig {
                main,
                output,
                disable_experimental_sea_warning: true,
                use_snapshot: false,
                use_code_cache: false,
                assets: Default::default(),
                exec_argv: Default::default(),
            };
            let base_dir = PathBuf::from(".");
            Ok((config, base_dir))
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("cannot use both SCRIPT argument and --config; pick one")
        }
        (None, None) => {
            anyhow::bail!(
                "provide a JavaScript file or use --config <path>\n\nUsage: nodesea <SCRIPT>\n       nodesea --config sea-config.json"
            )
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. Resolve config.
    let (config, base_dir) = resolve_config(&cli)?;

    // 2. Locate Node binary (explicit path > PATH > auto-download).
    let node_path = nodesea::node::resolve_node(cli.node.as_deref(), Some(&cli.node_version))
        .context("could not find or download Node.js")?;
    eprintln!("[nodesea] Using node: {}", node_path.display());

    // 3. Detect Node version.
    let node_version =
        nodesea::version::detect_version(&node_path).context("failed to detect Node.js version")?;
    eprintln!("[nodesea] Node.js {node_version}");

    // 4. Determine blob version.
    let blob_version = nodesea::version::blob_version(&node_version)
        .context("failed to determine blob version")?;

    // 5. Bundle or read the main JS file.
    let main_path = base_dir.join(&config.main);
    let main_code = if cli.no_bundle {
        std::fs::read(&main_path)
            .with_context(|| format!("failed to read: {}", main_path.display()))?
    } else {
        eprintln!("[nodesea] Bundling {}...", main_path.display());
        nodesea::bundle::bundle(&main_path)
            .with_context(|| format!("failed to bundle: {}", main_path.display()))?
    };

    // 6. Read asset files.
    let mut asset_data: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, file_path) in &config.assets {
        let full_path = base_dir.join(file_path);
        let data = std::fs::read(&full_path)
            .with_context(|| format!("failed to read asset '{name}': {}", full_path.display()))?;
        asset_data.push((name.clone(), data));
    }

    // 7. Serialize the blob.
    let flags = config.to_flags();
    let code_path = format!("/sea/{}", config.main);

    let assets_refs: Vec<(&str, &[u8])> = asset_data
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    let assets_opt = if assets_refs.is_empty() {
        None
    } else {
        Some(assets_refs.as_slice())
    };

    let exec_argv_refs: Vec<&str> = config.exec_argv.iter().map(|s| s.as_str()).collect();
    let exec_argv_opt = if exec_argv_refs.is_empty() {
        None
    } else {
        Some(exec_argv_refs.as_slice())
    };

    let blob = nodesea::blob::serialize(
        blob_version,
        &code_path,
        &main_code,
        flags,
        None,
        assets_opt,
        exec_argv_opt,
    )
    .context("failed to serialize SEA blob")?;
    eprintln!("[nodesea] Blob: {} bytes", blob.len());

    // 8. Dry-run: print summary and exit.
    if cli.dry_run {
        eprintln!();
        eprintln!("[nodesea] === DRY RUN ===");
        eprintln!("[nodesea] Script:      {}", main_path.display());
        eprintln!("[nodesea] Output:      {}", config.output);
        eprintln!(
            "[nodesea] Node:        {} (v{node_version})",
            node_path.display()
        );
        eprintln!(
            "[nodesea] Blob format: {blob_version:?}, {} bytes",
            blob.len()
        );
        eprintln!("[nodesea] No files were modified.");
        return Ok(());
    }

    // 9. Copy the Node binary to the output path.
    let output_path = base_dir.join(&config.output);
    std::fs::copy(&node_path, &output_path).with_context(|| {
        format!(
            "failed to copy {} -> {}",
            node_path.display(),
            output_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to set permissions on {}", output_path.display()))?;
    }

    // 10. Inject blob + flip fuse.
    eprintln!("[nodesea] Injecting blob...");
    let mut binary = std::fs::read(&output_path)
        .with_context(|| format!("failed to read: {}", output_path.display()))?;

    nodesea::inject::inject(&mut binary, &blob).context("failed to inject SEA blob")?;

    eprintln!("[nodesea] Flipping fuse...");
    nodesea::fuse::flip_fuse(&mut binary).context("failed to flip SEA fuse")?;

    std::fs::write(&output_path, &binary)
        .with_context(|| format!("failed to write: {}", output_path.display()))?;

    // 11. Code sign on macOS.
    #[cfg(target_os = "macos")]
    {
        if !cli.no_sign {
            eprintln!("[nodesea] Signing...");
            nodesea::codesign::codesign_adhoc(&output_path).context("failed to codesign output")?;
        }
    }

    eprintln!("[nodesea] Built: {}", output_path.display());
    Ok(())
}
