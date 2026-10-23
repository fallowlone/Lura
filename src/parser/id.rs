use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Content, Document, InlineNode, NodeId, Value};

/// Assign IDs to all blocks in the document.
/// Explicit IDs (non-empty block.id) are kept as-is.
/// Empty IDs are filled with a FNV-1a hash of the block's content.
/// Processing is bottom-up: children receive IDs before their parent.
/// Auto IDs are unique: a repeat of an already used ID gets `-2`, `-3`, ... in document order,
/// and an auto ID never takes one that is written explicitly anywhere in the document.
pub fn assign_ids(mut doc: Document) -> Document {
    let mut pool = IdPool::default();
    doc.for_each_block_mut(|block| {
        if !block.id.is_empty() {
            pool.used.insert(block.id.clone());
        }
    });
    for root in doc.root_ids().to_vec() {
        assign_block_id(root, &mut doc, &mut pool);
    }
    doc
}

/// Explicit IDs written more than once (sorted). Call before `assign_ids`.
pub fn duplicate_explicit_ids(doc: &Document) -> Vec<String> {
    fn walk(doc: &Document, id: NodeId, seen: &mut HashSet<String>, dups: &mut Vec<String>) {
        let block = doc.block(id);
        if !block.id.is_empty() && !seen.insert(block.id.clone()) && !dups.contains(&block.id) {
            dups.push(block.id.clone());
        }
        for &child in doc.children_ids(id) {
            walk(doc, child, seen, dups);
        }
    }
    let (mut seen, mut dups) = (HashSet::new(), Vec::new());
    for &root in doc.root_ids() {
        walk(doc, root, &mut seen, &mut dups);
    }
    dups.sort();
    dups
}

/// IDs taken so far, plus the last suffix handed out per base so that repeats do not rescan
/// every earlier `-2`, `-3`, ... (thousands of identical blocks would be quadratic).
#[derive(Default)]
struct IdPool {
    used: HashSet<String>,
    last_suffix: HashMap<String, u32>,
}

impl IdPool {
    fn unique(&mut self, base: String) -> String {
        if self.used.insert(base.clone()) {
            return base;
        }
        let n = self.last_suffix.entry(base.clone()).or_insert(1);
        loop {
            *n += 1;
            let id = format!("{base}-{n}");
            if self.used.insert(id.clone()) {
                return id;
            }
        }
    }
}

fn assign_block_id(root: NodeId, doc: &mut Document, pool: &mut IdPool) {
    // Iterative post-order: (node, visited_children)
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
            let new_id = pool.unique(compute_auto_id(node_id, doc));
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
        Content::Inline(nodes) => serialize_inline(nodes),
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

fn serialize_inline(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::TextRun(s) => {
                out.push_str("t:");
                out.push_str(s);
            }
            InlineNode::CodeSpan(s) => {
                out.push_str("c:");
                out.push_str(s);
            }
            InlineNode::Emphasis(children) => {
                out.push_str("e(");
                out.push_str(&serialize_inline(children));
                out.push(')');
            }
            InlineNode::Strong(children) => {
                out.push_str("s(");
                out.push_str(&serialize_inline(children));
                out.push(')');
            }
            InlineNode::LinkSpan { text, href } => {
                out.push_str("l(");
                out.push_str(&serialize_inline(text));
                out.push_str("->");
                out.push_str(href);
                out.push(')');
            }
        }
        out.push('|');
    }
    out
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
