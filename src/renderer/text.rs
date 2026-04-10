use crate::parser::ast::{Block, Content, Document};

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
        "P" => {
            out.push_str(&format!("{}\n\n", extract_text(block)));
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

fn extract_text(block: &Block) -> String {
    match &block.content {
        Content::Text(s) => s.clone(),
        Content::Empty => String::new(),
        Content::Children(_) => String::new(),
    }
}
