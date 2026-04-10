use id_arena::{Arena, Id};
use super::styles::StyledBox;

pub type NodeId = Id<StyledBox>;

/// Плоский Arena для всех узлов документа.
/// Вместо рекурсивных Box<Node> — единый Vec под капотом,
/// дети хранятся как Vec<NodeId> (индексы, не указатели).
/// Это даёт cache-friendly обход и 0 overhead аллокатора.
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
