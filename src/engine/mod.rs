pub mod arena;
pub mod styles;
pub mod resolver;
pub mod layout;
pub mod text;
pub mod paginate;
pub mod backend;

use crate::parser::ast::Document;

/// Полный pipeline: Document → PDF bytes
///
/// 1. Resolver:  AST → StyledTree (Arena)
/// 2. Layout:    StyledTree → LayoutTree (taffy)
/// 3. Paginate:  LayoutTree → PageTree (A4 pages)
/// 4. Backend:   PageTree → PDF bytes (pdf-writer)
pub fn render_pdf(doc: &Document) -> Vec<u8> {
    let styled = resolver::build_styled_tree(doc);
    let layout = layout::compute_layout(&styled);
    let pages  = paginate::paginate(&layout, &styled);
    backend::pdf::render(&pages)
}
