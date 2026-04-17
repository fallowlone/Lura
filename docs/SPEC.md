# Lura format specification (draft)

**Version:** 0.1-draft  
**Status:** working document; syntax and engine evolve together.

This file is the single place for normative behavior that is implemented or planned. Tooling and crate name: **`lura`** (see [PROGRESS.md](../PROGRESS.md)).

---

## Document model

- Semantic blocks: `PAGE`, `H1`–`H6`, `P`, `GRID`, `TABLE` / `ROW` / `CELL`, `FIGURE`, `CODE`, lists, etc.
- Each block may carry a stable **`id`** (explicit in source or assigned by `assign_ids` after parse).
- Variables in `STYLES` and attrs use `#name`; two-pass resolution in the parser.

---

## Heading outline counters

Headings `H1`–`H6` form an **outline** in document order (depth-first, following the styled tree roots and child order).

- Counters are **not** affected by pagination: only document structure matters.
- Rules (same as typical legal/technical outlines):
  - On each `Hn` (n = 1..6), increment counter at level n and **reset** deeper levels (n+1..6) to zero.
  - The **section label** joins non-zero counters from level 1 through n (zeros are omitted), e.g. `1`, `1.2`, `2.1.3`, and `2.1` when `H3` follows `H1` without an `H2` in between.

---

## Introspection placeholders (engine)

Placeholders are literal substrings in text or inline runs. They are expanded by the render engine **after** layout decisions that affect page breaks.

### Lexer note (important)

After `BLOCK(`, if the first non-space character is `{`, the parser treats it as the start of **attributes** `{ key: value }`, not as text. To start text with `{{sec}}` or `{{page:…}}`, escape the first brace with backslash (the lexer passes a literal `{` through):

- `H1(\{{sec}} Title)` not `H1({{sec}} Title)`
- `P(See \{{page:intro}}.)` not `P(See {{page:intro}}.)`

### `{{sec}}`

- **Meaning:** the outline label of the **current** heading block.
- **Where valid:** only inside content of an `H1`–`H6` block (including inline runs).
- **Elsewhere:** left unchanged (no substitution).

### `{{page:BLOCK_ID}}`

- **Meaning:** 1-based index of the **first** page on which that block’s laid-out content **starts** (when `place_node` begins for that block in the paginator).
- **BLOCK_ID:** must match the block’s stable `id` (same string as in the source or assigned by `assign_ids`). Allowed characters: letters, digits, `_`, `-`.
- **Unknown id:** replaced with `?`.
- **Convergence:** page numbers can change text width and reflow; the engine re-runs layout and pagination until the page map stabilizes or a pass limit is reached (see implementation).

---

## Render pipeline (informative)

1. Parse → resolve vars → assign ids.
2. Build styled tree.
3. Compute heading labels; substitute `{{sec}}` in headings.
4. Loop: layout → paginate → build block→page map; if `{{page:…}}` present and map changed, substitute and repeat.
5. Paint PDF / SVG.

**Layout measure:** Text and inline leaves are sized during the taffy pass via `compute_layout_with_measure`, using the same line-breaking helpers as pagination (`break_text` / `break_inline_runs`) and the content width implied by the parent (definite column width minus horizontal padding). Pagination then breaks again at the same inner width so grid/flex row heights and paint agree.

---

## Related documents

- [GRAPHICS_ROADMAP.md](GRAPHICS_ROADMAP.md) — extended graphics (fonts, opacity, clip, groups, masks).
- [PRODUCT_DECISIONS.md](PRODUCT_DECISIONS.md) — rename scope and graphics subset decisions.
