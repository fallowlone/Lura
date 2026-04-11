use crate::engine::grid_tracks::{parse_grid_columns_value, tracks_to_css, GridColumnTrack};
use crate::parser::ast::{Block, Content, Document, Value};

pub fn render(doc: &Document) -> String {
    let body = render_body(doc);
    let css = build_css(doc);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Folio Document</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
  <style>
{css}
  </style>
</head>
<body>
  <div class="folio-root">
{body}
  </div>
</body>
</html>"#,
        css = css,
        body = body,
    )
}

fn build_css(doc: &Document) -> String {
    // Resolve CSS custom properties from STYLES block vars
    let mut vars_css = String::new();
    for (key, val) in &doc.vars {
        vars_css.push_str(&format!(
            "    --fol-{}: {};\n",
            sanitize_css_var_name(key),
            value_for_root_custom_property(val)
        ));
    }

    format!(
        r#"    :root {{
{vars_css}    }}

    *, *::before, *::after {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }}

    html, body {{
      height: 100%;
      background: #e8e8e8;
      color-scheme: light;
    }}

    body {{
      font-family: 'Inter', system-ui, -apple-system, sans-serif;
      font-size: 14px;
      line-height: 1.6;
      color: #1a1a1a;
      padding: 32px 24px;
      -webkit-font-smoothing: antialiased;
    }}

    .folio-root {{
      max-width: 800px;
      margin: 0 auto;
    }}

    .folio-page {{
      background: #ffffff;
      border-radius: 4px;
      padding: 48px 56px;
      margin-bottom: 24px;
      box-shadow: 0 2px 12px rgba(0, 0, 0, 0.15);
      position: relative;
    }}

    .folio-page::after {{
      content: '.fol';
      position: absolute;
      bottom: 12px;
      right: 16px;
      font-size: 0.6rem;
      color: #cccccc;
      font-family: monospace;
      letter-spacing: 0.05em;
    }}

    h1.folio-h1 {{
      font-size: 1.75rem;
      font-weight: 700;
      letter-spacing: -0.02em;
      line-height: 1.25;
      margin-bottom: 6px;
      color: #111111;
    }}

    h2.folio-h2 {{
      font-size: 1.1rem;
      font-weight: 600;
      letter-spacing: -0.01em;
      line-height: 1.35;
      margin-bottom: 8px;
      margin-top: 24px;
      color: #222222;
    }}

    h3.folio-h3 {{
      font-size: 1rem;
      font-weight: 600;
      line-height: 1.4;
      margin-bottom: 6px;
      margin-top: 18px;
      color: #333333;
    }}

    p.folio-p {{
      font-size: 0.9rem;
      color: #333333;
      margin-bottom: 10px;
      line-height: 1.6;
    }}

    .folio-grid {{
      display: grid;
      gap: 16px;
      margin-bottom: 12px;
    }}

    ul.folio-list {{
      margin: 4px 0 10px 20px;
      padding: 0;
    }}

    ul.folio-list li, ol.folio-list li {{
      font-size: 0.9rem;
      color: #333333;
      margin-bottom: 4px;
      line-height: 1.5;
    }}

    ol.folio-list {{
      margin: 4px 0 10px 20px;
      padding: 0;
    }}

    .folio-code {{
      font-family: 'JetBrains Mono', 'Fira Code', monospace;
      font-size: 0.8rem;
      background: #f5f5f5;
      border: 1px solid #e0e0e0;
      border-radius: 4px;
      padding: 14px 16px;
      margin-bottom: 12px;
      color: #333333;
      overflow-x: auto;
      white-space: pre;
    }}

    .folio-divider {{
      border: none;
      border-top: 1px solid #e0e0e0;
      margin: 16px 0;
    }}

    .folio-table {{
      width: 100%;
      border-collapse: collapse;
      margin-bottom: 16px;
      font-size: 0.875rem;
    }}

    .folio-table td {{
      padding: 8px 12px;
      border-bottom: 1px solid #eeeeee;
      color: #222222;
      vertical-align: top;
    }}

    .folio-table tr:first-child td {{
      font-weight: 600;
      background: #f5f5f5;
      border-bottom: 2px solid #dddddd;
      color: #111111;
    }}

    .folio-table tr:last-child td {{
      border-bottom: none;
    }}

    .folio-table tr:hover td {{
      background: #fafafa;
    }}

    .folio-image {{
      max-width: 100%;
      height: auto;
      border-radius: 4px;
      margin-bottom: 16px;
      display: block;
    }}

    .folio-figure {{
      margin: 0 0 16px 0;
    }}

    .folio-figure .folio-image {{
      margin-bottom: 8px;
    }}

    .folio-figure figcaption {{
      font-size: 0.8125rem;
      color: #555555;
      line-height: 1.4;
    }}"#,
        vars_css = vars_css,
    )
}

