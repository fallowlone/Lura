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
      background: #0f0f12;
    }}

    body {{
      font-family: 'Inter', system-ui, -apple-system, sans-serif;
      font-size: 16px;
      line-height: 1.7;
      color: #e8e8ed;
      padding: 48px 24px;
      -webkit-font-smoothing: antialiased;
    }}

    .folio-root {{
      max-width: 860px;
      margin: 0 auto;
    }}

    .folio-page {{
      background: #1a1a22;
      border: 1px solid #2a2a38;
      border-radius: 16px;
      padding: 56px 64px;
      margin-bottom: 32px;
      box-shadow:
        0 4px 6px rgba(0, 0, 0, 0.3),
        0 20px 60px rgba(0, 0, 0, 0.5),
        inset 0 1px 0 rgba(255,255,255,0.04);
      position: relative;
      overflow: hidden;
    }}

    .folio-page::before {{
      content: '';
      position: absolute;
      top: 0; left: 0; right: 0;
      height: 1px;
      background: linear-gradient(90deg, transparent, rgba(130, 100, 255, 0.4), transparent);
    }}

    h1.folio-h1 {{
      font-size: 2.25rem;
      font-weight: 700;
      letter-spacing: -0.03em;
      line-height: 1.2;
      margin-bottom: 24px;
      background: linear-gradient(135deg, #ffffff 0%, #a0a0c0 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }}

    h2.folio-h2 {{
      font-size: 1.6rem;
      font-weight: 600;
      letter-spacing: -0.02em;
      line-height: 1.3;
      margin-bottom: 16px;
      margin-top: 32px;
      color: #c8c8dd;
    }}

    h3.folio-h3 {{
      font-size: 1.25rem;
      font-weight: 600;
      letter-spacing: -0.01em;
      line-height: 1.4;
      margin-bottom: 12px;
      margin-top: 24px;
      color: #b0b0cc;
    }}

    p.folio-p {{
      font-size: 1rem;
      font-weight: 400;
      color: #9090a8;
      margin-bottom: 16px;
      max-width: 68ch;
    }}

    .folio-grid {{
      display: grid;
      gap: 24px;
      margin-bottom: 16px;
    }}

    .folio-code {{
      font-family: 'JetBrains Mono', 'Fira Code', monospace;
      font-size: 0.875rem;
      background: #131318;
      border: 1px solid #2a2a38;
      border-radius: 8px;
      padding: 20px 24px;
      margin-bottom: 16px;
      color: #7dd3fc;
      overflow-x: auto;
      white-space: pre;
    }}

    .folio-badge {{
      display: inline-block;
      font-family: 'JetBrains Mono', monospace;
      font-size: 0.65rem;
      color: #4a4a6a;
      position: absolute;
      bottom: 16px;
      right: 20px;
      text-transform: uppercase;
      letter-spacing: 0.1em;
    }}

    .folio-divider {{
      border: none;
      border-top: 1px solid #2a2a38;
      margin: 24px 0;
    }}

    .folio-table {{
      width: 100%;
      border-collapse: collapse;
      margin-bottom: 24px;
      font-size: 0.95rem;
    }}

    .folio-table td {{
      padding: 12px 16px;
      border-bottom: 1px solid #2a2a38;
      color: #e8e8ed;
      vertical-align: top;
    }}

    .folio-table tr:first-child td {{
      font-weight: 600;
      color: #8c8cdd;
      border-bottom: 2px solid #3a3a4c;
    }}

    .folio-table tr:last-child td {{
      border-bottom: none;
    }}

    .folio-table tr:hover td {{
      background: rgba(255, 255, 255, 0.02);
    }}

    .folio-image {{
      max-width: 100%;
      height: auto;
      border-radius: 8px;
      margin-bottom: 24px;
      display: block;
    }}"#,
        vars_css = vars_css,
    )
}

fn render_body(doc: &Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(block, &mut out, 2);
    }
    out
}

fn render_block(block: &Block, out: &mut String, indent: usize) {
    let pad = "  ".repeat(indent);

    match block.kind.as_str() {
        "PAGE" => {
            out.push_str(&format!("{}<div class=\"folio-page\">\n", pad));
            render_children(block, out, indent + 1);
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
        "GRID" => {
            let style = escape_html(&grid_style(block));
            out.push_str(&format!(
                "{}<div class=\"folio-grid\" style=\"{}\">\n",
                pad, style
            ));
            render_children(block, out, indent + 1);
            out.push_str(&format!("{}</div>\n", pad));
        }
        "TABLE" => {
            let style = build_inline_style(block);
            out.push_str(&format!("{}<table class=\"folio-table\"{}>\n", pad, style));
            render_children(block, out, indent + 1);
            out.push_str(&format!("{}</table>\n", pad));
        }
        "ROW" => {
            out.push_str(&format!("{}<tr class=\"folio-row\">\n", pad));
            render_children(block, out, indent + 1);
            out.push_str(&format!("{}</tr>\n", pad));
        }
        "CELL" => {
            let text = escape_html(&extract_text(block));
            let style = build_inline_style(block);
            out.push_str(&format!("{}<td{}>\n", pad, style));
            if has_children(block) {
                render_children(block, out, indent + 1);
            } else if !text.is_empty() {
                out.push_str(&format!("{}  {}\n", pad, text));
            }
            out.push_str(&format!("{}</td>\n", pad));
        }
        "IMAGE" => {
            let mut src = String::new();
            if let Some(Value::Str(s)) = block.attrs.get("src") {
                src = s.clone();
            }
            let mut alt = String::new();
            if let Some(Value::Str(a)) = block.attrs.get("alt") {
                alt = a.clone();
            }
            let style = build_inline_style(block);
            out.push_str(&format!("{}<img class=\"folio-image\" src=\"{}\" alt=\"{}\"{}>\n", pad, escape_html(&src), escape_html(&alt), style));
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
                render_children(block, out, indent + 1);
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

fn render_children(block: &Block, out: &mut String, indent: usize) {
    if let Content::Blocks(blocks) = &block.content {
        for child in blocks {
            render_block(child, out, indent);
        }
    }
}

fn has_children(block: &Block) -> bool {
    matches!(&block.content, Content::Blocks(b) if !b.is_empty())
}

fn extract_text(block: &Block) -> String {
    match &block.content {
        Content::Text(s) => s.clone(),
        Content::Empty => String::new(),
        Content::Blocks(_) => String::new(),
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

fn value_to_css_inline(val: &Value) -> String {
    match val {
        Value::Str(s) => sanitize_inline_css_fragment(s),
        Value::Number(n) => format!("{}", n),
        Value::Unit(n, u) => format!("{}{}", n, u),
        Value::Var(s) => format!("var(--fol-{})", sanitize_css_var_name(s)),
        Value::Color(s) => format!("#{}", sanitize_hex_color(s)),
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
        parts.push(format!("font-weight: {}", value_to_css_inline(val)));
    }
    if let Some(val) = block.attrs.get("background") {
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

/// Build grid-template-columns from GRID columns attr
fn grid_style(block: &Block) -> String {
    if let Some(val) = block.attrs.get("columns") {
        format!(
            "grid-template-columns: {}",
            value_to_css_inline(val)
        )
    } else {
        "grid-template-columns: 1fr 1fr".to_string()
    }
}
