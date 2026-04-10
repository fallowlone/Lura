use crate::parser::ast::{Content, Document, NodeId, Value};

/// Assign IDs to all blocks in the document.
/// Explicit IDs (non-empty block.id) are kept as-is.
/// Empty IDs are filled with a FNV-1a hash of the block's content.
/// Processing is bottom-up: children receive IDs before their parent.
pub fn assign_ids(mut doc: Document) -> Document {
    for root in doc.root_ids().to_vec() {
        assign_block_id(root, &mut doc);
    }
    doc
}

fn assign_block_id(root: NodeId, doc: &mut Document) {
    // Итеративный post-order: (node, visited_children)
    let mut stack: Vec<(NodeId, bool)> = vec![(root, false)];

    while let Some((node_id, visited)) = stack.pop() {
        if !visited {
            stack.push((node_id, true));
            if let Content::Children(children) = &doc.block(node_id).content {
                for &child_id in children.iter().rev() {
                    stack.push((child_id, false));
                }
            }
            continue;
        }

        if doc.block(node_id).id.is_empty() {
            let new_id = compute_auto_id(node_id, doc);
            doc.block_mut(node_id).id = new_id;
        }
    }
}

fn compute_auto_id(node_id: NodeId, doc: &Document) -> String {
    let block = doc.block(node_id);

    // Build sorted attrs string: "key=value,key=value"
    let mut attr_pairs: Vec<String> = block.attrs.iter()
        .map(|(k, v)| format!("{}={}", k, serialize_value(v)))
        .collect();
    attr_pairs.sort();
    let attrs_str = attr_pairs.join(",");

    // Build content string
    let content_str = match &block.content {
        Content::Text(s) => s.clone(),
        Content::Children(children) => {
            // Children already have IDs (bottom-up processing)
            children.iter()
                .map(|child_id| doc.block(*child_id).id.as_str())
                .collect::<Vec<_>>()
                .join(",")
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