fn render_image_element(block: &Block, pad: &str) -> String {
    let mut src = String::new();
    if let Some(Value::Str(s)) = block.attrs.get("src") {
        src = s.clone();
    }
    let mut alt = String::new();
    if let Some(Value::Str(a)) = block.attrs.get("alt") {
        alt = a.clone();
    }
    let style = build_inline_style(block);
    format!(
        "{}<img class=\"folio-image\" src=\"{}\" alt=\"{}\"{}>\n",
        pad,
        escape_html(&src),
        escape_html(&alt),
        style
    )
}

fn render_body(doc: &Document) -> String {
    let mut out = String::new();
    for (_, block) in doc.root_blocks() {
        render_block(block, doc, &mut out, 2);
    }
    out
}

fn render_block(block: &Block, doc: &Document, out: &mut String, indent: usize) {
    let pad = "  ".repeat(indent);

    match block.kind.as_str() {
        "PAGE" => {
            out.push_str(&format!("{}<div class=\"folio-page\">\n", pad));
            render_children(block, doc, out, indent + 1);
            out.push_str(&format!(
                "{0}  <span class=\"folio-badge\">.fol</span>\n{0}</div>\n",
                pad
            ));
        }
        "H1" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!(
                "{}<h1 class=\"folio-h1\"{}>{}</h1>\n",
                pad, style, text
            ));
        }
        "H2" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!(
                "{}<h2 class=\"folio-h2\"{}>{}</h2>\n",
                pad, style, text
            ));
        }
        "H3" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!(
                "{}<h3 class=\"folio-h3\"{}>{}</h3>\n",
                pad, style, text
            ));
        }
        "P" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!(
                "{}<p class=\"folio-p\"{}>{}</p>\n",
                pad, style, text
            ));
        }
        "CODE" => {
            let text = escape_html(&extract_text(block));
            out.push_str(&format!("{}<div class=\"folio-code\">{}</div>\n", pad, text));
        }
        "HR" | "DIVIDER" => {
            out.push_str(&format!("{}<hr class=\"folio-divider\">\n", pad));
        }
        "LIST" => {
            let ordered = block.attrs.get("type")
                .and_then(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
                .map(|s| matches!(s, "ordered" | "ol" | "numbered"))
                .unwrap_or(false);
            let tag = if ordered { "ol" } else { "ul" };
            out.push_str(&format!("{}<{} class=\"folio-list\">\n", pad, tag));
            render_children(block, doc, out, indent + 1);
            out.push_str(&format!("{}</{}>\n", pad, tag));
        }
        "ITEM" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!("{}<li{}>{}</li>\n", pad, style, text));
        }
        "GRID" => {
            let style = escape_html(&grid_style(block));
            out.push_str(&format!(
                "{}<div class=\"folio-grid\" style=\"{}\">\n",
                pad, style
            ));
            render_children(block, doc, out, indent + 1);
            out.push_str(&format!("{}</div>\n", pad));
        }
        "TABLE" => {
            let style = build_inline_style(block);
            out.push_str(&format!("{}<table class=\"folio-table\"{}>\n", pad, style));
            render_children(block, doc, out, indent + 1);
            out.push_str(&format!("{}</table>\n", pad));
        }
        "ROW" => {
            out.push_str(&format!("{}<tr class=\"folio-row\">\n", pad));
            render_children(block, doc, out, indent + 1);
            out.push_str(&format!("{}</tr>\n", pad));
        }
        "CELL" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!("{}<td{}>\n", pad, style));
            if has_children(block) {
                render_children(block, doc, out, indent + 1);
            } else if !text.is_empty() {
                out.push_str(&format!("{}  {}\n", pad, text));
            }
            out.push_str(&format!("{}</td>\n", pad));
        }
        "FIGURE" => {
            let style = build_inline_style(block);
            out.push_str(&format!("{}<figure class=\"folio-figure\"{}>\n", pad, style));
            if has_children(block) {
                render_children(block, doc, out, indent + 1);
            } else {
                let inner_pad = format!("{}  ", pad);
                out.push_str(&render_image_element(block, &inner_pad));
                if let Some(Value::Str(c)) = block.attrs.get("caption") {
                    out.push_str(&format!(
                        "{}  <figcaption>{}</figcaption>\n",
                        pad,
                        escape_html(c)
                    ));
                }
            }
            out.push_str(&format!("{}</figure>\n", pad));
        }
        "IMAGE" => {
            out.push_str(&render_image_element(block, &pad));
        }
        "STYLES" => {
            // STYLES blocks are handled at Document level, skip here
        }
        _ => {
            // Unknown block — render children if any, otherwise skip
            if has_children(block) {
                out.push_str(&format!(
                    "{}<div class=\"folio-{}\">\n",
                    pad,
                    sanitize_kind_for_class(&block.kind)
                ));
                render_children(block, doc, out, indent + 1);
                out.push_str(&format!("{}</div>\n", pad));
            } else {
                let text = escape_html(&extract_text(block));
                if !text.is_empty() {
                    out.push_str(&format!("{}<p class=\"folio-p\">{}</p>\n", pad, text));
                }
            }
        }
    }
}

