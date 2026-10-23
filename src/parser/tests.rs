use super::{ast::*, id, resolver, Parser};
use crate::lexer::Token;
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

fn all_ids(doc: &Document) -> Vec<String> {
    fn walk(doc: &Document, id: NodeId, out: &mut Vec<String>) {
        out.push(doc.block(id).id.clone());
        for &c in doc.children_ids(id) {
            walk(doc, c, out);
        }
    }
    let mut out = Vec::new();
    for &r in doc.root_ids() {
        walk(doc, r, &mut out);
    }
    out
}

#[test]
fn test_identical_blocks_get_unique_ids() {
    let doc = parse("PAGE(P(Yes) P(Yes) P(Yes))");
    let ids = all_ids(&doc);
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate ids: {ids:?}");
    // The first occurrence keeps the plain content hash; later ones are numbered in document order.
    assert!(ids[2].ends_with("-2") && ids[3].ends_with("-3"), "{ids:?}");
}

#[test]
fn test_auto_id_never_takes_an_explicit_id() {
    let plain = parse("PAGE(P(Yes))");
    let taken = plain.block(plain.children_ids(plain.root_ids()[0])[0]).id.clone();
    let doc = parse(&format!("PAGE(P(Yes) P[{taken}](other))"));
    let ids = all_ids(&doc);
    assert_eq!(ids.iter().filter(|i| **i == taken).count(), 1, "{ids:?}");
}

#[test]
fn test_duplicate_explicit_ids_are_reported() {
    let mut lexer = Lexer::new("PAGE(P[a](x) P[a](y) P[b](z) P[b](w) P[c](v))");
    let doc = Parser::new(lexer.tokenize()).parse().expect("parse failed");
    assert_eq!(id::duplicate_explicit_ids(&doc), ["a", "b"]);
}

fn parse_err(input: &str) -> String {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize();
    let positions = lexer.token_positions();
    Parser::new(tokens)
        .with_positions(positions)
        .parse()
        .expect_err("expected a parse error")
}

#[test]
fn test_parse_error_names_line_and_column() {
    let err = parse_err("PAGE(\n  LIST(\n    ITEM(a LIST(x))\n  )\n)");
    assert!(err.contains("line 3, column 12"), "{err}");
    assert!(err.contains("expected ')'") && err.contains("LIST"), "{err}");
}

#[test]
fn test_parse_error_at_end_of_input_points_at_the_end() {
    let err = parse_err("PAGE(\n  P(a)");
    assert!(err.contains("line 2"), "{err}");
}

#[test]
fn test_positions_are_one_based_and_count_characters() {
    let mut lexer = Lexer::new("P(Привет)\nH1(x)");
    let tokens = lexer.tokenize();
    let pos = lexer.token_positions();
    assert_eq!(tokens.len(), pos.len());
    let last_ident = tokens.iter().rposition(|t| matches!(t, Token::Ident(_))).unwrap();
    assert_eq!((pos[last_ident].line, pos[last_ident].column), (2, 1));
    assert_eq!((pos[0].line, pos[0].column), (1, 1));
}

#[test]
fn test_many_identical_blocks_get_unique_ids_quickly() {
    let src = format!("PAGE({})", "P(line)\n".repeat(8000));
    let doc = {
        let mut lexer = Lexer::new(&src);
        let doc = Parser::new(lexer.tokenize()).parse().expect("parse failed");
        let doc = resolver::resolve(doc);
        let started = std::time::Instant::now();
        let doc = id::assign_ids(doc);
        assert!(started.elapsed().as_secs_f32() < 2.0, "assign_ids took {:?}", started.elapsed());
        doc
    };
    let ids = all_ids(&doc);
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
}
