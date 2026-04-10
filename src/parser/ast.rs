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
    Children(Vec<NodeId>),
    Empty,
}

pub type NodeId = usize;

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
    arena: Vec<Block>,                   // flat list of all blocks
    roots: Vec<NodeId>,                  // top-level block IDs (PAGE, ...)
}

impl Document {
    pub(crate) fn from_parts(vars: HashMap<String, Value>, arena: Vec<Block>, roots: Vec<NodeId>) -> Self {
        Self { vars, arena, roots }
    }

    pub fn root_ids(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn root_blocks(&self) -> impl Iterator<Item = (NodeId, &Block)> {
        self.roots.iter().copied().map(|id| (id, self.block(id)))
    }

    pub fn block(&self, id: NodeId) -> &Block {
        &self.arena[id]
    }

    pub(crate) fn block_mut(&mut self, id: NodeId) -> &mut Block {
        &mut self.arena[id]
    }

    pub fn children_ids(&self, id: NodeId) -> &[NodeId] {
        match &self.block(id).content {
            Content::Children(children) => children,
            _ => &[],
        }
    }

    pub(crate) fn for_each_block_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Block),
    {
        for block in &mut self.arena {
            f(block);
        }
    }
}
