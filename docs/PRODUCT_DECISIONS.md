# Product decisions (logged)

**Updated:** 2026-04-12

Decisions called out as open in [PROGRESS.md](../PROGRESS.md) are recorded here so implementation can proceed without blocking on a full rename or graphics epic.

---

## Lura product name and crate

**Status:** aligned (2026-04-12).

- **Format / product name:** **Lura**; **CLI and Rust library crate:** **`lura`** (`lura` binary, `liblura.dylib` for macOS FFI).
- **Git repo / workspace folder** may still be `Folio` on disk until a separate GitHub rename.

---

## Block IDs

**Status:** decided (2026-10-23).

- Automatic block IDs stay a **content hash** (FNV-1a of kind, attributes and content; children by their IDs). The ID doubles as the per-block integrity value.
- Consequence: editing a block changes its ID. Anything that must survive edits (cross references, `{{page:id}}`, anchors, `diff` targets) uses an **explicit ID**: `P[intro](...)`.
- Identical blocks get `-2`, `-3`, ... suffixes in document order; an auto ID never takes an explicit one. `lura validate` fails on a repeated explicit ID.
- Positional IDs were considered and rejected: they break on insert and reorder, and cannot verify integrity.

---

## Lura Graphics 1.0 subset

**Status:** agreed baseline for the first “graphics release” epic (before full PDF graphics parity).

**Subset (from [GRAPHICS_ROADMAP.md](GRAPHICS_ROADMAP.md)):**

- **Phase A:** Non-standard fonts (asset bytes, embed in PDF / `@font-face` in SVG / same bytes for preview).
- **Phase B:** Simple scalar `opacity` on blocks (no isolated groups / blend modes yet).
- **Phase C (MVP):** Rectangular clip on containers only.

Phases D (groups + blend) and E (SMask / complex masks) are **out of scope** for Graphics 1.0.

---

## Backlog alignment (HANDOFF)

Tracked for upcoming work, not part of the counters/introspection milestone:

| Item                        | Notes                                                                         |
| --------------------------- | ----------------------------------------------------------------------------- |
| `lura diff`                 | AST or stable-id diff for git-friendly workflows ([CLAUDE.md](../CLAUDE.md)). |
| FIGURE end-to-end           | Parser → layout → export beyond placeholder when assets are wired.            |
| CI                          | `cargo test` and `cargo clippy` on push (add workflow if missing).            |
| WGPU preview                | Replace stub behind `wgpu-preview` when preview UX is defined.                |
| PDF fallback faces          | CJK/emoji (Liberation covers Latin, Cyrillic, Greek); overlaps Graphics phase A. |
