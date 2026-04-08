pub mod ast;
pub mod resolver;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use ast::{Block, Content, Document, Value};
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
        let mut blocks = Vec::new();

        while self.current() != &Token::Eof {
            match self.current().clone() {
                Token::Ident(ref name) if name == "STYLES" => {
                    let style_vars = self.parse_styles()?;
                    vars.extend(style_vars);
                }
                Token::Ident(_) => {
                    blocks.push(self.parse_block()?);
                }
                _ => { self.advance()?; }
            }
        }

        Ok(Document { vars, blocks })
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
            } else if self.current() == &Token::Comma {
                self.advance()?;
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
    fn parse_block(&mut self) -> Result<Block, String> {
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

        let content = self.parse_content()?;

        self.expect(&Token::RParen)?;

        Ok(Block { kind, id, attrs, content })
    }

    // Parse content: text or nested blocks until RParen
    fn parse_content(&mut self) -> Result<Content, String> {
        match self.current().clone() {
            Token::RParen => Ok(Content::Empty),
            Token::Text(s) => {
                self.advance()?;
                Ok(Content::Text(s))
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
                            blocks.push(self.parse_block()?);
                        }
                        _ => { self.advance()?; }
                    }
                }
                Ok(Content::Blocks(blocks))
            }
            _ => {
                self.advance()?;
                Ok(Content::Empty)
            }
        }
    }
}
