pub mod ast;
pub mod id;
pub mod resolver;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use ast::{Block, Content, Document, InlineNode, NodeId, Value};
use crate::lexer::token::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Result<Token, String> {
        if self.pos >= self.tokens.len() {
            return Err("unexpected end of input".into());
        }
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        Ok(t)
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.advance()? {
            Token::Ident(s) => Ok(s),
            t => Err(format!("expected identifier, got {:?}", t)),
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let t = self.advance()?;
        if &t != expected {
            Err(format!("expected {:?}, got {:?}", expected, t))
        } else {
            Ok(())
        }
    }

    pub fn parse(&mut self) -> Result<Document, String> {
        let mut vars = HashMap::new();
        let mut arena = Vec::new();
        let mut roots = Vec::new();

        while self.current() != &Token::Eof {
            match self.current().clone() {
                Token::Ident(ref name) if name == "STYLES" => {
                    let style_vars = self.parse_styles()?;
                    vars.extend(style_vars);
                }
                Token::Ident(_) => {
                    roots.push(self.parse_block(&mut arena)?);
                }
                _ => { self.advance()?; }
            }
        }

        Ok(Document::from_parts(vars, arena, roots))
    }

    // Parse STYLES({ #key: value, ... }) → HashMap
    fn parse_styles(&mut self) -> Result<HashMap<String, Value>, String> {
        self.expect_ident()?; // consume "STYLES"
        self.expect(&Token::LParen)?;
        self.expect(&Token::LBrace)?;

        let mut vars = HashMap::new();

        while self.current() != &Token::RBrace && self.current() != &Token::Eof {
            if let Token::Hash(key) = self.current().clone() {
                self.advance()?;
                self.expect(&Token::Colon)?;
                let value = self.parse_value()?;
                vars.insert(key, value);
            } else {
                self.advance()?;
            }
        }

        self.expect(&Token::RBrace)?;
        self.expect(&Token::RParen)?;

        Ok(vars)
    }

    // Parse a value token into a Value
    fn parse_value(&mut self) -> Result<Value, String> {
        match self.advance()? {
            Token::String(s) => Ok(Value::Str(s)),
            // Bare identifier used as string value: {type: ordered}, {align: center}
            Token::Ident(s) => Ok(Value::Str(s)),
            Token::Number(n) => Ok(Value::Number(n)),
            Token::Unit(n, u) => Ok(Value::Unit(n, u)),
            Token::Hash(s) => {
                // #FF0000 is a color, #name is a variable
                if s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() == 6 {
                    Ok(Value::Color(s))
                } else {
                    Ok(Value::Var(s))
                }
            }
            t => Err(format!("expected value, got {:?}", t)),
        }
    }

    // Parse attrs: { key: value, key: value }
    fn parse_attrs(&mut self) -> Result<HashMap<String, Value>, String> {
        self.expect(&Token::LBrace)?;
        let mut attrs = HashMap::new();

        while self.current() != &Token::RBrace && self.current() != &Token::Eof {
            match self.current().clone() {
                Token::Ident(key) => {
                    self.advance()?;
                    self.expect(&Token::Colon)?;
                    let value = self.parse_value()?;
                    attrs.insert(key, value);
                }
                Token::Comma => { self.advance()?; }
                _ => { self.advance()?; }
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(attrs)
    }

    // Parse a block: IDENT({attrs} content) or IDENT(content) or IDENT[id]({attrs} content)
    fn parse_block(&mut self, arena: &mut Vec<Block>) -> Result<NodeId, String> {
        let kind = self.expect_ident()?;

        // Check for optional [id]
        let id = if self.current() == &Token::LBracket {
            self.advance()?;
            let id_str = self.expect_ident()?;
            self.expect(&Token::RBracket)?;
            id_str
        } else {
            String::new()
        };

        self.expect(&Token::LParen)?;

        let attrs = if self.current() == &Token::LBrace {
            self.parse_attrs()?
        } else {
            HashMap::new()
        };

        let content = self.parse_content(arena)?;

        self.expect(&Token::RParen)?;

        let node_id = arena.len();
        arena.push(Block { kind, id, attrs, content });
        Ok(node_id)
    }

    // Parse content: text or nested blocks until RParen
    fn parse_content(&mut self, arena: &mut Vec<Block>) -> Result<Content, String> {
        match self.current().clone() {
            Token::RParen => Ok(Content::Empty),
            Token::Text(s) => {
                self.advance()?;
                Ok(Content::Inline(parse_inline_nodes(&s)))
            }
            Token::Ident(_) => {
                // nested blocks
                let mut blocks = Vec::new();
                while self.current() != &Token::RParen && self.current() != &Token::Eof {
                    match self.current().clone() {
                        Token::Ident(ref name) if name == "STYLES" => {
                            // page-level STYLES — skip for now
                            self.parse_styles()?;
                        }
                        Token::Ident(_) => {
                            blocks.push(self.parse_block(arena)?);
                        }
                        _ => { self.advance()?; }
                    }
                }
                Ok(Content::Children(blocks))
            }
            _ => {
                self.advance()?;
                Ok(Content::Empty)
            }
        }
    }
}

fn parse_inline_nodes(input: &str) -> Vec<InlineNode> {
    fn advance_char_boundary(s: &str, i: usize) -> usize {
        i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)
    }

    fn parse_segment(s: &str) -> Vec<InlineNode> {
        let mut out = Vec::new();
        let mut i = 0usize;

        while i < s.len() {
            if s[i..].starts_with("**")
                && let Some(close_rel) = s[i + 2..].find("**") {
                    let inner = &s[i + 2..i + 2 + close_rel];
                    out.push(InlineNode::Strong(parse_segment(inner)));
                    i += 4 + close_rel;
                    continue;
                }

            if s[i..].starts_with('*')
                && let Some(close_rel) = s[i + 1..].find('*') {
                    let inner = &s[i + 1..i + 1 + close_rel];
                    out.push(InlineNode::Emphasis(parse_segment(inner)));
                    i += 2 + close_rel;
                    continue;
                }

            if s[i..].starts_with('`')
                && let Some(close_rel) = s[i + 1..].find('`') {
                    let inner = &s[i + 1..i + 1 + close_rel];
                    out.push(InlineNode::CodeSpan(inner.to_string()));
                    i += 2 + close_rel;
                    continue;
                }

            if s[i..].starts_with('[')
                && let Some(close_text_rel) = s[i + 1..].find(']') {
                    let text_end = i + 1 + close_text_rel;
                    if s[text_end + 1..].starts_with('(')
                        && let Some(close_href_rel) = s[text_end + 2..].find(')') {
                            let text_inner = &s[i + 1..text_end];
                            let href_inner = &s[text_end + 2..text_end + 2 + close_href_rel];
                            out.push(InlineNode::LinkSpan {
                                text: parse_segment(text_inner),
                                href: href_inner.to_string(),
                            });
                            i = text_end + 3 + close_href_rel;
                            continue;
                        }
                }

            let mut next = advance_char_boundary(s, i);
            while next < s.len()
                && !s[next..].starts_with("**")
                && !s[next..].starts_with('*')
                && !s[next..].starts_with('`')
                && !s[next..].starts_with('[')
            {
                next = advance_char_boundary(s, next);
            }

            let chunk = &s[i..next];
            if !chunk.is_empty() {
                out.push(InlineNode::TextRun(chunk.to_string()));
            }
            i = next;
        }

        if out.is_empty() {
            out.push(InlineNode::TextRun(String::new()));
        }
        out
    }

    parse_segment(input)
}
