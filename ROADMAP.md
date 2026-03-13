# Roadmap

## Implemented

- **SEA Blob V1 Serializer** — Node 20.x–24.5.x (8-byte header: magic + flags)
- **SEA Blob V2 Serializer** — Node 24.6.0+ (9-byte header with exec_argv_extension)
- **Flags Bitfield** — All known SEA flags: disable warning, snapshot, code cache, assets, exec_argv
- **sea-config.json Parser** — Compatible with Node.js format, validates required fields, converts to flags
- **Node Version Detection** — Runs `node --version`, maps to V1/V2 blob format
- **Fuse Scanner & Flipper** — Finds SEA fuse sentinel, flips `:0` → `:1`, detects already-enabled
- **Binary Format Detection** — Auto-detects ELF/Mach-O/PE via goblin
- **Mach-O Injection** — Injects blob as `NODE_SEA` segment with `__NODE_SEA_BLOB` section
- **ELF Injection** — Injects blob as `PT_NOTE` with name `NODE_SEA_BLOB`
- **macOS Code Signing** — Ad-hoc codesign via system `codesign` tool
- **CLI** — `--config`, `--node`, `--no-sign`, `--dry-run` flags

## Future Work

- **PE Injection (Windows)** — Inject blob as RCDATA resource `NODE_SEA_BLOB`
- **Code Cache Generation** — Generate V8 code cache for faster startup (requires V8 or Node)
- **V8 Snapshot Support** — Support snapshot payloads in the blob
- **Cross-Platform Code Signing** — Pure Rust Mach-O ad-hoc signing (for Linux → macOS builds)
- **npm Wrapper Package** — Thin npm package that downloads the correct Rust binary per platform
- **crates.io Publish** — Publish as a Rust crate with library API (`nodesea::build()`)
- **CI Matrix** — Test across Node 20/22/24/25 on Linux x86_64/aarch64 + macOS arm64/x86_64 + Windows
- **Fat/Universal Binary Support** — Handle macOS universal binaries (arm64 + x86_64)
- **PE Checksum Recalculation** — Fix PE checksum after resource injection on Windows
