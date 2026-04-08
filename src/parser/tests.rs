use super::{ast::*, id, resolver, Parser};
use crate::lexer::Lexer;

fn parse(input: &str) -> Document {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let doc = parser.parse().expect("parse failed");
    let doc = resolver::resolve(doc);
    id::assign_ids(doc)
}

#[test]
fn test_global_styles() {
    let doc = parse(r#"STYLES({ #mainColor: #FF0000 })"#);
    assert!(doc.vars.contains_key("mainColor"));
    assert_eq!(doc.vars["mainColor"], Value::Color("FF0000".into()));
}

#[test]
fn test_simple_page() {
    let doc = parse("PAGE(\n  P(Hello)\n)");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].kind, "PAGE");
}

#[test]
fn test_nested_blocks_in_page() {
    let doc = parse("PAGE(\n  H1(Title)\n  P(Body)\n)");
    let page = &doc.blocks[0];
    match &page.content {
        Content::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            assert_eq!(blocks[0].kind, "H1");
            assert_eq!(blocks[1].kind, "P");
        }
        _ => panic!("Expected Blocks content"),
    }
}

#[test]
fn test_block_text_content() {
    let doc = parse("P(Hello World)");
    assert_eq!(doc.blocks[0].kind, "P");
    match &doc.blocks[0].content {
        Content::Text(s) => assert_eq!(s, "Hello World"),
        _ => panic!("Expected Text content"),
    }
}

#[test]
fn test_block_attrs() {
    let doc = parse("H1({color: #mainColor} Title)");
    let attrs = &doc.blocks[0].attrs;
    assert!(attrs.contains_key("color"));
    assert_eq!(attrs["color"], Value::Var("mainColor".into()));
}

#[test]
fn test_empty_block() {
    let doc = parse("P()");
    assert_eq!(doc.blocks[0].kind, "P");
    assert!(matches!(doc.blocks[0].content, Content::Empty));
}

#[test]
fn test_variable_resolution() {
    let doc = parse(r#"
STYLES({ #brand: #0000FF })
H1({color: #brand} Title)
"#);
    let h1 = &doc.blocks[0];
    assert_eq!(h1.attrs["color"], Value::Color("0000FF".into()));
}

#[test]
fn test_unresolved_var_kept() {
    let doc = parse("H1({color: #unknown} Title)");
    assert_eq!(doc.blocks[0].attrs["color"], Value::Var("unknown".into()));
}

#[test]
fn test_full_document() {
    let doc = parse(r#"
STYLES({
  #mainColor: #FF0000
})

PAGE(
  H1({color: #mainColor} Hello World)
  P(This is a paragraph.)
)
"#);
    assert!(doc.vars.contains_key("mainColor"));
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].kind, "PAGE");
}

#[test]
fn test_explicit_block_id_parsed() {
    let doc = parse("H1[intro](Hello)");
    assert_eq!(doc.blocks[0].id, "intro");
}

#[test]
fn test_explicit_id_preserved() {
    let doc = parse("H1[intro](Hello)");
    assert_eq!(doc.blocks[0].id, "intro");
}

#[test]
fn test_auto_id_generated() {
    let doc = parse("P(Hello)");
    assert!(!doc.blocks[0].id.is_empty(), "auto ID should be non-empty");
    assert!(doc.blocks[0].id.starts_with("p_"), "auto ID should start with kind prefix");
}

#[test]
fn test_auto_id_is_deterministic() {
    let doc1 = parse("P(Hello)");
    let doc2 = parse("P(Hello)");
    assert_eq!(doc1.blocks[0].id, doc2.blocks[0].id);
}

#[test]
fn test_different_content_different_id() {
    let doc1 = parse("P(Hello)");
    let doc2 = parse("P(World)");
    assert_ne!(doc1.blocks[0].id, doc2.blocks[0].id);
}

#[test]
fn test_nested_blocks_all_get_ids() {
    let doc = parse("PAGE(\n  H1(Title)\n  P(Body)\n)");
    let page = &doc.blocks[0];
    assert!(!page.id.is_empty());
    if let Content::Blocks(children) = &page.content {
        assert!(!children[0].id.is_empty());
        assert!(!children[1].id.is_empty());
    } else {
        panic!("Expected Blocks content");
    }
}
