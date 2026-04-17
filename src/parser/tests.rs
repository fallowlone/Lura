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
    assert_eq!(doc.root_ids().len(), 1);
    assert_eq!(doc.block(doc.root_ids()[0]).kind, "PAGE");
}

#[test]
fn test_nested_blocks_in_page() {
    let doc = parse("PAGE(\n  H1(Title)\n  P(Body)\n)");
    let page = doc.block(doc.root_ids()[0]);
    match &page.content {
        Content::Children(children) => {
            assert_eq!(children.len(), 2);
            assert_eq!(doc.block(children[0]).kind, "H1");
            assert_eq!(doc.block(children[1]).kind, "P");
        }
        _ => panic!("Expected Children content"),
    }
}

#[test]
fn test_block_text_content() {
    let doc = parse("P(Hello World)");
    let p = doc.block(doc.root_ids()[0]);
    assert_eq!(p.kind, "P");
    match &p.content {
        Content::Inline(nodes) => assert_eq!(Document::inline_text(nodes), "Hello World"),
        _ => panic!("Expected Inline content"),
    }
}

#[test]
fn showcase_first_h1_preserves_spaces_in_plaintext() {
    let src = include_str!("../../examples/showcase-large.lura");
    let doc = parse(src);
    let page = doc.block(doc.root_ids()[0]);
    let Content::Children(ids) = &page.content else {
        panic!("expected PAGE children");
    };
    let h1 = doc.block(ids[0]);
    assert_eq!(h1.kind, "H1");
    let Content::Inline(nodes) = &h1.content else {
        panic!("expected H1 inline");
    };
    let t = Document::inline_text(nodes);
    assert!(
        t.contains("Capability Showcase"),
        "spaces missing in H1 plaintext: {:?}",
        t
    );
}

#[test]
fn test_inline_markup_nodes() {
    let doc = parse("P(Hello *world* and **bold** with `code` and [link](https://example.com))");
    let p = doc.block(doc.root_ids()[0]);
    match &p.content {
        Content::Inline(nodes) => {
            let plain = Document::inline_text(nodes);
            assert!(plain.contains("Hello world"));
            assert!(plain.contains("bold"));
            assert!(plain.contains("code"));
            assert!(plain.contains("link"));
        }
        _ => panic!("Expected Inline content"),
    }
}

#[test]
fn test_block_attrs() {
    let doc = parse("H1({color: #mainColor} Title)");
    let attrs = &doc.block(doc.root_ids()[0]).attrs;
    assert!(attrs.contains_key("color"));
    assert_eq!(attrs["color"], Value::Var("mainColor".into()));
}

#[test]
fn test_empty_block() {
    let doc = parse("P()");
    let p = doc.block(doc.root_ids()[0]);
    assert_eq!(p.kind, "P");
    assert!(matches!(p.content, Content::Empty));
}

#[test]
fn test_variable_resolution() {
    let doc = parse(r#"
STYLES({ #brand: #0000FF })
H1({color: #brand} Title)
"#);
    let h1 = doc.block(doc.root_ids()[0]);
    assert_eq!(h1.attrs["color"], Value::Color("0000FF".into()));
}

#[test]
fn test_unresolved_var_kept() {
    let doc = parse("H1({color: #unknown} Title)");
    assert_eq!(doc.block(doc.root_ids()[0]).attrs["color"], Value::Var("unknown".into()));
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
    assert_eq!(doc.root_ids().len(), 1);
    assert_eq!(doc.block(doc.root_ids()[0]).kind, "PAGE");
}

#[test]
fn test_explicit_id_preserved() {
    let doc = parse("H1[intro](Hello)");
    assert_eq!(doc.block(doc.root_ids()[0]).id, "intro");
}

#[test]
fn test_auto_id_generated() {
    let doc = parse("P(Hello)");
    let p = doc.block(doc.root_ids()[0]);
    assert!(!p.id.is_empty(), "auto ID should be non-empty");
    assert!(p.id.starts_with("p_"), "auto ID should start with kind prefix");
}

#[test]
fn test_auto_id_is_deterministic() {
    let doc1 = parse("P(Hello)");
    let doc2 = parse("P(Hello)");
    assert_eq!(doc1.block(doc1.root_ids()[0]).id, doc2.block(doc2.root_ids()[0]).id);
}

#[test]
fn test_different_content_different_id() {
    let doc1 = parse("P(Hello)");
    let doc2 = parse("P(World)");
    assert_ne!(doc1.block(doc1.root_ids()[0]).id, doc2.block(doc2.root_ids()[0]).id);
}

#[test]
fn test_nested_blocks_all_get_ids() {
    let doc = parse("PAGE(\n  H1(Title)\n  P(Body)\n)");
    let page = doc.block(doc.root_ids()[0]);
    assert!(!page.id.is_empty());
    if let Content::Children(children) = &page.content {
        assert!(!doc.block(children[0]).id.is_empty());
        assert!(!doc.block(children[1]).id.is_empty());
    } else {
        panic!("Expected Children content");
    }
}

fn first_code<'a>(doc: &'a Document, id: NodeId) -> Option<&'a Block> {
    let b = doc.block(id);
    if b.kind == "CODE" {
        return Some(b);
    }
    if let Content::Children(children) = &b.content {
        for &c in children {
            if let Some(hit) = first_code(doc, c) {
                return Some(hit);
            }
        }
    }
    None
}

#[test]
fn code_raw_body_preserves_newlines_and_leading_indent() {
    let src = "PAGE(CODE(\n    fn main() {\n      println!(\"hi\");\n    }\n))";
    let doc = parse(src);
    let code = first_code(&doc, doc.root_ids()[0]).expect("CODE");
    match &code.content {
        Content::Text(body) => {
            // Common 4-space prefix stripped; indentation of inner lines preserved.
            assert_eq!(body, "fn main() {\n  println!(\"hi\");\n}");
        }
        other => panic!("expected Content::Text, got {:?}", other),
    }
}

#[test]
fn code_raw_body_with_attrs_still_raw() {
    let src = "PAGE(CODE({background: #F0F0F0}\nfn main() {}\n))";
    let doc = parse(src);
    let code = first_code(&doc, doc.root_ids()[0]).expect("CODE");
    assert!(code.attrs.contains_key("background"));
    match &code.content {
        Content::Text(body) => assert_eq!(body, "fn main() {}"),
        other => panic!("expected Content::Text, got {:?}", other),
    }
}

#[test]
fn code_legacy_block_children_still_parse_as_children() {
    let src = "PAGE(CODE(P(line one) P(line two)))";
    let doc = parse(src);
    let code = first_code(&doc, doc.root_ids()[0]).expect("CODE");
    match &code.content {
        Content::Children(children) => {
            assert_eq!(children.len(), 2);
            assert_eq!(doc.block(children[0]).kind, "P");
            assert_eq!(doc.block(children[1]).kind, "P");
        }
        other => panic!("expected Content::Children, got {:?}", other),
    }
}

#[test]
fn code_with_explicit_id_still_parses_raw_body() {
    // Regression: the `[id]` annotation ident must not clobber the
    // `last_ident_was_code` flag, or the body falls back to Content mode.
    let src = "PAGE(CODE[myid](\nfn main() {\n  println!(\"hi\");\n}\n))";
    let doc = parse(src);
    let code = first_code(&doc, doc.root_ids()[0]).expect("CODE");
    assert_eq!(code.id, "myid");
    match &code.content {
        Content::Text(body) => {
            assert_eq!(body, "fn main() {\n  println!(\"hi\");\n}");
        }
        other => panic!("expected raw Content::Text, got {:?}", other),
    }
}
