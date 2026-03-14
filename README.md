# nodesea

Pure Rust Node.js Single Executable Application (SEA) builder.

Takes a JavaScript source file and a Node.js binary, produces a standalone executable — **without requiring Node.js at build time**.

## Features

- **Built-in bundling** — automatically bundles imports and `node_modules` via [rolldown](https://rolldown.rs)
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
| `--node <path>` | Path to Node.js binary (default: `node` in PATH) |
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
