# Examples

## hello.js — Minimal

```bash
cargo run -- examples/hello.js
./hello
# Hello from SEA!
```

## greeting.js — CLI Arguments

```bash
cargo run -- examples/greeting.js
./greeting Alice
# Hello, Alice!
# Node.js v22.12.0
# Platform: darwin arm64
```

## http-server.js — HTTP Server

```bash
cargo run -- examples/http-server.js
./http-server
# Server running at http://localhost:3000/

# In another terminal:
curl http://localhost:3000/
# Hello from SEA!
# Path: /
```

## task-runner/ — Multi-Module Bundling

A task runner with a complex module graph demonstrating automatic bundling of
multiple local files. All `require()` calls are resolved and bundled into a
single file by rolldown.

```
main.js
├── lib/runner.js    — DAG-based task executor
│   └── lib/logger.js
├── lib/task.js      — single task with timeout
│   └── lib/logger.js  (shared)
├── lib/utils.js     — env info, hashing, table formatting
│   ├── node:path
│   ├── node:os
│   └── node:crypto
└── lib/logger.js    (shared)
```

```bash
nodesea examples/task-runner/main.js
./main
# 11:39:13 [main] task-runner starting
# 11:39:13 [runner] running 1 task(s): report
# 11:39:13 [gather] starting
# 11:39:13 [gather] darwin/arm64, 16 cpus, 65536MB RAM
# ...
# 11:39:13 [runner] all tasks complete (2.4ms)
```

## cli/ — TypeScript CLI with Subcommands

A multi-command CLI tool written entirely in TypeScript. Demonstrates that
nodesea handles `.ts` files natively via rolldown — no `tsc`, no `tsconfig.json`,
no build step.

```
main.ts
├── src/args.ts              — argument parser (zero deps)
├── src/colors.ts            — ANSI color helpers (shared)
├── src/commands/info.ts     — system info subcommand
├── src/commands/hash.ts     — file checksum subcommand
└── src/commands/serve.ts    — static file server subcommand
```

```bash
nodesea examples/cli/main.ts -o mycli
```

```bash
./mycli --help
# mycli v1.0.0 — a demo CLI built with nodesea
# ...

./mycli info
#   Node.js  v22.16.0
#  Platform  darwin arm64
#      CPUs  16x Apple M4 Max
#    Memory  64.0 GB
#       ...

./mycli hash package.json src/lib.rs --algo sha1
#   da39a3ee5e6b...  package.json (1.2 KB)
#   a1b2c3d4e5f6...  src/lib.rs (256 B)

./mycli serve ./public --port 3000
#   Root  /Users/you/public
#   URL   http://localhost:3000/
```

## Using a Config File

```bash
cargo run -- --config examples/sea-config.json
./myapp
```
