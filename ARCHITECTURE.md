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
- **Alignment:** Page-aligned (16384 bytes on arm64, 4096 on x86_64)

Mach-O injection is the most involved of the three formats because macOS
`codesign` enforces strict layout constraints. The algorithm:

```
Original layout:           After injection:

┌──────────────┐           ┌──────────────┐
│  mach_header │           │  mach_header │  ncmds += 1, sizeofcmds += 152
│  + load cmds │           │  + load cmds │
│              │           │  + NODE_SEA  │  ← inserted BEFORE __LINKEDIT
│              │           │  + __LINKEDIT│  ← shifted forward
├──────────────┤           ├──────────────┤
│  __TEXT data  │           │  __TEXT data  │  unchanged
├──────────────┤           ├──────────────┤
│  __LINKEDIT  │           │  NODE_SEA    │  ← blob data (page-aligned)
│  (symtab,    │           │  blob data   │
│   exports,   │           ├──────────────┤
│   codesig)   │           │  __LINKEDIT  │  ← relocated after blob
└──────────────┘           │  (symtab,    │
                           │   exports)   │  codesig removed
                           └──────────────┘
```

**Step-by-step:**

1. **Remove `LC_CODE_SIGNATURE`** — zero out the load command, shift
   subsequent commands down, decrement `ncmds`/`sizeofcmds`. Truncate the
   file at the code signature data offset so the signature bytes are removed.

2. **Shrink `__LINKEDIT`** — update its `filesize` and `vmsize` to reflect
   the removal of the signature data.

3. **Save and relocate `__LINKEDIT`** — copy the `__LINKEDIT` data out of the
   binary, truncate the file to just before where `__LINKEDIT` started, then:
   - Page-align and append the blob data.
   - Page-align and re-append `__LINKEDIT` data after the blob.

4. **Update `__LINKEDIT` fields** — set new `fileoff`, `filesize`, `vmsize`,
   and `vmaddr` to reflect its new position.

5. **Fix up offset references** — any load command that stores absolute file
   offsets pointing into `__LINKEDIT` must be adjusted by the relocation
   delta. This includes `LC_SYMTAB` (`symoff`, `stroff`), `LC_DYSYMTAB`
   (six offset fields), `LC_DYLD_INFO`/`LC_DYLD_INFO_ONLY` (five offset
   fields), `LC_FUNCTION_STARTS`, `LC_DATA_IN_CODE`,
   `LC_DYLD_EXPORTS_TRIE`, and `LC_DYLD_CHAINED_FIXUPS`.

6. **Check header space** — the new `LC_SEGMENT_64` + `Section64` is 152
   bytes. The injector scans all sections to find the earliest data offset
   in the file and verifies there is room between the end of the current load
   commands and that first section.

7. **Insert `NODE_SEA` load command** — the new segment is written *before*
   `__LINKEDIT`'s load command (not appended at the end). `__LINKEDIT` and
   all subsequent load commands are shifted forward by 152 bytes. This
   ensures `__LINKEDIT` remains the last `LC_SEGMENT_64` in the load command
   table — a hard requirement for `codesign`.

8. **Update `mach_header_64`** — increment `ncmds` and `sizeofcmds`.

**Why `__LINKEDIT` must be last:**

macOS code signing appends the code signature as the final data in
`__LINKEDIT`. The `codesign` tool expects `__LINKEDIT` to be the last
segment in both the load command table and the file layout. If any segment
data follows `__LINKEDIT`, signing fails with "main executable failed strict
validation". This constraint drives the entire relocation strategy above.

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
 nodesea app.js  (or --config sea-config.json)
      |
      v
 1. Config resolve       If a positional script is given, synthesize a
                         config with sensible defaults (output name from
                         file stem, warnings suppressed). Otherwise parse
                         the JSON config file.
      |
      v
 2. Version detect       Run `node --version` on the target binary, parse
                         the semver output, and select V1 or V2 blob format.
      |
      v
 3. Bundle / read        By default, bundle the entry point and all its
                         imports into a single CJS file using rolldown
                         (in-process, no Node.js needed). Node.js built-in
                         modules are kept as external. With --no-bundle,
                         the script is read as-is.
      |
      v
 4. Read assets          Load any files declared in the config's `assets`
                         map into memory.
      |
      v
 5. Blob serialize       Build the binary blob: write header (magic, flags,
                         and exec_argv_extension for V2), then body fields
                         in order with u64 LE length prefixes.
      |
      v
 6. Copy binary          Copy the Node.js binary to the output path. All
                         subsequent mutations happen on the copy.
      |
      v
 7. Inject               Inject the blob into the copied binary using the
                         platform-appropriate method (Mach-O / ELF / PE).
                         On Mach-O this includes removing the existing code
                         signature and relocating __LINKEDIT.
      |
      v
 8. Fuse flip            Scan the binary for the fuse sentinel and flip
                         the state byte from :0 to :1.
      |
      v
 9. Codesign             On macOS, run `codesign --sign - --force` for
                         ad-hoc re-signing. Required on Apple Silicon
                         because the original signature was removed during
                         injection.
```

### Bundling

When bundling is enabled (the default), nodesea uses [rolldown](https://rolldown.rs)
as an in-process Rust library — no Node.js subprocess is spawned. The bundler is
configured with `platform: node` and `format: cjs`:

- Local imports (`./lib/utils.js`, `../shared.js`) are resolved and inlined.
- `node_modules` dependencies are resolved and inlined.
- Node.js built-in modules (`fs`, `path`, `http`, `crypto`, etc.) are kept
  external — they are provided by the Node.js runtime at execution time.
- The output is a single self-contained CommonJS file suitable for embedding.

This means `nodesea app.js` handles a project with a complex module graph the
same way as a single file — no separate bundling step is needed.

### Source layout

```
src/
  lib.rs            -- Public API, module re-exports
  main.rs           -- CLI entry point (clap)
  error.rs          -- Error types (thiserror)
  config.rs         -- sea-config.json parsing and validation
  version.rs        -- Node.js version detection, V1/V2 selection
  bundle.rs         -- JavaScript bundling via rolldown
  fuse.rs           -- Fuse sentinel scanner and flipper
  codesign.rs       -- macOS ad-hoc code signing
  blob/
    mod.rs          -- Blob types, flags, serialize() dispatcher
    v1.rs           -- V1 serializer (Node 20--24.5)
    v2.rs           -- V2 serializer (Node 24.6+)
  inject/
    mod.rs          -- Format detection, Injector trait, dispatcher
    macho.rs        -- Mach-O injection with __LINKEDIT relocation
    elf.rs          -- ELF PT_NOTE injection
    pe.rs           -- PE RCDATA injection (planned)
```
