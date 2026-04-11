//! Integration checks for Document → layout → pagination.

use crate::engine::layout::compute_layout;
use crate::engine::paginate::paginate;
use crate::engine::resolver::build_styled_tree;
use crate::lexer::Lexer;
use crate::parser::{self, Parser};

fn load_fol(src: &str) -> crate::parser::ast::Document {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let doc = parser.parse().expect("parse");
    let doc = parser::resolver::resolve(doc);
    parser::id::assign_ids(doc)
}

/// Each top-level `PAGE` must begin on a new physical page when the prior
/// page still has room (short content). Regression: merged PAGE blocks on one PDF page.
#[test]
fn five_page_blocks_yield_five_physical_pages() {
    let mut fol = String::new();
    for i in 0..5 {
        fol.push_str(&format!("PAGE(P(Page {i} short.))\n"));
    }
    let doc = load_fol(&fol);
    let styled = build_styled_tree(&doc);
    let layout = compute_layout(&styled);
    assert_eq!(
        layout.roots.len(),
        5,
        "fixture must produce one layout root per PAGE block"
    );
    let pages = paginate(&layout, &styled);
    assert_eq!(
        pages.pages.len(),
        5,
        "each PAGE block must start a new physical page"
    );
}
