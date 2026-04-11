# PROGRESS.md — Lura (document format)

**Updated:** 2026-04-12

---

## Branding and files (plan)

- **Public format name:** **Lura**.
- **Document file extension:** **`.lura`** (examples may still use `.fol` until bulk rename).
- **Rust crate / CLI binary:** **`lura`** (`cargo build` produces `liblura.dylib` and the `lura` executable). GitHub repo folder may still be named `Folio` locally; product name in docs is Lura.

---

## Graphics and compositing (roadmap)

Goal: masks, transparency groups, non-standard fonts, richer graphics stack (PDF-like model discussion).

**Phased plan A–E:** [docs/GRAPHICS_ROADMAP.md](docs/GRAPHICS_ROADMAP.md)  
(fonts → simple opacity → clip → groups + blend modes → SMask / complex masks; separately — IR growth beyond flat `DrawCommand`-only paths.)

---

## Current phase: Render Engine v2 (Rust)

**Status:** active development

---

## Stack

- Language: Rust
- Workflow: AI-assisted implementation; Artem owns architecture and review

---

## Format specification — progress

- [x] Core syntax sketched on paper
- [x] Core block types: heading, paragraph, table, **figure**, code (`H1`–`H6`, `P`, `TABLE`/`ROW`/`CELL`, `FIGURE`/`IMAGE`, `CODE`; PDF: placeholder for empty asset); full spec vs vision still partial in places
- [x] Layout: grid-based (not coordinate-based)
- [x] Block ID scheme
- [x] Certificate scheme (design level)
- [x] Asset handling: inline (base64) vs linked (hash)

### Syntax (draft)

```
STYLES({
  #mainColor: #FF0000
  #mainFont: "Arial"
})

PAGE(
  STYLES({
    #bgColor: #FFFFFF
  })

  H1({color: #mainColor} Hello World)

  P(Paragraph text.)

  GRID({columns: "1fr 2fr"}
    P(Left column)
    P(Right column)
  )
)
```

Rules:

- Block: `TYPE({attrs} content)` or `TYPE(content)`
- Attributes optional
- `STYLES` is always the first block (document and page)
- Variables: `#name`, resolved globally (two parser passes)
- Grid `columns`: fixed lengths, `fr`, `auto`

---

## Renderer — progress

- [x] AST → JSON
- [x] AST → plain text
- [x] AST → plain text: `CELL` with `Content::Children` renders via `render_children` (nested block text), not only `extract_text`
- [x] ~~AST → HTML~~ removed; preview and tooling use the PDF/SVG engine pipeline only
- [x] Engine v2: StyledTree → LayoutTree → PageTree → PDF (`pdf-writer`)
- [x] Engine v2: SVG export
- [x] Legacy PDF path (`printpdf`) removed

---

## Lexer — progress

- [x] Token set defined
- [x] Mode-based lexer (Normal / Attrs / Content)
- [x] Tests for token kinds

---

## Parser — progress

- [x] AST types (`Document`, `Block`, `Content`, `Value`)
- [x] Parser: tokens → AST
- [x] Variables: `#var` substitution in attrs (two passes)
- [x] Tests
- [x] Arena AST (`NodeId`, `Content::Children`)
- [x] `id::assign_ids` post-order by ID
- [x] `Document` API for external modules (no raw arena fields)
- [x] Inline AST v1: `TextRun`, `Emphasis`, `Strong`, `CodeSpan`, `LinkSpan`

---

## Engine v2 — progress

- [x] Data-oriented styled arena (`id-arena`)
- [x] `taffy` layout (Grid/Flex)
- [x] Pagination `LayoutTree → PageTree` (A4)
- [x] `unicode-linebreak` for breaks
- [x] `fontdb` for system fonts
- [x] `rustybuzz` shaping for measurement
- [x] PDF backend (`pdf-writer`, built-in Type1 + WinAnsi; TrueType/ToUnicode = future)
- [x] Painter API skeleton
- [x] WGPU backend scaffold behind `wgpu-preview` (stub; optional deps include `wgpu` 28 + `glyphon`)
- [x] Inline layout v1: line builder over runs + mixed-style PDF/SVG
- [x] Typography v1: `letter-spacing`, `word-spacing`, basic `justify`
- [x] Pagination rules v2 (base): `keep-with-next`, `keep-together`, row split policy
- [x] Global deps foundation: multi-pass convergence guard
- [x] Heading counters and page introspection (outline `H1`–`H6`, `{{sec}}`, `{{page:id}}`, multi-pass pagination; see [docs/SPEC.md](docs/SPEC.md))
- [x] Advanced layout foundation: min/max constraints, float (left/right), page header/footer
- [x] Export parity: capability matrix + integration smoke tests + cache regression test

---

## CLI — progress

- [x] `parse` — token dump
- [x] `validate` — syntax check
- [x] `convert` — json, text, pdf, svg
- [x] `render` — PDF via engine v2
- [x] `printers` / `print` — CUPS (`lp` / `lpr`) integration (Unix-oriented)

---

## Open questions

- Rename scope: product/spec only vs also `Cargo.toml`/crate name and GitHub repo (logged in [docs/PRODUCT_DECISIONS.md](docs/PRODUCT_DECISIONS.md)).
- «Lura Graphics 1.0» subset for the first graphics release (see GRAPHICS_ROADMAP; baseline A+B+clip logged in PRODUCT_DECISIONS).

---

## Decisions locked in

- Implementation language: Rust
- Semantic blocks, not visual coordinates
- Diff-friendly stable block IDs
- Assets: self-contained (base64) vs linked (external + hash)
- Verification without central CA (self-contained)
- Storage syntax: human-readable text (not JSON/YAML)
- Sparse layout: absolute units (mm) in authored model; engine uses pt
- Certificate: SHA-256 over document (design)
- Lura = storage format; editors and alternate authoring layers are separate
