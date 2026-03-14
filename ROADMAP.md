# Roadmap

## Implemented

- **SEA Blob V1 Serializer** — Node 20.x–24.5.x (8-byte header: magic + flags)
- **SEA Blob V2 Serializer** — Node 24.6.0+ (9-byte header with exec_argv_extension)
- **Flags Bitfield** — All known SEA flags: disable warning, snapshot, code cache, assets, exec_argv
- **sea-config.json Parser** — Compatible with Node.js format, validates required fields, converts to flags
- **Node Version Detection** — Runs `node --version`, maps to V1/V2 blob format
- **Fuse Scanner & Flipper** — Finds SEA fuse sentinel, flips `:0` → `:1`, detects already-enabled
- **Binary Format Detection** — Auto-detects ELF/Mach-O/PE via goblin
- **Mach-O Injection** — Injects blob as `NODE_SEA` segment with `__NODE_SEA_BLOB` section, relocates `__LINKEDIT` to satisfy codesign constraints
- **ELF Injection** — Injects blob as `PT_NOTE` with name `NODE_SEA_BLOB`
- **macOS Code Signing** — Ad-hoc codesign via system `codesign` tool
- **CLI** — Positional script arg, `--config`, `--node`, `--node-version`, `--no-bundle`, `--no-sign`, `--dry-run`
- **Config-Optional Mode** — `nodesea app.js` works without a config file
- **Built-in Bundling** — Bundles JS/TS imports and `node_modules` via rolldown (in-process)
- **TypeScript Support** — `.ts` files bundled natively, no `tsc` or `tsconfig.json` needed
- **Auto-Download Node.js** — Downloads from nodejs.org when not installed, cached at `~/.nodesea/cache/`
- **Version Resolution** — `--node-version 24` resolves to latest 24.x.x via nodejs.org index
- **GitHub Actions Release** — CI builds binaries for linux-x64, linux-arm64, darwin-x64, darwin-arm64
- **Install Script** — `curl | sh` one-liner for macOS and Linux

## Future Work

- **PE Injection (Windows)** — Inject blob as RCDATA resource `NODE_SEA_BLOB`
- **Windows Release Artifacts** — Add `x86_64-pc-windows-msvc` to CI matrix
- **Code Cache Generation** — Generate V8 code cache for faster startup (requires V8 or Node)
- **V8 Snapshot Support** — Support snapshot payloads in the blob
- **Cross-Platform Code Signing** — Pure Rust Mach-O ad-hoc signing (for Linux → macOS builds)
- **Fat/Universal Binary Support** — Handle macOS universal binaries (arm64 + x86_64)
- **npm Binary Package** — Thin npm package that downloads the correct binary per platform (like esbuild)
- **CI Test Matrix** — Test across Node 20/22/24/25 on Linux + macOS
- **PE Checksum Recalculation** — Fix PE checksum after resource injection on Windows
- **Native Addon Bundling** — Automate embedding `.node` files as SEA assets with runtime extraction
