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
