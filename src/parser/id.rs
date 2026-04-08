use crate::parser::ast::{Block, Content, Document, Value};

/// Assign IDs to all blocks in the document.
/// Explicit IDs (non-empty block.id) are kept as-is.
/// Empty IDs are filled with a FNV-1a hash of the block's content.
/// Processing is bottom-up: children receive IDs before their parent.
pub fn assign_ids(mut doc: Document) -> Document {
    for block in &mut doc.blocks {
        assign_block_id(block);
    }
    doc
}

fn assign_block_id(block: &mut Block) {
    // Process children first (bottom-up)
    if let Content::Blocks(ref mut children) = block.content {
        for child in children.iter_mut() {
            assign_block_id(child);
        }
    }
    // Assign this block's ID if not set explicitly
    if block.id.is_empty() {
        block.id = compute_auto_id(block);
    }
}

fn compute_auto_id(block: &Block) -> String {
    // Build sorted attrs string: "key=value,key=value"
    let mut attr_pairs: Vec<String> = block.attrs.iter()
        .map(|(k, v)| format!("{}={}", k, serialize_value(v)))
        .collect();
    attr_pairs.sort();
    let attrs_str = attr_pairs.join(",");

    // Build content string
    let content_str = match &block.content {
        Content::Text(s) => s.clone(),
        Content::Blocks(children) => {
            // Children already have IDs (bottom-up processing)
            children.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(",")
        }
        Content::Empty => String::new(),
    };

    let input = format!("{}|{}|{}", block.kind, attrs_str, content_str);
    let hash = fnv1a(&input);
    // Use lower 32 bits -> 8 hex chars
    format!("{}_{:08x}", block.kind.to_lowercase(), hash as u32)
}

fn serialize_value(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Unit(n, u) => format!("{}{}", n, u),
        Value::Var(s) => format!("var:#{}", s),
        Value::Color(s) => format!("color:#{}", s),
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}
