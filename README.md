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
- [x] macOS Lura app + Quick Look: native PDF preview (`PDFKit`) via FFI `lura_render_pdf` / `lura_free_pdf_result`

### Preview: why `.fol` can feel slower than `.pdf` in Quick Look

Opening a **`.fol`** file runs the full Lura pipeline (parse → resolver → layout → paginate → PDF bytes) in the preview path, then **PDFKit** displays that PDF. Opening an existing **`.pdf`** only decodes and draws bytes that were already rendered when the file was exported.

Large examples such as [`examples/showcase-large.fol`](examples/showcase-large.fol) intentionally stress the pipeline (multiple pages, tables, grids, long text), so Quick Look for that source file is expected to take longer than opening the pre-built [`examples/showcase-large.pdf`](examples/showcase-large.pdf).

The engine keeps a small **in-process** render cache in [`src/engine/mod.rs`](src/engine/mod.rs) (helps if the same document is rendered again without unloading the library). Quick Look often uses a **cold** process, so the first preview is usually a full run. Build with **`cargo build --release`** (as in [`install-preview.sh`](install-preview.sh)) so the bundled `liblura.dylib` is optimized; debug builds add overhead but do not change the fact that FOL preview does more work than opening a static PDF.

Faster path (see [`docs/PREVIEW_FIRST_OPEN.md`](docs/PREVIEW_FIRST_OPEN.md)):

1. **Shared disk cache** — SHA256-keyed PDF under an **App Group** (`group.com.fallowlone.lura`) so the editor and the QL extension share the same folder ([`quicklook/Shared/LuraPreviewDiskCache.swift`](quicklook/Shared/LuraPreviewDiskCache.swift)). The app **prewarms** the cache after each successful debounced preview, so Finder can open cold-fast after the document was previewed in Lura.

A previous sidecar `yourdoc.fol.preview.pdf` path was removed: reading a sibling file from the QL extension trips the macOS 15 "data from other apps" TCC prompt. The App Group disk cache is the supported warm path.

If the cache does not apply, preview still runs the full pipeline once.

Relevant code: [`quicklook/Extension/PreviewViewController.swift`](quicklook/Extension/PreviewViewController.swift), [`src/lib.rs`](src/lib.rs) (`lura_render_pdf`), [`src/engine/mod.rs`](src/engine/mod.rs) (`render`).

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
cargo run -- convert examples/sample.fol --format pdf --output out.pdf
cargo run -- convert examples/sample.fol --format pdf --output out.pdf --preview-sidecar
```

## License

Apache 2.0
