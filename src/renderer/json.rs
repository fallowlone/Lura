use crate::parser::ast::{Block, Content, Document, Value};

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"vars\": ");
    out.push_str(&render_vars(doc));
    out.push_str(",\n");
    out.push_str("  \"blocks\": [\n");
    let blocks: Vec<String> = doc.blocks.iter()
        .map(|b| render_block(b, 2))
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

fn render_block(block: &Block, indent: usize) -> String {
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
    out.push_str(&format!("{}  \"content\": {}\n", pad, render_content(&block.content, indent + 1)));
    out.push_str(&format!("{}}}", pad));
    out
}

fn render_content(content: &Content, indent: usize) -> String {
    match content {
        Content::Text(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Content::Empty => "null".into(),
        Content::Blocks(blocks) => {
            let pad = "  ".repeat(indent);
            let mut out = String::from("[\n");
            let rendered: Vec<String> = blocks.iter()
                .map(|b| render_block(b, indent + 1))
                .collect();
            out.push_str(&rendered.join(",\n"));
            out.push_str(&format!("\n{}]", pad));
            out
        }
    }
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
