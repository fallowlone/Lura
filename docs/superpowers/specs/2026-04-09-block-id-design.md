# Block ID Design

**Date:** 2026-04-09  
**Status:** Approved

## Overview

Every block in a Lura document gets a stable, unique ID. IDs are used for diffing and referencing blocks across document versions. ID appears only in JSON output.

## Syntax

Optional explicit ID in square brackets immediately after the block name:

```
H1[intro]({color: #accent} Hello from Lura)
P(No explicit ID — auto-generated.)
P[second-para](Explicit ID.)
```

If `[id]` is absent, the ID is computed automatically. Nested blocks each get their own ID independently.

## ID Generation

**Algorithm: FNV-1a 64-bit, implemented inline (no external dependency)**

Input to hash: `kind + "|" + sorted_attrs_string + "|" + content_string`

- `kind`: block type, e.g. `"H1"`
- `sorted_attrs_string`: attrs serialized as `"key=value"` pairs sorted by key, joined with `,`
- `content_string`: for `Text(s)` — the string `s`; for `Blocks(...)` — concatenation of child IDs; for `Empty` — `""`

Output: lowercase hex string of the 64-bit hash, e.g. `"3f9c2a1b5e7d04a2"` (16 chars).

Auto-generated ID format: `{kind_lowercase}_{hash8}` — first 8 hex chars of the hash.  
Example: `h1_3f9c2a1b`

Explicit ID: used as-is, no prefix.  
Example: `intro`

## Architecture

```
Lexer
  └── new tokens: LBracket, RBracket (for [ and ])

Parser
  └── after reading Ident, peeks for LBracket → reads id string → RBracket
      if [id] present: sets block.id = explicit id string
      if [id] absent:  sets block.id = "" (empty, to be filled by ID resolver)

AST
  └── Block gains one field: id: String
      (empty string = "not yet assigned", filled by ID resolver)

ID Resolver (new module: src/parser/id.rs)
  └── walks AST bottom-up: children get IDs before parent
      if block.id is non-empty → keep (explicit)
      if block.id is empty → compute FNV-1a hash and assign
      hash input for parent uses already-assigned child IDs

JSON Renderer
  └── includes "id" field for every block
```

## Code Changes

| File                   | Change                                                          |
| ---------------------- | --------------------------------------------------------------- |
| `src/lexer/token.rs`   | Add `LBracket`, `RBracket` tokens                               |
| `src/lexer/mod.rs`     | Lex `[` and `]` characters                                      |
| `src/parser/ast.rs`    | Add `id: String` to `Block`                                     |
| `src/parser/mod.rs`    | Parse optional `[id]` after block name, set `block.id` directly |
| `src/parser/id.rs`     | New: FNV-1a impl + walk AST and assign IDs                      |
| `src/parser/mod.rs`    | Export `id` module                                              |
| `src/renderer/json.rs` | Include `"id"` in block output                                  |
| `src/main.rs`          | Call `id::assign_ids(&mut doc)` after resolver                  |

## JSON Output

```json
{
  "vars": {},
  "blocks": [
    {
      "kind": "PAGE",
      "id": "page_a1b2c3d4",
      "attrs": {},
      "content": {
        "blocks": [
          {
            "kind": "H1",
            "id": "intro",
            "attrs": { "color": "#3498DB" },
            "content": "Hello from Lura"
          },
          {
            "kind": "P",
            "id": "p_9f3e1a2b",
            "attrs": {},
            "content": "This is a sample document."
          }
        ]
      }
    }
  ]
}
```

## Out of Scope

- IDs in plain text renderer
- IDs in `lura parse` token output
- ID uniqueness validation across a document
- ID stability guarantees when content changes (content hash by design)
