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
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '=' if self.take('=') => TokenKind::EqualEqual,
            '=' if self.take('>') => TokenKind::FatArrow,
            '=' => TokenKind::Equal,
            '!' if self.take('=') => TokenKind::BangEqual,
            '!' => TokenKind::Bang,
            '<' if self.take('=') => TokenKind::LessEqual,
            '<' => TokenKind::Less,
            '>' if self.take('=') => TokenKind::GreaterEqual,
            '>' => TokenKind::Greater,
            '+' if self.take('+') => TokenKind::PlusPlus,
            '+' => TokenKind::Plus,
            '-' if self.take('-') => TokenKind::MinusMinus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '&' if self.take('&') => TokenKind::AndAnd,
            '|' if self.take('|') => TokenKind::OrOr,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
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
                    "null" => TokenKind::Null,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "for" => TokenKind::For,
                    "while" => TokenKind::While,
                    "do" => TokenKind::Do,
                    "break" => TokenKind::Break,
                    "continue" => TokenKind::Continue,
                    "return" => TokenKind::Return,
                    "new" => TokenKind::New,
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

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_control_flow_and_operator_families_case_insensitively() {
        let source = "IF (count++ <= 10 && !FALSE) { ConTinue; }";
        let kinds: Vec<TokenKind> = Lexer::new(source)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect();

        assert_eq!(
            kinds,
            vec![
                TokenKind::If,
                TokenKind::LeftParen,
                TokenKind::Identifier("count".to_owned()),
                TokenKind::PlusPlus,
                TokenKind::LessEqual,
                TokenKind::IntegerLiteral(10),
                TokenKind::AndAnd,
                TokenKind::Bang,
                TokenKind::BooleanLiteral(false),
                TokenKind::RightParen,
                TokenKind::LeftBrace,
                TokenKind::Continue,
                TokenKind::Semicolon,
                TokenKind::RightBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn distinguishes_division_from_comments_and_preserves_token_spans() {
        let source = "8 / 2 /* ignored / */ // ignored\nvalue--;";
        let tokens = Lexer::new(source).tokenize().unwrap();

        assert_eq!(tokens[1].kind, TokenKind::Slash);
        assert_eq!(&source[tokens[1].span.start..tokens[1].span.end], "/");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == TokenKind::MinusMinus)
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Slash)
                .count(),
            1
        );
    }

    #[test]
    fn reports_the_full_unterminated_block_comment_span() {
        let source = "Integer value = 1; /* never closed";
        let error = Lexer::new(source).tokenize().unwrap_err();

        assert_eq!(error.message, "unterminated block comment");
        assert_eq!(&source[error.span.start..error.span.end], "/* never closed");
    }

    #[test]
    fn tokenizes_collection_syntax_and_new_case_insensitively() {
        let source = "NeW Map<String, Integer>{'one' => values[0]}; for (String item : items) {}";
        let kinds: Vec<TokenKind> = Lexer::new(source)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect();

        assert!(matches!(kinds[0], TokenKind::New));
        assert!(kinds.contains(&TokenKind::Comma));
        assert!(kinds.contains(&TokenKind::FatArrow));
        assert!(kinds.contains(&TokenKind::LeftBracket));
        assert!(kinds.contains(&TokenKind::RightBracket));
        assert!(kinds.contains(&TokenKind::Colon));
    }
}
