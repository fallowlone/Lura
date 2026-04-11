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
