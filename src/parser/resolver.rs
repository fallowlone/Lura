use std::collections::HashMap;
use super::ast::{Document, Value};

pub fn resolve(mut doc: Document) -> Document {
    let vars = doc.vars.clone();

    doc.for_each_block_mut(|block| {
        block.attrs = block.attrs.clone().into_iter()
            .map(|(key, val)| (key, resolve_value(val, &vars)))
            .collect();
    });
    doc
}

fn resolve_value(value: Value, vars: &HashMap<String, Value>) -> Value {
    match value {
        Value::Var(ref name) => {
            vars.get(name).cloned().unwrap_or(value)
        }
        other => other,
    }
}