fn render_children(block: &Block, doc: &Document, out: &mut String, indent: usize) {
    if let Content::Children(children) = &block.content {
        for &child in children {
            render_block(doc.block(child), doc, out, indent);
        }
    }
}

fn has_children(block: &Block) -> bool {
    matches!(&block.content, Content::Children(children) if !children.is_empty())
}

fn extract_text(block: &Block) -> String {
    match &block.content {
        Content::Text(s) => s.clone(),
        Content::Inline(nodes) => Document::inline_text(nodes),
        Content::Empty => String::new(),
        Content::Children(_) => String::new(),
    }
}

/// Escape text for HTML text nodes and attributes (shared with FFI error pages).
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn sanitize_css_var_name(s: &str) -> String {
    if s.is_empty() {
        return "_".to_string();
    }
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_kind_for_class(kind: &str) -> String {
    kind.to_lowercase().chars().map(sanitize_class_char).collect()
}

fn sanitize_class_char(c: char) -> char {
    if c.is_ascii_alphanumeric() || c == '-' {
        c
    } else {
        '-'
    }
}

/// CSS string token: double-quoted, for custom properties in `<style>` (allows `;` inside).
fn css_double_quoted_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\A "),
            '\r' => out.push_str("\\D "),
            '\0' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn sanitize_hex_color(s: &str) -> String {
    let hex: String = s
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    if hex.is_empty() {
        "000000".to_string()
    } else {
        hex
    }
}

fn value_for_root_custom_property(val: &Value) -> String {
    match val {
        Value::Str(s) => css_double_quoted_string(s),
        Value::Number(n) => format!("{}", n),
        Value::Unit(n, u) => format!("{}{}", n, u),
        Value::Var(s) => format!("var(--fol-{})", sanitize_css_var_name(s)),
        Value::Color(s) => format!("#{}", sanitize_hex_color(s)),
    }
}

/// Strip characters that would break a single declaration inside `style="..."`.
fn sanitize_inline_css_fragment(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !matches!(
                c,
                ';' | '{' | '}' | '\\' | '\n' | '\r' | '\0' | '"' | '&' | '<'
            )
        })
        .collect()
}

/// Convert a Value to a CSS length/color/string fragment.
/// Bare `Value::Number` is treated as a point value — appends `pt`.
fn value_to_css_inline(val: &Value) -> String {
    match val {
        Value::Str(s) => sanitize_inline_css_fragment(s),
        Value::Number(n) => format!("{}pt", n),
        Value::Unit(n, u) => format!("{}{}", n, u),
        Value::Var(s) => format!("var(--fol-{})", sanitize_css_var_name(s)),
        Value::Color(s) => format!("#{}", sanitize_hex_color(s)),
    }
}

/// Convert a Value to a CSS unitless number (for font-weight etc.).
/// Does not append a unit to bare `Value::Number`.
fn value_to_css_unitless(val: &Value) -> String {
    match val {
        Value::Number(n) => format!("{}", n),
        other => value_to_css_inline(other),
    }
}

/// Build `style="..."` attribute string from block attrs (color, font-size, etc.)
fn build_inline_style(block: &Block) -> String {
    let mut parts = Vec::new();

    if let Some(val) = block.attrs.get("color") {
        parts.push(format!("color: {}", value_to_css_inline(val)));
    }
    if let Some(val) = block.attrs.get("font-size") {
        parts.push(format!("font-size: {}", value_to_css_inline(val)));
    }
    if let Some(val) = block.attrs.get("font-weight") {
        parts.push(format!("font-weight: {}", value_to_css_unitless(val)));
    }
    if let Some(val) = block.attrs.get("background").or_else(|| block.attrs.get("background-color")) {
        parts.push(format!("background: {}", value_to_css_inline(val)));
    }
    if let Some(val) = block.attrs.get("margin") {
        parts.push(format!("margin: {}", value_to_css_inline(val)));
    }
    if let Some(val) = block.attrs.get("padding") {
        parts.push(format!("padding: {}", value_to_css_inline(val)));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", escape_html(&parts.join("; ")))
    }
}

/// Build grid-template-columns from GRID columns attr (согласовано с engine resolver).
fn grid_style(block: &Block) -> String {
    let mut tracks = vec![GridColumnTrack::Fr(1.0)];
    if let Some(value) = block
        .attrs
        .get("columns")
        .or_else(|| block.attrs.get("grid-columns"))
    {
        if let Some(parsed) = parse_grid_columns_value(value) {
            tracks = parsed;
        }
    }
    format!("grid-template-columns: {}", tracks_to_css(&tracks))
}
