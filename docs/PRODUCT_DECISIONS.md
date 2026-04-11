# Product decisions (logged)

**Updated:** 2026-04-12

Decisions called out as open in [PROGRESS.md](../PROGRESS.md) are recorded here so implementation can proceed without blocking on a full rename or graphics epic.

---

## Lura product name and crate

**Status:** aligned (2026-04-12).

- **Format / product name:** **Lura**; **CLI and Rust library crate:** **`lura`** (`lura` binary, `liblura.dylib` for macOS FFI).
- **Git repo / workspace folder** may still be `Folio` on disk until a separate GitHub rename.

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

| Item | Notes |
|------|--------|
| `lura diff` | AST or stable-id diff for git-friendly workflows ([CLAUDE.md](../CLAUDE.md)). |
| FIGURE end-to-end | Parser → layout → export beyond placeholder when assets are wired. |
| CI | `cargo test` and `cargo clippy` on push (add workflow if missing). |
| WGPU preview | Replace stub behind `wgpu-preview` when preview UX is defined. |
| TrueType + ToUnicode in PDF | Unicode beyond WinAnsi; overlaps Graphics phase A. |
