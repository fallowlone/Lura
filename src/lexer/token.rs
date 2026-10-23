#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // Block types
    Ident(String), // H1, P, PAGE, STYLES, GRID, IMAGE, ...

    // Delimiters
    LParen,  // (
    RParen,  // )
    LBrace,  // {
    RBrace,  // }
    LBracket, // [
    RBracket, // ]

    // Attributes
    Colon,   // :
    Comma,   // ,

    // Values
    Text(String),    // raw text content
    String(String),  // "quoted string"
    Number(f64),     // 24, 1.5
    Unit(f64, String), // 25mm, 1fr
    Hash(String),    // #mainColor, #FF0000

    // Special
    Eof,
}

/// 1-based source position of a token (columns count characters, not bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

impl std::fmt::Display for Token {
    /// How a token reads in an error message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Ident(s) => write!(f, "block name {s}"),
            Token::LParen => f.write_str("'('"),
            Token::RParen => f.write_str("')'"),
            Token::LBrace => f.write_str("'{'"),
            Token::RBrace => f.write_str("'}'"),
            Token::LBracket => f.write_str("'['"),
            Token::RBracket => f.write_str("']'"),
            Token::Colon => f.write_str("':'"),
            Token::Comma => f.write_str("','"),
            Token::Text(s) => write!(f, "text \"{s}\""),
            Token::String(s) => write!(f, "string \"{s}\""),
            Token::Number(n) => write!(f, "number {n}"),
            Token::Unit(n, u) => write!(f, "{n}{u}"),
            Token::Hash(s) => write!(f, "#{s}"),
            Token::Eof => f.write_str("end of input"),
        }
    }
}
