//! End-to-end integration tests for nodesea.
//!
//! These tests require Node.js to be installed and are marked `#[ignore]`.
//! Run with: `cargo test -- --ignored`

use std::fs;
use std::path::Path;
use std::process::Command;

/// Full round-trip: build a SEA binary from hello.js and run it.
#[test]
#[ignore]
fn sea_round_trip() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let tmp_dir = tmp.path();

    // Copy fixture files to the temp directory.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fs::copy(fixtures.join("hello.js"), tmp_dir.join("hello.js")).expect("failed to copy hello.js");

    // Write sea-config.json with output path pointing into the temp dir.
    let output_name = if cfg!(target_os = "windows") {
        "hello-sea.exe"
    } else {
        "hello-sea"
    };
    let config_json = serde_json::json!({
        "main": "hello.js",
        "output": output_name,
        "disableExperimentalSEAWarning": true
    });
    let config_path = tmp_dir.join("sea-config.json");
    fs::write(&config_path, config_json.to_string()).expect("failed to write sea-config.json");

    // Locate Node.js.
    let node_path = which::which("node").expect("node not found in PATH");
    eprintln!("Using node: {}", node_path.display());

    // 1. Load config.
    let config = nodesea::config::from_path(&config_path).expect("failed to load sea-config.json");

    // 2. Detect Node.js version.
    let node_version =
        nodesea::version::detect_version(&node_path).expect("failed to detect node version");
    eprintln!("Node.js version: {node_version}");

    // 3. Determine blob version.
    let blob_ver =
        nodesea::version::blob_version(&node_version).expect("failed to determine blob version");
    eprintln!("Blob version: {blob_ver:?}");

    // 4. Read JS source.
    let main_path = tmp_dir.join(&config.main);
    let main_code = fs::read(&main_path).expect("failed to read hello.js");

    // 5. Serialize blob.
    let flags = config.to_flags();
    let code_path = format!("/sea/{}", config.main);
    let blob = nodesea::blob::serialize(blob_ver, &code_path, &main_code, flags, None, None, None)
        .expect("failed to serialize blob");
    eprintln!("Blob size: {} bytes", blob.len());

    // 6. Copy node binary to output path.
    let output_path = tmp_dir.join(&config.output);
    fs::copy(&node_path, &output_path).expect("failed to copy node binary");

    // Set executable permissions on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&output_path, fs::Permissions::from_mode(0o755))
            .expect("failed to set executable permissions");
    }

    // 7. Read the copy, inject blob.
    let mut binary = fs::read(&output_path).expect("failed to read output binary");
    nodesea::inject::inject(&mut binary, &blob).expect("failed to inject blob");

    // 8. Flip fuse.
    if nodesea::fuse::flip_fuse(&mut binary).is_err() {
        eprintln!(
            "SKIPPED: Node binary does not contain SEA fuse sentinel (custom or minimal build)"
        );
        return;
    }

    // 9. Write back.
    fs::write(&output_path, &binary).expect("failed to write modified binary");

    // 10. On macOS, codesign.
    #[cfg(target_os = "macos")]
    {
        nodesea::codesign::codesign_adhoc(&output_path).expect("failed to codesign");
    }

    // 11. Run the output binary and verify output.
    let run_output = Command::new(&output_path)
        .output()
        .expect("failed to run SEA binary");

    let stdout = String::from_utf8_lossy(&run_output.stdout);
    let stderr = String::from_utf8_lossy(&run_output.stderr);
    eprintln!("stdout: {stdout}");
    eprintln!("stderr: {stderr}");

    assert!(
        run_output.status.success(),
        "SEA binary exited with status: {}",
        run_output.status,
    );
    assert!(
        stdout.contains("Hello from SEA!"),
        "expected 'Hello from SEA!' in stdout, got: {stdout}",
    );
}

/// Dry-run mode: run the CLI with --dry-run and verify it prints summary
/// without creating the output file.
#[test]
#[ignore]
fn dry_run_does_not_create_output() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let tmp_dir = tmp.path();

    // Copy fixture files.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fs::copy(fixtures.join("hello.js"), tmp_dir.join("hello.js")).expect("failed to copy hello.js");

    let output_name = if cfg!(target_os = "windows") {
        "hello-sea.exe"
    } else {
        "hello-sea"
    };
    let config_json = serde_json::json!({
        "main": "hello.js",
        "output": output_name,
        "disableExperimentalSEAWarning": true
    });
    let config_path = tmp_dir.join("sea-config.json");
    fs::write(&config_path, config_json.to_string()).expect("failed to write sea-config.json");

    // Run CLI with --dry-run via `cargo run` (using --config mode).
    let cargo_run = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "--config",
            config_path.to_str().unwrap(),
            "--dry-run",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run cargo run");

    let stderr = String::from_utf8_lossy(&cargo_run.stderr);
    eprintln!("dry-run stderr:\n{stderr}");

    assert!(
        cargo_run.status.success(),
        "cargo run --dry-run failed with status: {}\nstderr: {stderr}",
        cargo_run.status,
    );

    // Verify dry-run summary appears in stderr.
    assert!(
        stderr.contains("DRY RUN"),
        "expected 'DRY RUN' in stderr output",
    );
    assert!(
        stderr.contains("No files were modified"),
        "expected 'No files were modified' in stderr output",
    );

    // Verify no output file was created.
    let output_path = tmp_dir.join(output_name);
    assert!(
        !output_path.exists(),
        "output file should not exist after dry-run: {}",
        output_path.display(),
    );
}
