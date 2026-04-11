# CLI Design — Lura

**Date:** 2026-04-09  
**Status:** Approved

## Overview

Add a CLI layer to the Lura tool. Users pass `.fol` / `.lura` files as arguments and choose an operation. The existing lexer/parser/renderer pipeline stays unchanged — CLI is a thin dispatch layer on top.

## Commands

```
lura parse <file>            # tokenize and print tokens (debug use)
lura validate <file>         # parse and report validity, exit 0 on success / 1 on error
lura convert <file>          # parse and render to stdout (default: JSON)
  --format <json|text>        # output format, default: json
  --output <path>             # write result to file instead of stdout
```

`diff` command is deferred — not in scope for this phase.

## Architecture

```
main.rs
  └── cli.rs         (clap structs: Cli, Commands enum)
        ├── parse    → lexer::Lexer → print tokens
        ├── validate → lexer + parser → print "✓ valid" or error, exit code
        └── convert  → lexer + parser + resolver + renderer → stdout or file
```

`cli.rs` owns argument definitions. `main.rs` matches on the parsed command and dispatches. No business logic lives in CLI code.

## Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
```

## Error Handling

- File not found: print error to stderr, exit 1
- Parse error: print error to stderr, exit 1
- `validate` uses exit code as the machine-readable result (0 = valid, 1 = invalid)

## Output

- Default: stdout
- `--output <path>`: write to file, print nothing to stdout on success

## Out of Scope

- `diff` command
- Config files
- Multiple input files
- Streaming/watch mode
