# Folio

A new document format to replace PDF.

## Why

PDF was designed for print in 1993. It stores visual coordinates, not meaning.
Folio stores semantic structure — headings, paragraphs, tables — not pixel positions.

|                | PDF    | Folio           |
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

**Blocks:** `H1`, `H2`, `H3`, `P`, `PAGE`, `GRID`, `IMAGE`, `STYLES`

**Attributes:** `{key: value}` — optional, before content

**Variables:** `#name` — defined in `STYLES`, resolved globally

## Status

Early development. Not ready for use.

- [x] Lexer
- [x] Parser + variable resolution
- [x] Renderer: JSON, plain text
- [ ] CLI
- [ ] PDF export
- [ ] Format specification

## Build

Requires [Rust](https://rustup.rs).

```sh
cargo build
cargo test
cargo run
```

## License

Apache 2.0
