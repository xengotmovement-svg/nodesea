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

## Using a Config File

```bash
cargo run -- --config examples/sea-config.json
./myapp
```
