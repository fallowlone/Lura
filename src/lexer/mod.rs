pub mod token;

pub use token::{Pos, Token};

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
    last_block: String, // most recent block name, to tell CODE from other blocks
    in_bracket: bool,   // inside [id], so its ident is not a block name
    block_stack: Vec<bool>, // per open '(': is it a CODE block
    token_start: usize,     // char offset where the token being read begins
    starts: Vec<usize>,     // start offset of every token returned by `tokenize`
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            mode: Mode::Normal,
            paren_depth: 0,
            brace_depth: 0,
            last_block: String::new(),
            in_bracket: false,
            block_stack: Vec::new(),
            token_start: 0,
            starts: Vec::new(),
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token == Token::Eof;
            self.starts.push(self.token_start.min(self.input.len()));
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
        while matches!(self.current(), Some(c) if c.is_alphanumeric() || c == '_' || c == '-') {
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

    // Look ahead: is the word starting at current pos an ALL-CAPS block name followed by ( ?
    // Block names in FOL are always uppercase: H1, TABLE, CELL, GRID, LIST, ITEM, etc.
    // This prevents German words like "Informationen (" from being parsed as blocks.
    fn is_block_start(&self) -> bool {
        let mut i = self.pos;
        let start = i;
        // skip ident chars
        while matches!(self.input.get(i), Some(c) if c.is_alphanumeric() || *c == '_' || *c == '-') {
            i += 1;
        }
        if i == start {
            return false; // empty ident
        }
        // ident must be all-uppercase (block names: H1, TABLE, CELL, …)
        let all_upper = self.input[start..i].iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_' || *c == '-');
        if !all_upper {
            return false;
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

    // Read raw text content until unbalanced ) or nested block name
    fn read_content(&mut self) -> Token {
        let mut s = String::new();
        let mut paren_depth = 0usize;
        while let Some(c) = self.current() {
            match c {
                '\\' => {
                    self.advance();
                    if let Some(c) = self.advance() {
                        s.push(c);
                    }
                }
                ')' if paren_depth == 0 => break,
                ')' => {
                    paren_depth -= 1;
                    s.push(self.advance().unwrap());
                }
                '(' => {
                    paren_depth += 1;
                    s.push(self.advance().unwrap());
                }
                '#' if paren_depth == 0 => break,
                _ if c.is_alphabetic() && paren_depth == 0 && self.is_block_start() => break,
                _ => { s.push(self.advance().unwrap()); }
            }
        }
        if self.in_code() && !s.trim().is_empty() {
            return Token::Text(s.trim_end().trim_start_matches(['\n', '\r']).to_string());
        }
        Token::Text(s.trim().to_string())
    }

    /// Source position of every token returned by the last `tokenize` call (same length).
    pub fn token_positions(&self) -> Vec<Pos> {
        let mut out = Vec::with_capacity(self.starts.len());
        let (mut line, mut column, mut at) = (1u32, 1u32, 0usize);
        for &start in &self.starts {
            while at < start {
                if self.input[at] == '\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                at += 1;
            }
            out.push(Pos { line, column });
        }
        out
    }

    fn in_code(&self) -> bool {
        self.block_stack.iter().any(|&is_code| is_code)
    }

    fn next_token(&mut self) -> Token {
        // Inside CODE, leading whitespace of a line is indentation, not separator.
        if !(self.mode == Mode::Content && self.in_code()) {
            self.skip_whitespace();
        }
        self.token_start = self.pos;

        match self.mode {
            Mode::Content => {
                match self.current() {
                    None => Token::Eof,
                    Some(')') => {
                        self.advance();
                        self.paren_depth = self.paren_depth.saturating_sub(1);
                        self.block_stack.pop();
                        if self.paren_depth == 0 {
                            self.mode = Mode::Normal;
                        }
                        Token::RParen
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
                        self.block_stack.push(self.last_block == "CODE");
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
                        self.block_stack.pop();
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
                        self.in_bracket = true;
                        Token::LBracket
                    }
                    Some(']') => {
                        self.advance();
                        self.in_bracket = false;
                        Token::RBracket
                    }
                    Some(c) if c.is_alphabetic() => {
                        let ident = self.read_ident();
                        if !self.in_bracket {
                            self.last_block = ident.clone();
                        }
                        Token::Ident(ident)
                    }
                    _ => { self.advance(); self.next_token() }
                }
            }
        }
    }
}
