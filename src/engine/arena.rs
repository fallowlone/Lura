use id_arena::{Arena, Id};
use super::styles::StyledBox;

pub type NodeId = Id<StyledBox>;

/// Flat arena for all document nodes.
/// Instead of recursive `Box<Node>`, a single `Vec` under the hood;
/// children are stored as `Vec<NodeId>` (indices, not pointers).
/// This yields cache-friendly traversal and zero allocator overhead.
pub struct DocumentArena {
    pub arena: Arena<StyledBox>,
    pub roots: Vec<NodeId>,
}

impl DocumentArena {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
            roots: Vec::new(),
        }
    }

    pub fn alloc(&mut self, node: StyledBox) -> NodeId {
        self.arena.alloc(node)
    }

    pub fn get(&self, id: NodeId) -> &StyledBox {
        &self.arena[id]
    }

    pub fn get_mut(&mut self, id: NodeId) -> &mut StyledBox {
        &mut self.arena[id]
    }

    pub fn add_root(&mut self, id: NodeId) {
        self.roots.push(id);
    }
}

impl Default for DocumentArena {
    fn default() -> Self {
        Self::new()
    }
}
