# PROGRESS.md — Folio (doc format)

**Updated:** 2026-04-11

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
- [x] AST → HTML
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
- [x] WGPU backend scaffold behind `wgpu-preview` (stub)
- [x] Inline layout v1: line builder over runs + mixed-style PDF/SVG
- [x] Typography v1: `letter-spacing`, `word-spacing`, basic `justify`
- [x] Pagination rules v2 (base): `keep-with-next`, `keep-together`, row split policy
- [x] Global deps foundation: multi-pass convergence guard + `counters` / `introspection`
- [x] Advanced layout foundation: min/max constraints, float (left/right), page header/footer
- [x] Export parity: capability matrix + integration smoke tests + cache regression test

---

## CLI — progress

- [x] `parse` — token dump
- [x] `validate` — syntax check
- [x] `convert` — json, text, html, pdf, svg
- [x] `render` — PDF via engine v2
- [x] `printers` / `print` — CUPS (`lp` / `lpr`) integration (Unix-oriented)

---

## Open questions

_(none tracked)_

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
- Folio = storage format; editors and alternate authoring layers are separate
