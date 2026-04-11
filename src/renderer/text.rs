use crate::parser::ast::{Block, Content, Document, Value};

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    for (_, block) in doc.root_blocks() {
        render_block(block, doc, &mut out, 0);
    }
    out.trim().to_string()
}

fn render_block(block: &Block, doc: &Document, out: &mut String, depth: usize) {
    match block.kind.as_str() {
        "PAGE" => {
            if depth > 0 {
                out.push_str("\n--- Page ---\n\n");
            }
            render_children(block, doc, out, depth);
        }
        "H1" => {
            out.push_str(&format!("# {}\n\n", extract_text(block)));
        }
        "H2" => {
            out.push_str(&format!("## {}\n\n", extract_text(block)));
        }
        "H3" => {
            out.push_str(&format!("### {}\n\n", extract_text(block)));
        }
        "H4" => {
            out.push_str(&format!("#### {}\n\n", extract_text(block)));
        }
        "H5" => {
            out.push_str(&format!("##### {}\n\n", extract_text(block)));
        }
        "H6" => {
            out.push_str(&format!("###### {}\n\n", extract_text(block)));
        }
        "P" => {
            out.push_str(&format!("{}\n\n", extract_text(block)));
        }
        "CODE" => {
            out.push_str("```\n");
            out.push_str(&extract_text(block));
            out.push_str("\n```\n\n");
        }
        "TABLE" => {
            out.push_str("--- table ---\n");
            render_children(block, doc, out, depth);
            out.push('\n');
        }
        "ROW" => {
            render_children(block, doc, out, depth);
        }
        "CELL" => {
            if has_children(block) {
                render_children(block, doc, out, depth);
            } else {
                let t = extract_text(block);
                if !t.is_empty() {
                    out.push_str(&format!("  · {}\n", t));
                }
            }
        }
        "FIGURE" => {
            out.push_str("[figure]\n");
            if let Content::Children(children) = &block.content {
                for &child in children {
                    render_block(doc.block(child), doc, out, depth + 1);
                }
            } else {
                if let Some(Value::Str(src)) = block.attrs.get("src") {
                    out.push_str(&format!("  (image: {})\n", src));
                }
                if let Some(Value::Str(c)) = block.attrs.get("caption") {
                    out.push_str(&format!("  caption: {}\n", c));
                }
            }
            out.push_str("\n");
        }
        "IMAGE" => {
            if let Some(Value::Str(src)) = block.attrs.get("src") {
                out.push_str(&format!("![image]({})\n\n", src));
            } else {
                out.push_str("[image]\n\n");
            }
        }
        "GRID" => {
            render_children(block, doc, out, depth);
        }
        _ => {
            // unknown block — render content if any
            render_children(block, doc, out, depth);
        }
    }
}

fn render_children(block: &Block, doc: &Document, out: &mut String, depth: usize) {
    if let Content::Children(children) = &block.content {
        for &child in children {
            render_block(doc.block(child), doc, out, depth + 1);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::{id, resolver, Parser};

    fn parse(input: &str) -> Document {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let doc = parser.parse().expect("parse failed");
        let doc = resolver::resolve(doc);
        id::assign_ids(doc)
    }

    #[test]
    fn table_cell_with_nested_blocks_exports_paragraph_text() {
        let doc = parse("PAGE(TABLE(ROW(CELL(P(Hello from cell)))))");
        let out = render(&doc);
        assert!(
            out.contains("Hello from cell"),
            "expected nested P in CELL; got {out:?}"
        );
        assert!(
            !out.contains("  · \n"),
            "empty bullet line means extract_text-only CELL path; got {out:?}"
        );
    }
}
