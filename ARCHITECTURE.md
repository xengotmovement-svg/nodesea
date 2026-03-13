# Architecture

## Overview

A Node.js Single Executable Application (SEA) is a standalone binary that bundles
JavaScript source code inside a stock Node.js executable. At startup, the Node.js
runtime detects the embedded payload and runs it instead of entering the normal REPL
or script-execution path.

**nodesea** is a pure Rust tool that creates these SEA binaries *without requiring
Node.js at build time*. The official Node.js workflow (`node --experimental-sea-config`)
needs a working Node.js installation to generate the SEA blob. nodesea replaces that
entire step: it serializes the blob itself, injects it into a copy of the Node binary,
flips the activation fuse, and re-signs on macOS -- all from a single native executable.

## SEA Blob Format

The SEA blob is the binary payload that the Node.js runtime deserializes at startup
via its internal `BlobDeserializer`. Two format versions exist, selected based on the
target Node.js version.

### Header

| Version | Node.js range | Header size | Layout |
|---------|---------------|-------------|--------|
| V1      | 20.0 -- 24.5  | 8 bytes     | `magic(u32 LE) + flags(u32 LE)` |
| V2      | 24.6+         | 9 bytes     | `magic(u32 LE) + flags(u32 LE) + exec_argv_extension(u8)` |

The magic number is `0x143da20` (little-endian u32), defined in `node_sea.h` in the
Node.js source tree.

### Flags

The flags field is a u32 bitfield:

| Bit | Name                              | Meaning |
|-----|-----------------------------------|---------|
| 0   | `DISABLE_EXPERIMENTAL_SEA_WARNING`| Suppress the "ExperimentalWarning" message |
| 1   | `USE_SNAPSHOT`                    | Main payload is a V8 heap snapshot, not JS source |
| 2   | `USE_CODE_CACHE`                  | A V8 code cache follows the main payload |
| 3   | `INCLUDE_ASSETS`                  | An assets map is present (Node 21+) |
| 4   | `INCLUDE_EXEC_ARGV`              | An exec_argv list is present (V2 only, Node 24.6+) |

### V2 `exec_argv_extension` byte

The ninth header byte in V2 controls how exec argv is extended at runtime:

| Value | Name  | Meaning |
|-------|-------|---------|
| 0     | None  | No extension |
| 1     | Env   | Extend via environment variable |
| 2     | Cli   | Extend via CLI arguments |

### Body

After the header, the body consists of length-prefixed fields. Each field uses
Node's "StringView" encoding: a **u64 little-endian** length prefix followed by
that many raw bytes.

Fields appear in this fixed order (some are conditional on flags):

1. **code_path** -- Virtual path for the embedded script (e.g. `/sea/main.js`).
2. **main_code** -- JavaScript source bytes (or V8 snapshot if `USE_SNAPSHOT`).
3. **code_cache** -- V8 code cache bytes. Only present when `USE_CODE_CACHE` is set.
4. **assets** -- Asset map. Only present when `INCLUDE_ASSETS` is set. Encoded as a
   u64 LE count followed by that many (key, value) pairs, each length-prefixed.
5. **exec_argv** -- Argument list. V2 only, present when `INCLUDE_EXEC_ARGV` is set.
   Encoded as a u64 LE count followed by that many length-prefixed strings.

## Fuse Mechanism

Every Node.js binary contains an embedded sentinel string:

```
NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2:0
```

The trailing `:0` means "no SEA payload -- run as normal Node.js". After blob
injection, nodesea scans the binary for this sentinel and flips the last byte
from `0` to `1`:

```
NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2:1
```

This tells the Node.js runtime at startup to look for the injected SEA blob
instead of entering normal execution. The fuse is a one-way switch -- once
flipped, the binary is committed to SEA mode.

The scanner (`fuse.rs`) performs a brute-force byte search for the sentinel
prefix and validates the state byte is either `0` or `1`.

## Platform Injection

The SEA blob must be placed where the Node.js runtime knows to look for it.
Each executable format uses a different mechanism.

### Mach-O (macOS)

The blob is injected as a new section in a new load command:

- **Segment name:** `NODE_SEA`
- **Section name:** `__NODE_SEA_BLOB`
- **Load command type:** `LC_SEGMENT_64`
- **Alignment:** Page-aligned (4096 bytes on arm64, 4096 on x86_64)

The injection adds an `LC_SEGMENT_64` load command to the Mach-O header and
appends the blob data at the end of the binary, padded to page boundaries.
This requires sufficient slack space in the header for the new load command
(112 bytes for the segment command plus 80 bytes for the section).

### ELF (Linux)

The blob is injected as a `PT_NOTE` program header entry:

- **Note name:** `NODE_SEA_BLOB`
- **Note type:** `NT_GNU_BUILD_ID` (typically 3)

The injection appends a properly aligned ELF note to the binary and adds
(or repurposes) a `PT_NOTE` entry in the program header table pointing to it.

### PE (Windows)

The blob is injected as a Win32 resource:

- **Resource type:** `RT_RCDATA`
- **Resource name:** `NODE_SEA_BLOB`

This uses the Windows resource table mechanism to embed arbitrary binary data
that the Node.js runtime reads via `FindResource`/`LoadResource` at startup.

## Build Pipeline

The end-to-end build process follows this sequence:

```
sea-config.json
      |
      v
 1. Config parse         Parse the JSON config for main script path,
                         output path, flags, assets, and exec_argv.
      |
      v
 2. JS read              Read the JavaScript source file (and any assets)
                         from disk into memory.
      |
      v
 3. Version detect       Run `node --version` on the target binary, parse
                         the semver output, and select V1 or V2 blob format.
      |
      v
 4. Blob serialize       Build the binary blob: write header (magic, flags,
                         and exec_argv_extension for V2), then body fields
                         in order with u64 LE length prefixes.
      |
      v
 5. Copy binary          Copy the Node.js binary to the output path. All
                         subsequent mutations happen on the copy.
      |
      v
 6. Inject               Inject the blob into the copied binary using the
                         platform-appropriate method (Mach-O / ELF / PE).
      |
      v
 7. Fuse flip            Scan the binary for the fuse sentinel and flip
                         the state byte from :0 to :1.
      |
      v
 8. Codesign             On macOS, run `codesign --remove-signature` then
                         `codesign --sign -` for ad-hoc re-signing. The
                         original signature is invalid after injection.
```

### Source layout

```
src/
  lib.rs            -- Public API, module re-exports
  main.rs           -- CLI entry point
  error.rs          -- Error types (thiserror)
  config.rs         -- sea-config.json parsing
  version.rs        -- Node.js version detection, V1/V2 selection
  fuse.rs           -- Fuse sentinel scanner and flipper
  codesign.rs       -- macOS ad-hoc code signing
  blob/
    mod.rs          -- Blob types, flags, serialize() dispatcher
    v1.rs           -- V1 serializer (Node 20--24.5)
    v2.rs           -- V2 serializer (Node 24.6+)
  inject/
    mod.rs          -- Injection module re-exports
    macho.rs        -- Mach-O injection (macOS)
    elf.rs          -- ELF injection (Linux)
    pe.rs           -- PE injection (Windows)
```
