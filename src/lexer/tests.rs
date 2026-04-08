use super::{Lexer, Token};

fn lex(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize();
    // drop Eof
    tokens.into_iter().filter(|t| t != &Token::Eof).collect()
}

#[test]
fn test_simple_block_no_attrs() {
    let tokens = lex("P(Hello World)");
    assert_eq!(tokens, vec![
        Token::Ident("P".into()),
        Token::LParen,
        Token::Text("Hello World".into()),
        Token::RParen,
    ]);
}

#[test]
fn test_block_with_attrs() {
    let tokens = lex("H1({color: #mainColor} Hello)");
    assert_eq!(tokens, vec![
        Token::Ident("H1".into()),
        Token::LParen,
        Token::LBrace,
        Token::Ident("color".into()),
        Token::Colon,
        Token::Hash("mainColor".into()),
        Token::RBrace,
        Token::Text("Hello".into()),
        Token::RParen,
    ]);
}

#[test]
fn test_styles_block() {
    let tokens = lex("STYLES({ #mainColor: #FF0000 })");
    assert_eq!(tokens, vec![
        Token::Ident("STYLES".into()),
        Token::LParen,
        Token::LBrace,
        Token::Hash("mainColor".into()),
        Token::Colon,
        Token::Hash("FF0000".into()),
        Token::RBrace,
        Token::RParen,
    ]);
}

#[test]
fn test_nested_blocks() {
    let tokens = lex("PAGE(\n  H1(Title)\n  P(Text)\n)");
    assert_eq!(tokens, vec![
        Token::Ident("PAGE".into()),
        Token::LParen,
        Token::Ident("H1".into()),
        Token::LParen,
        Token::Text("Title".into()),
        Token::RParen,
        Token::Ident("P".into()),
        Token::LParen,
        Token::Text("Text".into()),
        Token::RParen,
        Token::RParen,
    ]);
}

#[test]
fn test_number_and_unit() {
    let tokens = lex("IMAGE({width: 25mm, size: 1fr})");
    assert!(tokens.contains(&Token::Unit(25.0, "mm".into())));
    assert!(tokens.contains(&Token::Unit(1.0, "fr".into())));
}

#[test]
fn test_quoted_string() {
    let tokens = lex(r#"STYLES({ #font: "Arial" })"#);
    assert!(tokens.contains(&Token::String("Arial".into())));
}

#[test]
fn test_empty_block() {
    let tokens = lex("P()");
    assert_eq!(tokens, vec![
        Token::Ident("P".into()),
        Token::LParen,
        Token::RParen,
    ]);
}

#[test]
fn test_lbracket_rbracket() {
    let mut lexer = Lexer::new("H1[intro](Hello)");
    let tokens = lexer.tokenize();
    assert!(tokens.contains(&Token::LBracket));
    assert!(tokens.contains(&Token::RBracket));
}

#[test]
fn test_block_id_token_sequence() {
    let mut lexer = Lexer::new("P[my_id](Hello)");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0], Token::Ident("P".into()));
    assert_eq!(tokens[1], Token::LBracket);
    assert_eq!(tokens[2], Token::Ident("my_id".into()));
    assert_eq!(tokens[3], Token::RBracket);
    assert_eq!(tokens[4], Token::LParen);
}
