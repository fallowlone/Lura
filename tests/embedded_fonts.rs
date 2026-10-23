use lura::engine;
use lura::lexer::Lexer;
use lura::parser::{self, Parser};

fn pdf(input: &str) -> Vec<u8> {
    let mut parser = Parser::new(Lexer::new(input).tokenize());
    let doc = parser.parse().expect("parse failed");
    let doc = parser::id::assign_ids(parser::resolver::resolve(doc));
    engine::render_pdf(&doc)
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn cyrillic_and_umlauts_are_embedded_not_replaced() {
    let out = pdf("PAGE(H1(Привет мир) P(Ärger über Straße, Україна ґєії))");
    assert!(contains(&out, "/FontFile2"), "no embedded TrueType font");
    assert!(contains(&out, "/Identity-H"));
    assert!(!contains(&out, "/Type1"), "built-in Type1 fonts only cover WinAnsi");
    // ToUnicode keeps the text extractable: 'П' U+041F and 'ґ' U+0491.
    assert!(contains(&out, "041F") && contains(&out, "0491"), "ToUnicode map is incomplete");
}

#[test]
fn only_used_faces_are_embedded_and_pdf_stays_small() {
    let out = pdf("PAGE(P(Hello world))");
    assert_eq!(out.windows(10).filter(|w| *w == b"/FontFile2").count(), 1);
    assert!(out.len() < 60_000, "pdf is {} bytes", out.len());
}

#[test]
fn bold_italic_and_code_use_separate_faces() {
    let out = pdf("PAGE(P(plain **bold** *italic* `code`))");
    assert_eq!(out.windows(10).filter(|w| *w == b"/FontFile2").count(), 4);
}
