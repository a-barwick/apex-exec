use crate::{
    diagnostic::Diagnostic,
    span::Span,
    token::{Token, TokenKind},
};

pub struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        while self.cursor < self.source.len() {
            self.skip_trivia()?;
            if self.cursor >= self.source.len() {
                break;
            }
            tokens.push(self.next_token()?);
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.source.len(), self.source.len()),
            lexeme: String::new(),
        });
        Ok(tokens)
    }

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.bump();
            }
            if self.remaining().starts_with("//") {
                while self.peek().is_some_and(|ch| ch != '\n') {
                    self.bump();
                }
            } else if self.remaining().starts_with("/*") {
                let start = self.cursor;
                self.cursor += 2;
                while !self.remaining().starts_with("*/") {
                    if self.bump().is_none() {
                        return Err(Diagnostic::new(
                            "unterminated block comment",
                            Span::new(start, self.source.len()),
                        ));
                    }
                }
                self.cursor += 2;
            } else {
                return Ok(());
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        let start = self.cursor;
        let ch = self.bump().expect("lexer called before EOF");
        let kind = match ch {
            '.' => TokenKind::Dot,
            '=' => TokenKind::Equal,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            ';' => TokenKind::Semicolon,
            '\'' => return self.string_token(start),
            ch if is_identifier_start(ch) => {
                while self.peek().is_some_and(is_identifier_continue) {
                    self.bump();
                }
                let text = &self.source[start..self.cursor];
                match text.to_ascii_lowercase().as_str() {
                    "true" => TokenKind::BooleanLiteral(true),
                    "false" => TokenKind::BooleanLiteral(false),
                    _ => TokenKind::Identifier(text.to_owned()),
                }
            }
            ch if ch.is_ascii_digit() => {
                while self.peek().is_some_and(|next| next.is_ascii_digit()) {
                    self.bump();
                }
                let text = &self.source[start..self.cursor];
                let value = text.parse::<i64>().map_err(|_| {
                    Diagnostic::new(
                        "integer literal is out of range",
                        Span::new(start, self.cursor),
                    )
                })?;
                TokenKind::IntegerLiteral(value)
            }
            '"' => {
                return Err(Diagnostic::new(
                    "Apex string literals must use single quotes",
                    Span::new(start, self.cursor),
                ));
            }
            _ => {
                return Err(Diagnostic::new(
                    format!("unexpected character `{ch}`"),
                    Span::new(start, self.cursor),
                ));
            }
        };
        Ok(self.token(start, kind))
    }

    fn string_token(&mut self, start: usize) -> Result<Token, Diagnostic> {
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('\'') => return Ok(self.token(start, TokenKind::StringLiteral(value))),
                Some('\\') => match self.bump() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('\'') => value.push('\''),
                    Some('\\') => value.push('\\'),
                    Some(ch) => {
                        value.push('\\');
                        value.push(ch);
                    }
                    None => return Err(self.unterminated_string(start)),
                },
                Some('\n') | Some('\r') | None => return Err(self.unterminated_string(start)),
                Some(ch) => value.push(ch),
            }
        }
    }

    fn unterminated_string(&self, start: usize) -> Diagnostic {
        Diagnostic::new("unterminated string literal", Span::new(start, self.cursor))
    }

    fn token(&self, start: usize, kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span::new(start, self.cursor),
            lexeme: self.source[start..self.cursor].to_owned(),
        }
    }

    fn remaining(&self) -> &'a str {
        &self.source[self.cursor..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}
