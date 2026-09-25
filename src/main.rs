mod ast;
mod compiler;
mod error;
mod gc;
mod heap;
mod json;
mod lexer;
mod module;
mod parser;
mod runtime_value;
mod token;
mod vm;

use compiler::compile;
use lexer::Lexer;
use parser::Parser;
use vm::run_bytecode;

fn run_source(source: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse()?;
    let bytecode = compile(&program)?;
    let output = run_bytecode(bytecode)?;
    Ok(output)
}

fn main() {
    println!("ZeroScript VM v0.1.0");
}
