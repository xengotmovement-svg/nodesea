//! Sea-config.json parsing and validation.
//!
//! Compatible with Node.js's own `sea-config.json` format.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::blob::SeaFlags;
use crate::error::{Error, Result};

fn default_true() -> bool {
    true
}

/// Parsed representation of a `sea-config.json` file.
///
/// Field names use `camelCase` to match the Node.js JSON format.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeaConfig {
    /// Path to the main JavaScript entry point.
    pub main: String,

    /// Path for the output executable.
    pub output: String,

    /// Suppress the "ExperimentalWarning: Single executable application" message.
    #[serde(default, rename = "disableExperimentalSEAWarning")]
    pub disable_experimental_sea_warning: bool,

    /// Whether the main payload is a V8 snapshot.
    #[serde(default)]
    pub use_snapshot: bool,

    /// Whether to include a V8 code cache in the blob.
    #[serde(default = "default_true")]
    pub use_code_cache: bool,

    /// Map of virtual asset name to file path on disk.
    #[serde(default)]
    pub assets: HashMap<String, String>,

    /// Extra `execArgv` flags baked into the executable (Node 24.6+).
    #[serde(default)]
    pub exec_argv: Vec<String>,
}

impl SeaConfig {
    /// Validate the config, returning an error if required fields are empty.
    pub fn validate(&self) -> Result<()> {
        if self.main.is_empty() {
            return Err(Error::InvalidConfig("'main' must not be empty".into()));
        }
        if self.output.is_empty() {
            return Err(Error::InvalidConfig("'output' must not be empty".into()));
        }
        Ok(())
    }

    /// Convert the boolean config fields into the corresponding [`SeaFlags`] bitfield.
    pub fn to_flags(&self) -> SeaFlags {
        let mut flags = SeaFlags::DEFAULT;

        if self.disable_experimental_sea_warning {
            flags |= SeaFlags::DISABLE_EXPERIMENTAL_SEA_WARNING;
        }
        if self.use_snapshot {
            flags |= SeaFlags::USE_SNAPSHOT;
        }
        if self.use_code_cache {
            flags |= SeaFlags::USE_CODE_CACHE;
        }
        if !self.assets.is_empty() {
            flags |= SeaFlags::INCLUDE_ASSETS;
        }
        if !self.exec_argv.is_empty() {
            flags |= SeaFlags::INCLUDE_EXEC_ARGV;
        }

        flags
    }
}

/// Read, parse, and validate a `sea-config.json` file at the given path.
pub fn from_path(path: impl AsRef<Path>) -> Result<SeaConfig> {
    let contents = std::fs::read_to_string(path.as_ref())?;
    let config: SeaConfig =
        serde_json::from_str(&contents).map_err(|e| Error::InvalidConfig(e.to_string()))?;
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r#"{ "main": "hello.js", "output": "hello" }"#;
        let config: SeaConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();

        assert_eq!(config.main, "hello.js");
        assert_eq!(config.output, "hello");
        assert!(!config.disable_experimental_sea_warning);
        assert!(!config.use_snapshot);
        assert!(config.use_code_cache);
        assert!(config.assets.is_empty());
        assert!(config.exec_argv.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let json = r#"{
            "main": "app.js",
            "output": "myapp",
            "disableExperimentalSEAWarning": true,
            "useSnapshot": true,
            "useCodeCache": true,
            "assets": { "config.json": "config.json", "data.bin": "data.bin" },
            "execArgv": ["--experimental-permission"]
        }"#;
        let config: SeaConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();

        assert_eq!(config.main, "app.js");
        assert_eq!(config.output, "myapp");
        assert!(config.disable_experimental_sea_warning);
        assert!(config.use_snapshot);
        assert!(config.use_code_cache);
        assert_eq!(config.assets.len(), 2);
        assert_eq!(config.assets["config.json"], "config.json");
        assert_eq!(config.exec_argv, vec!["--experimental-permission"]);
    }

    #[test]
    fn reject_missing_main() {
        // serde requires `main` — omitting it should fail to parse.
        let json = r#"{ "output": "hello" }"#;
        let result = serde_json::from_str::<SeaConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_main() {
        let json = r#"{ "main": "", "output": "hello" }"#;
        let config: SeaConfig = serde_json::from_str(json).unwrap();
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn config_to_flags_default() {
        let json = r#"{ "main": "hello.js", "output": "hello" }"#;
        let config: SeaConfig = serde_json::from_str(json).unwrap();
        let flags = config.to_flags();
        assert_eq!(flags, SeaFlags::USE_CODE_CACHE);
    }

    #[test]
    fn config_to_flags_full() {
        let json = r#"{
            "main": "app.js",
            "output": "myapp",
            "disableExperimentalSEAWarning": true,
            "useSnapshot": true,
            "useCodeCache": true,
            "assets": { "a.txt": "a.txt" },
            "execArgv": ["--flag"]
        }"#;
        let config: SeaConfig = serde_json::from_str(json).unwrap();
        let flags = config.to_flags();

        assert!(flags.contains(SeaFlags::DISABLE_EXPERIMENTAL_SEA_WARNING));
        assert!(flags.contains(SeaFlags::USE_SNAPSHOT));
        assert!(flags.contains(SeaFlags::USE_CODE_CACHE));
        assert!(flags.contains(SeaFlags::INCLUDE_ASSETS));
        assert!(flags.contains(SeaFlags::INCLUDE_EXEC_ARGV));
    }

    #[test]
    fn from_path_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("sea-config.json");
        std::fs::write(&config_path, r#"{ "main": "index.js", "output": "app" }"#).unwrap();

        let config = from_path(&config_path).unwrap();
        assert_eq!(config.main, "index.js");
        assert_eq!(config.output, "app");
    }

    #[test]
    fn from_path_rejects_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("sea-config.json");
        std::fs::write(&config_path, "not json").unwrap();

        let result = from_path(&config_path);
        assert!(result.is_err());
    }
}
