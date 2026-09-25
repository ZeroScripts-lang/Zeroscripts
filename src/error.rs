use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl LexerError {
    pub fn new(message: &str, line: usize, column: usize) -> Self {
        Self {
            message: message.to_string(),
            line,
            column,
        }
    }
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Lexer error at {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl Error for LexerError {}

#[derive(Debug, Clone, PartialEq)]
pub struct ParserError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl ParserError {
    pub fn new(message: &str, line: usize, column: usize) -> Self {
        Self {
            message: message.to_string(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parser error at {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl Error for ParserError {}

#[derive(Debug, Clone, PartialEq)]
pub struct CompilerError {
    pub message: String,
}

impl CompilerError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Compiler error: {}", self.message)
    }
}

impl Error for CompilerError {}
