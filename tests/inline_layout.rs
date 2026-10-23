use lura::engine::{self, ExportFormat, ExportOptions};
use lura::parser::{self, Parser};
use lura::lexer::Lexer;

fn parse_doc(input: &str) -> lura::parser::ast::Document {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let doc = parser.parse().expect("parse failed");
    let doc = parser::resolver::resolve(doc);
    parser::id::assign_ids(doc)
}

#[test]
fn inline_mixed_emphasis_smoke_svg() {
    let doc = parse_doc("PAGE(P(Hello *italic* and **bold** world))");
    let svg = String::from_utf8(engine::render(
        &doc,
        ExportOptions { format: ExportFormat::Svg },
    )).expect("svg utf8");
    assert!(svg.contains("<text"));
    assert!(svg.contains("italic"));
    assert!(svg.contains("bold"));
}

#[test]
fn inline_link_smoke_svg() {
    let doc = parse_doc("PAGE(P(Read [docs](https://example.com) now))");
    let svg = String::from_utf8(engine::render(
        &doc,
        ExportOptions { format: ExportFormat::Svg },
    )).expect("svg utf8");
    assert!(svg.contains("Read"));
    assert!(svg.contains("docs"));
}

#[test]
fn inline_code_smoke_svg() {
    let doc = parse_doc("PAGE(P(Use `cargo test` for checks))");
    let svg = String::from_utf8(engine::render(
        &doc,
        ExportOptions { format: ExportFormat::Svg },
    )).expect("svg utf8");
    assert!(svg.contains("cargo"));
    assert!(svg.contains("test"));
}

#[test]
fn render_cache_regression_same_bytes() {
    let doc = parse_doc("PAGE(P(Cache warmup example))");
    let first = engine::render(&doc, ExportOptions { format: ExportFormat::Pdf });
    let second = engine::render(&doc, ExportOptions { format: ExportFormat::Pdf });
    assert_eq!(first, second);
}

#[test]
fn huge_colspan_is_clamped_and_renders() {
    let src = "PAGE(TABLE(ROW(CELL({colspan: 99999999999} a)CELL(b))))";
    let mut doc = lura::parser::Parser::new(lura::lexer::Lexer::new(src).tokenize())
        .parse()
        .expect("parse");
    doc = lura::parser::resolver::resolve(doc);
    doc = lura::parser::id::assign_ids(doc);
    assert!(lura::engine::render_pdf(&doc).starts_with(b"%PDF"));
}

fn svg_texts(input: &str) -> Vec<(f32, String)> {
    let svg = engine::render_svg(&parse_doc(input));
    svg.split("<text ")
        .skip(1)
        .map(|t| {
            let x = t.split("x=\"").nth(1).unwrap().split('"').next().unwrap();
            let body = t.split('>').nth(1).unwrap().split('<').next().unwrap();
            (x.parse().unwrap(), body.to_string())
        })
        .collect()
}

#[test]
fn list_items_get_markers_for_plain_text_and_multiple_blocks() {
    let t: Vec<String> = svg_texts("PAGE(LIST({type: ordered} ITEM(a) ITEM(P(x) P(y))))")
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    assert_eq!(t, ["1.", "a", "2.", "x", "y"]);
}

#[test]
fn code_block_keeps_indent_and_uses_courier_advance() {
    let t = svg_texts("PAGE(CODE(P(fn main())\nP(    four)))");
    // "fn" (2 chars) is followed by a space fragment exactly 2 * 0.6 em later.
    let (x_fn, _) = &t[0];
    let (x_space, _) = &t[1];
    assert!((x_space - x_fn - 12.0).abs() < 0.1, "fn advance {}", x_space - x_fn);
    assert!(t.iter().any(|(_, s)| s == "    "), "indentation lost: {t:?}");
}

#[test]
fn render_cache_distinguishes_inline_styling_with_equal_explicit_ids() {
    let styled = engine::render_svg(&parse_doc("PAGE(P[k](see **this** [x](http://a.example)))"));
    let plain = engine::render_svg(&parse_doc("PAGE(P[k](see this [x](http://b.example)))"));
    assert!(styled.contains("font-weight=\"700\""));
    assert!(!plain.contains("font-weight=\"700\""), "cached output of the bold document was reused");
}

#[test]
fn unknown_attributes_are_reported_but_known_ones_are_not() {
    let doc = parse_doc("PAGE(LIST({style: ordered, type: ordered} ITEM(a)) IMAGE({src: \"x\", alt: \"y\"}))");
    let unknown = engine::resolver::unknown_attributes(&doc);
    assert_eq!(unknown, [("LIST".to_string(), "style".to_string())]);
}
