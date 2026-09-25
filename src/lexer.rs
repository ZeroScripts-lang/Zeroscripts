use crate::error::LexerError;
use crate::token::{Token, TokenKind};

pub struct Lexer {
    source: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, LexerError> {
        self.skip_whitespace_and_comments();

        if self.is_at_end() {
            return Ok(Token::new(TokenKind::Eof, self.line, self.column));
        }

        let ch = self.current_char();
        let line = self.line;
        let column = self.column;

        match ch {
            '(' => { self.advance(); Ok(Token::new(TokenKind::LeftParen, line, column)) }
            ')' => { self.advance(); Ok(Token::new(TokenKind::RightParen, line, column)) }
            '{' => { self.advance(); Ok(Token::new(TokenKind::LeftBrace, line, column)) }
            '}' => { self.advance(); Ok(Token::new(TokenKind::RightBrace, line, column)) }
            '[' => { self.advance(); Ok(Token::new(TokenKind::LeftBracket, line, column)) }
            ']' => { self.advance(); Ok(Token::new(TokenKind::RightBracket, line, column)) }
            ';' => { self.advance(); Ok(Token::new(TokenKind::Semicolon, line, column)) }
            ',' => { self.advance(); Ok(Token::new(TokenKind::Comma, line, column)) }
            '.' if self.peek_is_digit() => self.read_number(),
            '.' => { self.advance(); Ok(Token::new(TokenKind::Dot, line, column)) }
            ':' => { self.advance(); Ok(Token::new(TokenKind::Colon, line, column)) }
            '?' => { self.advance(); Ok(Token::new(TokenKind::Question, line, column)) }
            '=' if self.peek() == Some('>') => {
                self.advance(); // =
                self.advance(); // >
                Ok(Token::new(TokenKind::Arrow, line, column))
            }
            '+' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::PlusEqual, line, column))
                } else if self.current_char() == '+' {
                    self.advance();
                    Ok(Token::new(TokenKind::PlusPlus, line, column))
                } else {
                    Ok(Token::new(TokenKind::Plus, line, column))
                }
            }
            '-' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::MinusEqual, line, column))
                } else if self.current_char() == '-' {
                    self.advance();
                    Ok(Token::new(TokenKind::MinusMinus, line, column))
                } else {
                    Ok(Token::new(TokenKind::Minus, line, column))
                }
            }
            '*' => {
                self.advance();
                if self.current_char() == '*' {
                    self.advance();
                    if self.current_char() == '=' {
                        self.advance();
                        // **= is not in spec, treat as ** followed by =
                        Ok(Token::new(TokenKind::StarStar, line, column))
                    } else {
                        Ok(Token::new(TokenKind::StarStar, line, column))
                    }
                } else if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::StarEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Star, line, column))
                }
            }
            '/' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::SlashEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Slash, line, column))
                }
            }
            '%' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::PercentEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Percent, line, column))
                }
            }
            '=' => {
                self.advance();
                    if self.current_char() == '=' {
                        self.advance();
                        if self.current_char() == '=' {
                            self.advance();
                            Ok(Token::new(TokenKind::TripleEqual, line, column))
                        } else {
                            Ok(Token::new(TokenKind::EqualEqual, line, column))
                        }
                    } else {
                        Ok(Token::new(TokenKind::Equal, line, column))
                    }
            }
            '!' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::BangEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Bang, line, column))
                }
            }
            '>' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::GreaterEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Greater, line, column))
                }
            }
            '<' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::new(TokenKind::LessEqual, line, column))
                } else {
                    Ok(Token::new(TokenKind::Less, line, column))
                }
            }
            '&' if self.peek() == Some('&') => {
                self.advance();
                self.advance();
                Ok(Token::new(TokenKind::And, line, column))
            }
            '|' if self.peek() == Some('|') => {
                self.advance();
                self.advance();
                Ok(Token::new(TokenKind::Or, line, column))
            }
            '"' => self.read_string(),
            '0'..='9' => self.read_number(),
            _ if ch.is_alphabetic() || ch == '_' => self.read_identifier_or_keyword(),
            _ => Err(LexerError::new(
                &format!("Unexpected character '{}'", ch),
                line,
                column,
            )),
        }
    }

    fn read_string(&mut self) -> Result<Token, LexerError> {
        let line = self.line;
        let column = self.column;
        self.advance(); // consume opening quote

        let mut value = String::new();

        while !self.is_at_end() && self.current_char() != '"' {
            if self.current_char() == '\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(LexerError::new("Unterminated string escape", self.line, self.column));
                }
                let escaped = match self.current_char() {
                    'n' => '\n',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    _ => return Err(LexerError::new(
                        &format!("Invalid escape sequence '\\{}'", self.current_char()),
                        self.line,
                        self.column,
                    )),
                };
                value.push(escaped);
            } else {
                if self.current_char() == '\n' {
                    self.line += 1;
                    self.column = 1;
                } else {
                    self.column += 1;
                }
                value.push(self.current_char());
            }
            self.advance();
        }

        if self.is_at_end() {
            return Err(LexerError::new("Unterminated string", line, column));
        }

        self.advance(); // consume closing quote

        Ok(Token::new(TokenKind::StringLiteral(value), line, column))
    }

    fn read_number(&mut self) -> Result<Token, LexerError> {
        let line = self.line;
        let column = self.column;
        let mut value = String::new();
        let mut has_decimal = false;

        // Handle leading dot (e.g., .2)
        if !self.is_at_end() && self.current_char() == '.' {
            value.push('.');
            has_decimal = true;
            self.advance();
        }

        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            value.push(self.current_char());
            self.advance();
        }

        if !has_decimal && !self.is_at_end() && self.current_char() == '.' && self.peek_is_digit() {
            value.push('.');
            self.advance();

            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                value.push(self.current_char());
                self.advance();
            }
        }

        let number: f64 = value.parse().map_err(|_| {
            LexerError::new(&format!("Invalid number '{}'", value), line, column)
        })?;

        Ok(Token::new(TokenKind::Number(number), line, column))
    }

    fn read_identifier_or_keyword(&mut self) -> Result<Token, LexerError> {
        let line = self.line;
        let column = self.column;
        let mut value = String::new();

        while !self.is_at_end() && (self.current_char().is_alphanumeric() || self.current_char() == '_') {
            value.push(self.current_char());
            self.advance();
        }

        let kind = match value.as_str() {
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "switch" => TokenKind::Switch,
            "case" => TokenKind::Case,
            "default" => TokenKind::Default,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "finally" => TokenKind::Finally,
            "true" => TokenKind::Boolean(true),
            "false" => TokenKind::Boolean(false),
            "null" => TokenKind::Null,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            _ => TokenKind::Identifier(value),
        };

        Ok(Token::new(kind, line, column))
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            if self.is_at_end() {
                break;
            }
            match self.current_char() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                    self.advance();
                }
                '/' if self.peek() == Some('/') => {
                    // Single-line comment
                    while !self.is_at_end() && self.current_char() != '\n' {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn current_char(&self) -> char {
        self.source[self.position]
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.position];
        self.position += 1;
        if ch != '\n' {
            self.column += 1;
        }
        ch
    }

    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.source.len() {
            Some(self.source[self.position + 1])
        } else {
            None
        }
    }

    fn peek_is_digit(&self) -> bool {
        self.peek().map_or(false, |c| c.is_ascii_digit())
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.source.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_source() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("42 3.14 .5");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Number(42.0));
        assert_eq!(tokens[1].kind, TokenKind::Number(3.14));
        assert_eq!(tokens[2].kind, TokenKind::Number(0.5));
    }

    #[test]
    fn test_strings() {
        let mut lexer = Lexer::new(r#""hello" "world""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral("hello".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::StringLiteral("world".to_string()));
    }

    #[test]
    fn test_string_escape() {
        let mut lexer = Lexer::new(r#""line\nnewline""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral("line\nnewline".to_string()));
    }

    #[test]
    fn test_identifiers() {
        let mut lexer = Lexer::new("foo _bar baz123");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Identifier("foo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Identifier("_bar".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Identifier("baz123".to_string()));
    }

    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("let const function return if else while loop break continue switch case default try catch finally");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert_eq!(tokens[1].kind, TokenKind::Const);
        assert_eq!(tokens[2].kind, TokenKind::Function);
        assert_eq!(tokens[3].kind, TokenKind::Return);
        assert_eq!(tokens[4].kind, TokenKind::If);
        assert_eq!(tokens[5].kind, TokenKind::Else);
        assert_eq!(tokens[6].kind, TokenKind::While);
        assert_eq!(tokens[7].kind, TokenKind::Loop);
        assert_eq!(tokens[8].kind, TokenKind::Break);
        assert_eq!(tokens[9].kind, TokenKind::Continue);
        assert_eq!(tokens[10].kind, TokenKind::Switch);
        assert_eq!(tokens[11].kind, TokenKind::Case);
        assert_eq!(tokens[12].kind, TokenKind::Default);
        assert_eq!(tokens[13].kind, TokenKind::Try);
        assert_eq!(tokens[14].kind, TokenKind::Catch);
        assert_eq!(tokens[15].kind, TokenKind::Finally);
    }

    #[test]
    fn test_async_keywords() {
        let mut lexer = Lexer::new("async await");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Async);
        assert_eq!(tokens[1].kind, TokenKind::Await);
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn test_boolean_and_null() {
        let mut lexer = Lexer::new("true false null");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Boolean(true));
        assert_eq!(tokens[1].kind, TokenKind::Boolean(false));
        assert_eq!(tokens[2].kind, TokenKind::Null);
    }

    #[test]
    fn test_logical_keywords() {
        let mut lexer = Lexer::new("and or not");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::And);
        assert_eq!(tokens[1].kind, TokenKind::Or);
        assert_eq!(tokens[2].kind, TokenKind::Not);
    }

    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("+ - * / % ** = == != === > < >= <= += -= *= /= %= ++ -- ! ?:");
        let tokens = lexer.tokenize().unwrap();
        let expected = vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::StarStar,
            TokenKind::Equal,
            TokenKind::EqualEqual,
            TokenKind::BangEqual,
            TokenKind::TripleEqual,
            TokenKind::Greater,
            TokenKind::Less,
            TokenKind::GreaterEqual,
            TokenKind::LessEqual,
            TokenKind::PlusEqual,
            TokenKind::MinusEqual,
            TokenKind::StarEqual,
            TokenKind::SlashEqual,
            TokenKind::PercentEqual,
            TokenKind::PlusPlus,
            TokenKind::MinusMinus,
            TokenKind::Bang,
            TokenKind::Question,
            TokenKind::Colon,
        ];
        for (i, kind) in expected.iter().enumerate() {
            assert_eq!(&tokens[i].kind, kind, "Mismatch at index {}", i);
        }
    }

    #[test]
    fn test_punctuation() {
        let mut lexer = Lexer::new("( ) { } [ ] ; , .");
        let tokens = lexer.tokenize().unwrap();
        let expected = vec![
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            TokenKind::LeftBracket,
            TokenKind::RightBracket,
            TokenKind::Semicolon,
            TokenKind::Comma,
            TokenKind::Dot,
        ];
        for (i, kind) in expected.iter().enumerate() {
            assert_eq!(&tokens[i].kind, kind, "Mismatch at index {}", i);
        }
    }

    #[test]
    fn test_comments() {
        let mut lexer = Lexer::new("42 // this is a comment\n43");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Number(42.0));
        assert_eq!(tokens[1].kind, TokenKind::Number(43.0));
    }

    #[test]
    fn test_whitespace() {
        let mut lexer = Lexer::new("  42  \n  43  ");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Number(42.0));
        assert_eq!(tokens[1].kind, TokenKind::Number(43.0));
    }

    #[test]
    fn test_line_tracking() {
        let mut lexer = Lexer::new("42\n43\n44");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[2].line, 3);
    }

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        let result = lexer.tokenize();
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Unterminated string"));
    }

    #[test]
    fn test_unexpected_character() {
        let mut lexer = Lexer::new("@");
        let result = lexer.tokenize();
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Unexpected character"));
    }

    #[test]
    fn test_let_declaration() {
        let mut lexer = Lexer::new("let A = 10");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Equal);
        assert_eq!(tokens[3].kind, TokenKind::Number(10.0));
    }

    #[test]
    fn test_function_definition() {
        let mut lexer = Lexer::new("function test(A, B) { return A + B }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Function);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("test".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::LeftParen);
        assert_eq!(tokens[3].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::Comma);
        assert_eq!(tokens[5].kind, TokenKind::Identifier("B".to_string()));
        assert_eq!(tokens[6].kind, TokenKind::RightParen);
        assert_eq!(tokens[7].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[8].kind, TokenKind::Return);
        assert_eq!(tokens[9].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[10].kind, TokenKind::Plus);
        assert_eq!(tokens[11].kind, TokenKind::Identifier("B".to_string()));
        assert_eq!(tokens[12].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_if_else() {
        let mut lexer = Lexer::new("if A > 10 { } else { }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::If);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Greater);
        assert_eq!(tokens[3].kind, TokenKind::Number(10.0));
        assert_eq!(tokens[4].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[5].kind, TokenKind::RightBrace);
        assert_eq!(tokens[6].kind, TokenKind::Else);
        assert_eq!(tokens[7].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[8].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_while_loop() {
        let mut lexer = Lexer::new("while A < 10 { A++ }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::While);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Less);
        assert_eq!(tokens[3].kind, TokenKind::Number(10.0));
        assert_eq!(tokens[4].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[5].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[6].kind, TokenKind::PlusPlus);
        assert_eq!(tokens[7].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_arrow_function() {
        let mut lexer = Lexer::new("let add = (A, B) => A + B");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("add".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Equal);
        assert_eq!(tokens[3].kind, TokenKind::LeftParen);
        assert_eq!(tokens[4].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[5].kind, TokenKind::Comma);
        assert_eq!(tokens[6].kind, TokenKind::Identifier("B".to_string()));
        assert_eq!(tokens[7].kind, TokenKind::RightParen);
        assert_eq!(tokens[8].kind, TokenKind::Arrow);
        assert_eq!(tokens[9].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[10].kind, TokenKind::Plus);
        assert_eq!(tokens[11].kind, TokenKind::Identifier("B".to_string()));
    }

    #[test]
    fn test_list_literal() {
        let mut lexer = Lexer::new("[10, 20, 30]");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LeftBracket);
        assert_eq!(tokens[1].kind, TokenKind::Number(10.0));
        assert_eq!(tokens[2].kind, TokenKind::Comma);
        assert_eq!(tokens[3].kind, TokenKind::Number(20.0));
        assert_eq!(tokens[4].kind, TokenKind::Comma);
        assert_eq!(tokens[5].kind, TokenKind::Number(30.0));
        assert_eq!(tokens[6].kind, TokenKind::RightBracket);
    }

    #[test]
    fn test_object_literal() {
        let mut lexer = Lexer::new("{ name: \"Kael\", age: 18 }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("name".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Colon);
        assert_eq!(tokens[3].kind, TokenKind::StringLiteral("Kael".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::Comma);
        assert_eq!(tokens[5].kind, TokenKind::Identifier("age".to_string()));
        assert_eq!(tokens[6].kind, TokenKind::Colon);
        assert_eq!(tokens[7].kind, TokenKind::Number(18.0));
        assert_eq!(tokens[8].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_switch_case() {
        let mut lexer = Lexer::new("switch A { case 1 { } default { } }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Switch);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[3].kind, TokenKind::Case);
        assert_eq!(tokens[4].kind, TokenKind::Number(1.0));
        assert_eq!(tokens[5].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[6].kind, TokenKind::RightBrace);
        assert_eq!(tokens[7].kind, TokenKind::Default);
        assert_eq!(tokens[8].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[9].kind, TokenKind::RightBrace);
        assert_eq!(tokens[10].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_try_catch() {
        let mut lexer = Lexer::new("try { } catch e { } finally { }");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Try);
        assert_eq!(tokens[1].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[2].kind, TokenKind::RightBrace);
        assert_eq!(tokens[3].kind, TokenKind::Catch);
        assert_eq!(tokens[4].kind, TokenKind::Identifier("e".to_string()));
        assert_eq!(tokens[5].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[6].kind, TokenKind::RightBrace);
        assert_eq!(tokens[7].kind, TokenKind::Finally);
        assert_eq!(tokens[8].kind, TokenKind::LeftBrace);
        assert_eq!(tokens[9].kind, TokenKind::RightBrace);
    }

    #[test]
    fn test_string_unterminated_escape() {
        let mut lexer = Lexer::new(r#""hello\"#);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_escape() {
        let mut lexer = Lexer::new(r#""hello\x""#);
        let result = lexer.tokenize();
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Invalid escape sequence"));
    }

    #[test]
    fn test_negative_number_with_operator() {
        let mut lexer = Lexer::new("-10");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Minus);
        assert_eq!(tokens[1].kind, TokenKind::Number(10.0));
    }

    #[test]
    fn test_compound_assignment() {
        let mut lexer = Lexer::new("A += 5 A -= 3 A *= 2 A /= 4 A %= 3");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::PlusEqual);
        assert_eq!(tokens[2].kind, TokenKind::Number(5.0));
        assert_eq!(tokens[3].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::MinusEqual);
        assert_eq!(tokens[5].kind, TokenKind::Number(3.0));
        assert_eq!(tokens[6].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[7].kind, TokenKind::StarEqual);
        assert_eq!(tokens[8].kind, TokenKind::Number(2.0));
        assert_eq!(tokens[9].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[10].kind, TokenKind::SlashEqual);
        assert_eq!(tokens[11].kind, TokenKind::Number(4.0));
        assert_eq!(tokens[12].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[13].kind, TokenKind::PercentEqual);
        assert_eq!(tokens[14].kind, TokenKind::Number(3.0));
    }

    #[test]
    fn test_equality_operators() {
        let mut lexer = Lexer::new("A == B A != B A === B");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::EqualEqual);
        assert_eq!(tokens[2].kind, TokenKind::Identifier("B".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::BangEqual);
        assert_eq!(tokens[5].kind, TokenKind::Identifier("B".to_string()));
        assert_eq!(tokens[6].kind, TokenKind::Identifier("A".to_string()));
        assert_eq!(tokens[7].kind, TokenKind::TripleEqual);
        assert_eq!(tokens[8].kind, TokenKind::Identifier("B".to_string()));
    }

    #[test]
    fn test_string_with_newline() {
        let mut lexer = Lexer::new("\"line1\nline2\"");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral("line1\nline2".to_string()));
    }
}
