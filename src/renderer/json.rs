use crate::engine::paginate::TextUnit;
use crate::parser::ast::{Block, Content, Document, Value};

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"vars\": ");
    out.push_str(&render_vars(doc));
    out.push_str(",\n");
    out.push_str("  \"blocks\": [\n");
    let blocks: Vec<String> = doc.root_blocks()
        .map(|(_, block)| render_block(block, doc, 2))
        .collect();
    out.push_str(&blocks.join(",\n"));
    out.push_str("\n  ]\n}");
    out
}

fn render_vars(doc: &Document) -> String {
    if doc.vars.is_empty() {
        return "{}".into();
    }
    let mut out = String::from("{\n");
    let entries: Vec<String> = doc.vars.iter()
        .map(|(k, v)| format!("    \"{}\": {}", k, render_value(v)))
        .collect();
    out.push_str(&entries.join(",\n"));
    out.push_str("\n  }");
    out
}

fn render_block(block: &Block, doc: &Document, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = format!("{}{{\n", pad);
    out.push_str(&format!("{}  \"kind\": \"{}\",\n", pad, block.kind));
    out.push_str(&format!("{}  \"id\": \"{}\",\n", pad, block.id));

    // attrs
    out.push_str(&format!("{}  \"attrs\": ", pad));
    if block.attrs.is_empty() {
        out.push_str("{}");
    } else {
        out.push_str("{\n");
        let entries: Vec<String> = block.attrs.iter()
            .map(|(k, v)| format!("{}    \"{}\": {}", pad, k, render_value(v)))
            .collect();
        out.push_str(&entries.join(",\n"));
        out.push_str(&format!("\n{}  }}", pad));
    }
    out.push_str(",\n");

    // content
    out.push_str(&format!("{}  \"content\": {}\n", pad, render_content(&block.content, doc, indent + 1)));
    out.push_str(&format!("{}}}", pad));
    out
}

fn render_content(content: &Content, doc: &Document, indent: usize) -> String {
    match content {
        Content::Text(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Content::Inline(nodes) => format!("\"{}\"", Document::inline_text(nodes).replace('"', "\\\"")),
        Content::Empty => "null".into(),
        Content::Children(blocks) => {
            let pad = "  ".repeat(indent);
            let mut out = String::from("[\n");
            let rendered: Vec<String> = blocks.iter()
                .map(|&id| render_block(doc.block(id), doc, indent + 1))
                .collect();
            out.push_str(&rendered.join(",\n"));
            out.push_str(&format!("\n{}]", pad));
            out
        }
    }
}

/// Serialize a painted-line index for Cmd+F. Shape:
/// `{"units":[{"page":0,"x":..,"y":..,"w":..,"h":..,"text":"..","block_id":".."}]}`
pub fn text_index_json(units: &[TextUnit]) -> String {
    let mut out = String::from("{\n  \"units\": [");
    let mut first = true;
    for u in units {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "\n    {{\"page\":{},\"x\":{:.4},\"y\":{:.4},\"w\":{:.4},\"h\":{:.4},\"text\":\"{}\",\"block_id\":\"{}\"}}",
            u.page,
            u.x,
            u.y,
            u.w,
            u.h,
            escape_json_string(&u.text),
            escape_json_string(&u.block_id),
        ));
    }
    if !first {
        out.push('\n');
        out.push_str("  ");
    }
    out.push_str("]\n}");
    out
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Str(s) => format!("\"{}\"", s),
        Value::Number(n) => format!("{}", n),
        Value::Unit(n, u) => format!("\"{}{}\"", n, u),
        Value::Var(s) => format!("\"#{}\"", s),
        Value::Color(s) => format!("\"#{}\"", s),
    }
}
