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

## Using a Config File

```bash
cargo run -- --config examples/sea-config.json
./myapp
```
