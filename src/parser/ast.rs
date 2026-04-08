use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),        // "Arial"
    Number(f64),        // 24
    Unit(f64, String),  // 25mm, 1fr
    Var(String),        // #mainColor
    Color(String),      // #FF0000
}

#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    Blocks(Vec<Block>),
    Empty,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: String,                    // "H1", "P", "PAGE", ...
    pub id: String,                      // empty = not yet assigned
    pub attrs: HashMap<String, Value>,   // key → value
    pub content: Content,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub vars: HashMap<String, Value>,    // global STYLES variables
    pub blocks: Vec<Block>,              // top-level blocks (PAGE, ...)
}
