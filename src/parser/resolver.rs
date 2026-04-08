use std::collections::HashMap;
use super::ast::{Block, Content, Document, Value};

pub fn resolve(doc: Document) -> Document {
    let vars = &doc.vars;
    let blocks = doc.blocks.into_iter()
        .map(|block| resolve_block(block, vars))
        .collect();

    Document { vars: doc.vars, blocks }
}

fn resolve_block(block: Block, vars: &HashMap<String, Value>) -> Block {
    let attrs = block.attrs.into_iter()
        .map(|(key, val)| (key, resolve_value(val, vars)))
        .collect();

    let content = match block.content {
        Content::Blocks(blocks) => {
            Content::Blocks(blocks.into_iter().map(|b| resolve_block(b, vars)).collect())
        }
        other => other,
    };

    Block { kind: block.kind, id: block.id, attrs, content }
}

fn resolve_value(value: Value, vars: &HashMap<String, Value>) -> Value {
    match value {
        Value::Var(ref name) => {
            vars.get(name).cloned().unwrap_or(value)
        }
        other => other,
    }
}
