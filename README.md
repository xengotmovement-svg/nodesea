# nodesea

Pure Rust Node.js Single Executable Application (SEA) builder.

Takes a JavaScript or TypeScript source file and a Node.js binary, produces a standalone executable — **without requiring Node.js at build time**.

## Features

- **Built-in bundling** — automatically bundles imports and `node_modules` via [rolldown](https://rolldown.rs)
- **TypeScript support** — `.ts` files are bundled natively, no `tsc` or `tsconfig.json` needed
- **Auto-download** — if Node.js isn't installed, downloads it from nodejs.org automatically
- **Zero Node.js dependency** — no `node`, `npm`, or `postject` needed during build
- **Multi-version support** — Node.js 20.x through 25.x+ (blob V1 and V2 formats)
- **Cross-platform injection** — Mach-O (macOS), ELF (Linux), PE (Windows, planned)
- **Automatic version detection** — detects Node.js version and selects the correct blob format
- **macOS code signing** — automatic ad-hoc re-signing after injection
- **Config compatible** — uses the same `sea-config.json` format as Node.js

## Quick Start

```bash
# Install
cargo install --path .

# Create your app
echo 'console.log("Hello from SEA!")' > hello.js

# Build — that's it
nodesea hello.js

# Run it
./hello
# => Hello from SEA!
```

## Usage

```
nodesea <script.js>                    # output name derived from script
nodesea <script.js> -o <output>        # explicit output name
nodesea --config sea-config.json       # use Node.js-compatible config file
```

| Flag | Description |
|------|-------------|
| `<SCRIPT>` | JavaScript file to embed (derives output name from file stem) |
| `-o, --output` | Output executable path (default: script name without extension) |
| `--config <path>` | Path to `sea-config.json` (alternative to positional arg) |
| `--node <path>` | Path to Node.js binary (default: `node` in PATH, or auto-download) |
| `--node-version <ver>` | Node.js version to download — `22`, `24`, `22.16.0` (default: `22`) |
| `--no-bundle` | Skip bundling — embed the script as-is |
| `--no-sign` | Skip macOS ad-hoc code signing |
| `--dry-run` | Validate and show build plan without modifying files |

## Bundling

By default, nodesea bundles your script and all its imports into a single file using [rolldown](https://rolldown.rs). This means `import`/`require` of local files and `node_modules` just works:

```js
// app.js
import express from 'express';
import { handler } from './handler.js';

const app = express();
app.get('/', handler);
app.listen(3000);
```

```bash
nodesea app.js    # bundles express + handler.js into the binary
```

Node.js built-in modules (`fs`, `path`, `http`, etc.) are automatically treated as external.

Use `--no-bundle` to skip bundling and embed a single file as-is.

## sea-config.json

For advanced options, use a config file compatible with [Node.js SEA config format](https://nodejs.org/api/single-executable-applications.html):

```json
{
  "main": "app.js",
  "output": "myapp",
  "disableExperimentalSEAWarning": true,
  "useCodeCache": false,
  "assets": {
    "config.json": "config.json"
  },
  "execArgv": ["--experimental-vm-modules"]
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `main` | yes | Path to the JavaScript entry point |
| `output` | yes | Output executable path |
| `disableExperimentalSEAWarning` | no | Suppress the SEA experimental warning |
| `useSnapshot` | no | Main payload is a V8 snapshot |
| `useCodeCache` | no | Include V8 code cache |
| `assets` | no | Map of virtual name to file path |
| `execArgv` | no | Baked-in Node.js flags (Node 24.6+) |

## Why nodesea?

Node.js has built-in SEA support, but the official workflow requires multiple tools and manual steps. Other tools like `pkg` are abandoned. nodesea replaces the entire toolchain with a single command.

### Comparison

| | nodesea | Node.js built-in SEA | pkg |
|---|---|---|---|
| **Build command** | `nodesea app.js` | 5-step process (see below) | `pkg app.js` |
| **Requires Node.js at build time** | No | Yes | Yes |
| **Requires npm packages** | No | Yes (`postject`) | Yes (`pkg`) |
| **Bundling** | Built-in (rolldown) | None — manual step | Built-in |
| **Config file** | Optional | Required | Optional |
| **macOS code signing** | Automatic | Manual step | Automatic |
| **Implementation** | Pure Rust | Node.js + JS + C++ | Node.js |
| **Node.js 20–25 support** | Yes (auto-detected) | Version-specific | Stopped at Node 18 |
| **Maintained** | Yes | Yes | Abandoned |

### The official Node.js SEA workflow

The built-in approach requires running five separate commands and installing an npm package:

```bash
# 1. Write a JSON config
echo '{"main":"app.js","output":"sea-prep.blob"}' > sea-config.json

# 2. Generate the blob (requires node)
node --experimental-sea-config sea-config.json

# 3. Copy the node binary
cp $(which node) myapp

# 4. Inject the blob (requires npm install postject)
npx postject myapp NODE_SEA_BLOB sea-prep.blob \
    --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2

# 5. Re-sign on macOS
codesign --sign - --force myapp
```

### With nodesea

```bash
nodesea app.js
```

That's it. One command, no intermediate files, no npm, no manual signing.

### Key advantages

- **Single binary, zero runtime deps** — nodesea is a standalone Rust binary. Drop it into a CI pipeline or Docker build stage without installing Node.js or npm.
- **Built-in bundling** — imports and `node_modules` are resolved automatically via rolldown. No need to run esbuild/webpack as a separate step.
- **Cross-version** — automatically detects the Node.js version and selects the correct blob format (V1 or V2). Works with Node 20 through 25+.
- **Correct by default** — handles Mach-O segment layout, fuse flipping, ad-hoc code signing, and `__LINKEDIT` relocation without user intervention.

## How It Works

1. **Parse config** — read `sea-config.json` and validate
2. **Read source** — load the JavaScript file and any assets
3. **Detect version** — run `node --version` on the target binary, select blob format
4. **Serialize blob** — build the binary blob (magic `0x143da20`, flags, length-prefixed fields)
5. **Copy binary** — copy the Node.js binary to the output path
6. **Inject blob** — write the blob into the binary (Mach-O segment / ELF note / PE resource)
7. **Flip fuse** — change the SEA fuse sentinel from `:0` to `:1`
8. **Code sign** — ad-hoc re-sign on macOS (required for Apple Silicon)

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed technical documentation.

## Node.js Version Support

| Node.js | Blob Format | Status |
|---------|-------------|--------|
| 20.x | V1 (8-byte header) | Supported |
| 21.x | V1 (8-byte header) | Supported |
| 22.x | V1 (8-byte header) | Supported |
| 24.0–24.5 | V1 (8-byte header) | Supported |
| 24.6+ | V2 (9-byte header) | Supported |
| 25.x+ | V2 (9-byte header) | Supported |

## Platform Support

| Platform | Format | Status |
|----------|--------|--------|
| macOS (arm64, x86_64) | Mach-O | Implemented |
| Linux (x86_64, aarch64) | ELF | Implemented |
| Windows (x86_64) | PE | Planned |

## Building from Source

```bash
git clone https://github.com/user/nodesea.git
cd nodesea
cargo build --release
```

### Running Tests

```bash
# Unit tests (no Node.js required)
cargo test

# Integration tests (requires Node.js with SEA support in PATH)
cargo test -- --ignored
```

## License

MIT
