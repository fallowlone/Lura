# Lura

A new document format to replace PDF.

## Why

PDF was designed for print in 1993. It stores visual coordinates, not meaning.
Lura stores semantic structure — headings, paragraphs, tables — not pixel positions.

|                | PDF    | Lura            |
| -------------- | ------ | --------------- |
| Human-readable | No     | Yes             |
| Git-friendly   | No     | Yes             |
| File size      | Large  | 10–100x smaller |
| Accessible     | Manual | Built-in        |
| Diffable       | No     | Yes             |

## Format

A `.fol` file is plain text. Any editor works.

```
STYLES({
  #brand: #0000FF
  #font: "Inter"
})

PAGE(
  H1({color: #brand} Hello World)
  P(This is a paragraph.)
  GRID({columns: "1fr 2fr"}
    P(Left column)
    P(Right column)
  )
)
```

**Blocks (examples):** `H1`–`H6`, `P`, `PAGE`, `GRID`, `TABLE` / `ROW` / `CELL`, `LIST` / `ITEM`, `IMAGE`, `CODE`, `QUOTE`, `HR`, `STYLES`

**Attributes:** `{key: value}` — optional, before content

**Variables:** `#name` — defined in `STYLES`, resolved globally

## Status

Early development. The pipeline works for experimentation; the **written specification**, **diff tooling**, and **full vision coverage** (figures, certificates, rich PDF fonts) are not done.

### Done (high level)

- [x] Lexer and parser (arena AST, variables, inline spans, stable IDs)
- [x] Exports: JSON, plain text; binary via PDF and SVG (same layout pipeline)
- [x] Layout engine v2: taffy → A4 pagination → PDF (`pdf-writer`) and SVG
- [x] CLI: `parse`, `validate`, `convert` (json | text | pdf | svg), `render`, `printers`, `print` (CUPS on Unix)
- [x] macOS Lura app + Quick Look: native PDF preview (`PDFKit`) via FFI `lura_render_pdf` / `lura_free_pdf_result` (no HTML/CSS)

### Not done / partial

- [ ] Published format specification (normative doc)
- [ ] `diff` (and merge helpers) on documents or block IDs
- [ ] Certificate generation and verification in tooling
- [ ] PDF: embedded TrueType + full Unicode (current path uses built-in Type1 + WinAnsi)
- [ ] WGPU live preview (feature flag exists; implementation is a stub)
- [ ] All semantic block types from the long-term vision (e.g. dedicated figure flow)

See **`PROGRESS.md`** for a detailed checklist and **`HANDOFF.md`** for the latest session handoff.

## Build

Requires [Rust](https://rustup.rs).

```sh
cargo build
cargo test
cargo run -- --help
```

### Example

```sh
cargo run -- convert --file examples/sample.fol --format pdf --output out.pdf
```

## License

Apache 2.0
