# Handoff — Folio

**Date:** 2026-04-11

## What changed this session

- All Russian doc comments in Rust sources were translated to English (engine, parser, renderer tests).
- `PROGRESS.md` rewritten in English and extended with an explicit **CLI** section.
- `README.md` status aligned with the repo (CLI and PDF/SVG are implemented; spec and polish remain).
- This file added for session continuity.

## Current snapshot

| Area | State |
|------|--------|
| Lexer / Parser | Solid base; variables, arena AST, inline spans, block IDs |
| Engine v2 | taffy layout → paginate → Painter → PDF (`pdf-writer`) / SVG; render cache |
| Exports | JSON, text, HTML, PDF, SVG via `folio convert` / `render` |
| CLI | clap subcommands: parse, validate, convert, render, printers, print |
| Gaps vs vision | Formal spec doc, `diff`/`validate` depth, certificate tooling, figure block, PDF Unicode beyond WinAnsi (TrueType path), real WGPU preview |

## Suggested next work (priority-agnostic)

1. **Specification** — single `SPEC.md` or similar derived from `PROGRESS` + examples; version the syntax.
2. **PDF typography** — embed TrueType + ToUnicode (noted as v3 in `pdf.rs` comments).
3. **Diff** — `folio diff` on AST or stable IDs (git-friendly story from CLAUDE.md).
4. **Block coverage** — `FIGURE` (and any missing semantic blocks) end-to-end parser → layout → export.
5. **WGPU** — replace stub behind `wgpu-preview` when preview UX is defined.
6. **CI** — `cargo test` + `cargo clippy` on push if not already elsewhere.

## How to verify locally

```sh
cargo build
cargo test
cargo run -- --help
```

## Note for the next agent

- Project rules live in `CLAUDE.md` (vision + phase plan). `PROGRESS.md` is the detailed checklist.
- User prefers Russian for explanations in chat; **code and repo docs** are English unless asked otherwise.
