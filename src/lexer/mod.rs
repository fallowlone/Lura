pub mod token;

pub use token::Token;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    Normal,  // reading block names and structure
    Attrs,   // inside { }, reading key: value pairs
    Content, // reading raw text content
    /// Inside a CODE(...) body with no legacy block children — slurp verbatim
    /// until the outer `)` and emit one `Token::RawText`.
    CodeRaw,
}

/// Block idents that keep the legacy per-line `P(...)` (or other block) wrapping
/// inside a `CODE(...)` body. Anything else triggers raw-body mode.
const LEGACY_CODE_CHILD_IDENTS: &[&str] = &[
    "P", "H1", "H2", "H3", "H4", "H5", "H6",
    "LIST", "ITEM", "PAGE", "CODE", "QUOTE", "TABLE", "ROW", "CELL",
    "GRID", "FIGURE", "IMAGE", "HR", "STYLES",
];

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    mode: Mode,
    paren_depth: usize, // tracks nested ( )
    brace_depth: usize, // tracks nested { }
    /// Tracks `[...]` nesting so the `[id]` annotation's ident does not
    /// clobber `last_ident_was_code`. 0 = outside any bracket pair.
    bracket_depth: usize,
    /// True iff the most recently emitted Ident was `CODE` and no other
    /// structural token has been emitted since.
    last_ident_was_code: bool,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            mode: Mode::Normal,
            paren_depth: 0,
            brace_depth: 0,
            bracket_depth: 0,
            last_ident_was_code: false,
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

    /// Peek ahead from `self.pos` (which is just after the opening `(` of the
    /// CODE block and any `{...}` attrs block has already been tokenized-and-
    /// advanced past). Returns true iff the next non-whitespace chars form a
    /// legacy-style block ident (`P(`, `H1(`, …). Cursor is not moved.
    fn code_body_is_legacy_children(&self) -> bool {
        let mut i = self.pos;
        while matches!(self.input.get(i), Some(c) if c.is_whitespace()) {
            i += 1;
        }
        let start = i;
        while matches!(self.input.get(i), Some(c) if c.is_alphanumeric() || *c == '_') {
            i += 1;
        }
        if i == start {
            return false;
        }
        let ident: String = self.input[start..i].iter().collect();
        if !LEGACY_CODE_CHILD_IDENTS.iter().any(|k| *k == ident) {
            return false;
        }
        while matches!(self.input.get(i), Some(c) if c.is_whitespace()) {
            i += 1;
        }
        // Optional [id]
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

    /// Read verbatim chars from `self.pos` until the matching outer `)` of the
    /// CODE block (tracking balanced parens). Cursor ends on that `)`. No
    /// escape handling in v1.
    fn read_code_raw(&mut self) -> String {
        let mut s = String::new();
        let mut depth: usize = 0;
        while let Some(c) = self.current() {
            match c {
                ')' if depth == 0 => break,
                ')' => {
                    depth -= 1;
                    s.push(self.advance().unwrap());
                }
                '(' => {
                    depth += 1;
                    s.push(self.advance().unwrap());
                }
                _ => {
                    s.push(self.advance().unwrap());
                }
            }
        }
        s
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
        Token::Text(s.trim().to_string())
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        match self.mode {
            Mode::CodeRaw => {
                // Emit one RawText, then let the ')' flip us back.
                match self.current() {
                    None => Token::Eof,
                    Some(')') => {
                        self.advance();
                        self.paren_depth = self.paren_depth.saturating_sub(1);
                        self.mode = if self.paren_depth == 0 { Mode::Normal } else { Mode::Content };
                        Token::RParen
                    }
                    _ => {
                        // NB: we keep whitespace that `skip_whitespace` already
                        // consumed by re-emitting from `self.pos` via the raw
                        // reader. `skip_whitespace` only runs once at fn entry;
                        // after the first RawText emission we come back here to
                        // the ')' branch above. To preserve leading whitespace
                        // of the body we rewind past any whitespace skipped on
                        // entry. In practice `skip_whitespace` will have eaten
                        // the initial `\n` after `CODE(` — put it back so the
                        // dedent pass sees true alignment.
                        let mut back = self.pos;
                        while back > 0
                            && matches!(self.input.get(back - 1), Some(c) if c.is_whitespace())
                        {
                            back -= 1;
                        }
                        self.pos = back;
                        let body = self.read_code_raw();
                        Token::RawText(body)
                    }
                }
            }

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
                        self.mode = if self.last_ident_was_code
                            && !self.code_body_is_legacy_children()
                        {
                            Mode::CodeRaw
                        } else {
                            Mode::Content
                        };
                        self.last_ident_was_code = false;
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
                            // stay Normal briefly, LBrace will switch to Attrs;
                            // keep `last_ident_was_code` alive so the RBrace
                            // flip sees it.
                        } else if self.last_ident_was_code
                            && !self.code_body_is_legacy_children()
                        {
                            self.mode = Mode::CodeRaw;
                            self.last_ident_was_code = false;
                        } else {
                            self.mode = Mode::Content;
                            self.last_ident_was_code = false;
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
                        self.bracket_depth += 1;
                        Token::LBracket
                    }
                    Some(']') => {
                        self.advance();
                        self.bracket_depth = self.bracket_depth.saturating_sub(1);
                        Token::RBracket
                    }
                    Some(c) if c.is_alphabetic() => {
                        let s = self.read_ident();
                        // Only block-opener idents (outside any `[id]` annotation)
                        // carry the CODE signal forward; id strings inside `[...]`
                        // must not clobber the flag.
                        if self.bracket_depth == 0 {
                            self.last_ident_was_code = s == "CODE";
                        }
                        Token::Ident(s)
                    }
                    _ => { self.advance(); self.next_token() }
                }
            }
        }
    }
}
