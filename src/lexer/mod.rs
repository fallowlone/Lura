pub mod token;

pub use token::Token;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    Normal,  // reading block names and structure
    Attrs,   // inside { }, reading key: value pairs
    Content, // reading raw text content
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    mode: Mode,
    paren_depth: usize, // tracks nested ( )
    brace_depth: usize, // tracks nested { }
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            mode: Mode::Normal,
            paren_depth: 0,
            brace_depth: 0,
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token == Token::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.current();
        self.pos += 1;
        ch
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while matches!(self.current(), Some(c) if c.is_alphanumeric() || c == '_') {
            s.push(self.advance().unwrap());
        }
        s
    }

    fn read_string(&mut self) -> Token {
        self.advance(); // skip opening "
        let mut s = String::new();
        while let Some(c) = self.current() {
            if c == '"' {
                self.advance();
                break;
            }
            s.push(self.advance().unwrap());
        }
        Token::String(s)
    }

    fn read_number(&mut self) -> Token {
        let mut s = String::new();
        while matches!(self.current(), Some(c) if c.is_ascii_digit() || c == '.') {
            s.push(self.advance().unwrap());
        }
        let value: f64 = s.parse().unwrap_or(0.0);

        let mut unit = String::new();
        while matches!(self.current(), Some(c) if c.is_alphabetic()) {
            unit.push(self.advance().unwrap());
        }

        if unit.is_empty() {
            Token::Number(value)
        } else {
            Token::Unit(value, unit)
        }
    }

    fn read_hash(&mut self) -> Token {
        self.advance(); // skip #
        let mut s = String::new();
        while matches!(self.current(), Some(c) if c.is_alphanumeric() || c == '_') {
            s.push(self.advance().unwrap());
        }
        Token::Hash(s)
    }

    // Look ahead: is the word starting at current pos followed by ( ?
    fn is_block_start(&self) -> bool {
        let mut i = self.pos;
        // skip ident chars
        while matches!(self.input.get(i), Some(c) if c.is_alphanumeric() || *c == '_') {
            i += 1;
        }
        // skip whitespace
        while matches!(self.input.get(i), Some(c) if c.is_whitespace()) {
            i += 1;
        }
        // skip optional [id]
        if matches!(self.input.get(i), Some('[')) {
            while matches!(self.input.get(i), Some(&c) if c != ']') {
                i += 1;
            }
            if matches!(self.input.get(i), Some(']')) {
                i += 1;
            }
            while matches!(self.input.get(i), Some(c) if c.is_whitespace()) {
                i += 1;
            }
        }
        matches!(self.input.get(i), Some('('))
    }

    // Read raw text content until ) or nested block (
    fn read_content(&mut self) -> Token {
        let mut s = String::new();
        while let Some(c) = self.current() {
            match c {
                ')' | '(' | '#' => break,
                _ if c.is_alphabetic() && self.is_block_start() => break,
                _ => { s.push(self.advance().unwrap()); }
            }
        }
        Token::Text(s.trim().to_string())
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        match self.mode {
            Mode::Content => {
                match self.current() {
                    None => Token::Eof,
                    Some(')') => {
                        self.advance();
                        self.paren_depth -= 1;
                        if self.paren_depth == 0 {
                            self.mode = Mode::Normal;
                        }
                        Token::RParen
                    }
                    Some('(') => {
                        self.mode = Mode::Normal;
                        self.next_token()
                    }
                    Some(c) if c.is_alphabetic() && self.is_block_start() => {
                        self.mode = Mode::Normal;
                        self.next_token()
                    }
                    Some('#') => self.read_hash(),
                    _ => {
                        let t = self.read_content();
                        if matches!(t, Token::Text(ref s) if s.is_empty()) {
                            self.next_token()
                        } else {
                            t
                        }
                    }
                }
            }

            Mode::Attrs => {
                match self.current() {
                    None => Token::Eof,
                    Some('}') => {
                        self.advance();
                        self.brace_depth -= 1;
                        self.mode = Mode::Content;
                        Token::RBrace
                    }
                    Some('{') => {
                        self.advance();
                        self.brace_depth += 1;
                        Token::LBrace
                    }
                    Some(':') => { self.advance(); Token::Colon }
                    Some(',') => { self.advance(); Token::Comma }
                    Some('"') => self.read_string(),
                    Some('#') => self.read_hash(),
                    Some(c) if c.is_ascii_digit() => self.read_number(),
                    Some(c) if c.is_alphabetic() => Token::Ident(self.read_ident()),
                    _ => { self.advance(); self.next_token() }
                }
            }

            Mode::Normal => {
                match self.current() {
                    None => Token::Eof,
                    Some('(') => {
                        self.advance();
                        self.paren_depth += 1;
                        // peek: if next non-whitespace is { → attrs, else → content
                        let next = self.input[self.pos..].iter()
                            .find(|&&c| !c.is_whitespace())
                            .copied();
                        if next == Some('{') {
                            // stay Normal briefly, LBrace will switch to Attrs
                        } else {
                            self.mode = Mode::Content;
                        }
                        Token::LParen
                    }
                    Some(')') => {
                        self.advance();
                        self.paren_depth = self.paren_depth.saturating_sub(1);
                        Token::RParen
                    }
                    Some('{') => {
                        self.advance();
                        self.brace_depth += 1;
                        self.mode = Mode::Attrs;
                        Token::LBrace
                    }
                    Some('[') => {
                        self.advance();
                        Token::LBracket
                    }
                    Some(']') => {
                        self.advance();
                        Token::RBracket
                    }
                    Some(c) if c.is_alphabetic() => Token::Ident(self.read_ident()),
                    _ => { self.advance(); self.next_token() }
                }
            }
        }
    }
}
