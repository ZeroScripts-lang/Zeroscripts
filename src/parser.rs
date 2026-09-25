use crate::ast::*;
use crate::error::ParserError;
use crate::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
    /// True only while parsing top-level statements of a file (§78: import
    /// must be at module level, not inside function/if/while/block).
    module_level: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            module_level: false,
        }
    }

    pub fn parse(&mut self) -> Result<Program, ParserError> {
        let mut statements = Vec::new();
        self.module_level = true;

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParserError> {
        match &self.current().kind {
            TokenKind::Let | TokenKind::Const => self.parse_variable_declaration(false),
            TokenKind::Function => self.parse_function_declaration(false, false),
            TokenKind::Async => {
                self.advance(); // consume async
                if !self.check(&TokenKind::Function) {
                    return Err(self.error("Expected 'function' after 'async'"));
                }
                self.parse_function_declaration(false, true)
            }
            TokenKind::Import => self.parse_import(),
            TokenKind::Export => self.parse_export(),
            TokenKind::Return => self.parse_return(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Switch => self.parse_switch(),
            TokenKind::Try => self.parse_try(),
            _ => self.parse_expression_statement(),
        }
    }

    /// `import { name, name2 as alias } from "./path"` (§46, §69, §70).
    /// Module-level only (§78).
    fn parse_import(&mut self) -> Result<Stmt, ParserError> {
        if !self.module_level {
            return Err(self.error(
                "import must be at module level (not inside function, if, while, or block)",
            ));
        }
        let span = self.current_span();
        self.advance(); // consume import

        self.expect(&TokenKind::LeftBrace)?;
        let mut names = Vec::new();
        if !self.check(&TokenKind::RightBrace) {
            loop {
                let export_name = self.expect_identifier()?;
                let local_name = if let TokenKind::Identifier(s) = &self.current().kind {
                    if s == "as" {
                        self.advance(); // consume 'as'
                        self.expect_identifier()?
                    } else {
                        export_name.clone()
                    }
                } else {
                    export_name.clone()
                };
                names.push((export_name, local_name));
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RightBrace) {
                        break; // trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RightBrace)?;

        // `from` is a contextual identifier, not a keyword token
        match &self.current().kind {
            TokenKind::Identifier(s) if s == "from" => {
                self.advance();
            }
            _ => {
                return Err(self.error("Expected 'from' in import statement"));
            }
        }

        let path = match &self.current().kind {
            TokenKind::StringLiteral(p) => {
                let p = p.clone();
                self.advance();
                p
            }
            _ => {
                return Err(self.error("Expected string path after 'from' in import"));
            }
        };

        Ok(Stmt::Import { names, path, span })
    }

    /// Handles `export function`, `export const`/`export let`,
    /// `export { a, b }`, `export { a } from "./path"` (§47, §67).
    fn parse_export(&mut self) -> Result<Stmt, ParserError> {
        if !self.module_level {
            return Err(self.error("export must be at module level"));
        }
        let span = self.current_span();
        self.advance(); // consume export

        match &self.current().kind {
            TokenKind::Function => self.parse_function_declaration(true, false),
            TokenKind::Async => {
                self.advance(); // consume async
                if !self.check(&TokenKind::Function) {
                    return Err(self.error("Expected 'function' after 'async'"));
                }
                self.parse_function_declaration(true, true)
            }
            TokenKind::Let | TokenKind::Const => self.parse_variable_declaration(true),
            TokenKind::LeftBrace => {
                self.advance(); // consume {
                let mut names = Vec::new();
                if !self.check(&TokenKind::RightBrace) {
                    loop {
                        names.push(self.expect_identifier()?);
                        if self.check(&TokenKind::Comma) {
                            self.advance();
                            if self.check(&TokenKind::RightBrace) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RightBrace)?;

                // Optional re-export: `from "./path"` (§67)
                let from = if let TokenKind::Identifier(s) = &self.current().kind {
                    if s == "from" {
                        self.advance(); // consume 'from'
                        match &self.current().kind {
                            TokenKind::StringLiteral(p) => {
                                let p = p.clone();
                                self.advance();
                                Some(p)
                            }
                            _ => {
                                return Err(
                                    self.error("Expected string path after 'from' in export")
                                );
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(Stmt::ExportList { names, from, span })
            }
            _ => Err(self.error(
                "Expected 'function', 'async function', 'let', 'const', or '{' after 'export'",
            )),
        }
    }

    fn parse_variable_declaration(&mut self, exported: bool) -> Result<Stmt, ParserError> {
        let is_const = self.current().kind == TokenKind::Const;
        let span = self.current_span();
        self.advance(); // consume let/const

        let name = self.expect_identifier()?;
        let initializer = if self.check(&TokenKind::Equal) {
            self.advance(); // consume =
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Stmt::VariableDecl {
            name,
            is_const,
            initializer,
            exported,
            span,
        })
    }

    fn parse_function_declaration(&mut self, exported: bool, is_async: bool) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume function

        let name = self.expect_identifier()?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;

        Ok(Stmt::FunctionDecl {
            name,
            params,
            body,
            exported,
            is_async,
            span,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume return

        let value = if self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
            || self.at_statement_end()
        {
            None
        } else {
            Some(self.parse_expression()?)
        };

        Ok(Stmt::Return { value, span })
    }

    fn parse_if(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume if

        let condition = self.parse_expression()?;
        let then_body = self.parse_block()?;

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance(); // consume else
            if self.check(&TokenKind::If) {
                let else_if = self.parse_if()?;
                match else_if {
                    Stmt::If {
                        condition,
                        then_body,
                        else_branch,
                        ..
                    } => Some(ElseBranch::If {
                        condition,
                        then_body,
                        else_branch: else_branch.map(Box::new),
                    }),
                    _ => unreachable!(),
                }
            } else {
                let body = self.parse_block()?;
                Some(ElseBranch::Block(body))
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_body,
            else_branch,
            span,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume while

        let condition = self.parse_expression()?;
        let body = self.parse_block()?;

        Ok(Stmt::While {
            condition,
            body,
            span,
        })
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume loop

        let body = self.parse_block()?;

        Ok(Stmt::Loop { body, span })
    }

    fn parse_break(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume break
        Ok(Stmt::Break { span })
    }

    fn parse_continue(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume continue
        Ok(Stmt::Continue { span })
    }

    fn parse_switch(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume switch

        let value = self.parse_expression()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut cases = Vec::new();
        let mut default_case = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if self.check(&TokenKind::Case) {
                self.advance(); // consume case
                let case_value = self.parse_expression()?;
                let body = self.parse_block()?;
                cases.push(SwitchCase {
                    value: case_value,
                    body,
                });
            } else if self.check(&TokenKind::Default) {
                self.advance(); // consume default
                let body = self.parse_block()?;
                default_case = Some(body);
            } else {
                return Err(self.error("Expected 'case' or 'default' in switch"));
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(Stmt::Switch {
            value,
            cases,
            default_case,
            span,
        })
    }

    fn parse_try(&mut self) -> Result<Stmt, ParserError> {
        let span = self.current_span();
        self.advance(); // consume try

        let body = self.parse_block()?;

        let (catch_param, catch_body) = if self.check(&TokenKind::Catch) {
            self.advance(); // consume catch
            let param = self.expect_identifier()?;
            let body = self.parse_block()?;
            (Some(param), Some(body))
        } else {
            (None, None)
        };

        let finally_body = if self.check(&TokenKind::Finally) {
            self.advance(); // consume finally
            Some(self.parse_block()?)
        } else {
            None
        };

        Ok(Stmt::Try {
            body,
            catch_param,
            catch_body,
            finally_body,
            span,
        })
    }

    fn parse_expression_statement(&mut self) -> Result<Stmt, ParserError> {
        let expr = self.parse_expression()?;
        Ok(Stmt::Expression(expr))
    }

    fn parse_expression(&mut self) -> Result<Expr, ParserError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParserError> {
        let condition = self.parse_assignment()?;

        if self.check(&TokenKind::Question) {
            let span = condition.span().clone();
            self.advance(); // consume ?
            let then_branch = self.parse_expression()?;
            self.expect(&TokenKind::Colon)?;
            let else_branch = self.parse_expression()?;
            return Ok(Expr::Ternary {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
                span,
            });
        }

        Ok(condition)
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParserError> {
        let expr = self.parse_or()?;

        if self.check(&TokenKind::Equal) {
            let span = expr.span().clone();
            self.advance(); // consume =
            let value = self.parse_assignment()?; // right-associative
            return Ok(Expr::Assignment {
                target: Box::new(expr),
                value: Box::new(value),
                span,
            });
        }

        if let Some(op) = self.check_compound_assignment() {
            let span = expr.span().clone();
            self.advance(); // consume op
            let value = self.parse_assignment()?;
            return Ok(Expr::CompoundAssignment {
                op,
                target: Box::new(expr),
                value: Box::new(value),
                span,
            });
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_and()?;

        while self.check(&TokenKind::Or) {
            let span = left.span().clone();
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_equality()?;

        while self.check(&TokenKind::And) {
            let span = left.span().clone();
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_comparison()?;

        loop {
            let op = match &self.current().kind {
                TokenKind::EqualEqual => BinaryOp::Equal,
                TokenKind::BangEqual => BinaryOp::NotEqual,
                TokenKind::TripleEqual => BinaryOp::RawEqual,
                _ => break,
            };
            let span = left.span().clone();
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_addition()?;

        loop {
            let op = match &self.current().kind {
                TokenKind::Greater => BinaryOp::Greater,
                TokenKind::Less => BinaryOp::Less,
                TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
                TokenKind::LessEqual => BinaryOp::LessEqual,
                _ => break,
            };
            let span = left.span().clone();
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_multiplication()?;

        loop {
            let op = match &self.current().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                _ => break,
            };
            let span = left.span().clone();
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParserError> {
        let mut left = self.parse_power()?;

        loop {
            let op = match &self.current().kind {
                TokenKind::Star => BinaryOp::Multiply,
                TokenKind::Slash => BinaryOp::Divide,
                TokenKind::Percent => BinaryOp::Modulo,
                _ => break,
            };
            let span = left.span().clone();
            self.advance();
            let right = self.parse_power()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, ParserError> {
        let base = self.parse_unary()?;

        if self.check(&TokenKind::StarStar) {
            let span = base.span().clone();
            self.advance();
            let exponent = self.parse_power()?; // right-associative
            return Ok(Expr::Binary {
                op: BinaryOp::Power,
                left: Box::new(base),
                right: Box::new(exponent),
                span,
            });
        }

        Ok(base)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParserError> {
        match &self.current().kind {
            TokenKind::Await => {
                let span = self.current_span();
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Await {
                    expr: Box::new(operand),
                    span,
                })
            }
            TokenKind::Minus => {
                let span = self.current_span();
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Plus => {
                let span = self.current_span();
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Plus,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Bang | TokenKind::Not => {
                let span = self.current_span();
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParserError> {
        let mut expr = self.parse_call_member_index()?;

        loop {
            match &self.current().kind {
                TokenKind::PlusPlus => {
                    let span = expr.span().clone();
                    self.advance();
                    expr = Expr::Postfix {
                        op: PostfixOp::Increment,
                        operand: Box::new(expr),
                        span,
                    };
                }
                TokenKind::MinusMinus => {
                    let span = expr.span().clone();
                    self.advance();
                    expr = Expr::Postfix {
                        op: PostfixOp::Decrement,
                        operand: Box::new(expr),
                        span,
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_member_index(&mut self) -> Result<Expr, ParserError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(&TokenKind::LeftParen) {
                // Function call
                let span = expr.span().clone();
                self.advance(); // consume (
                let args = self.parse_arguments()?;
                self.expect(&TokenKind::RightParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.check(&TokenKind::Dot) {
                // Member access
                let span = expr.span().clone();
                self.advance(); // consume .
                let property = self.expect_identifier()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    property,
                    span,
                };
            } else if self.check(&TokenKind::LeftBracket) {
                // Index access
                let span = expr.span().clone();
                self.advance(); // consume [
                let index = self.parse_expression()?;
                self.expect(&TokenKind::RightBracket)?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParserError> {
        let token = self.current().clone();
        let span = self.current_span();

        match &token.kind {
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::NumberLiteral(n, span))
            }
            TokenKind::StringLiteral(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::StringLiteral(s, span))
            }
            TokenKind::Boolean(b) => {
                let b = *b;
                self.advance();
                Ok(Expr::BooleanLiteral(b, span))
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::NullLiteral(span))
            }
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                // Check if this is a single-param arrow function: name =>
                if self.check(&TokenKind::Arrow) {
                    return self.parse_arrow_function_from_params(
                        vec![FunctionParam {
                            name,
                            default_value: None,
                        }],
                        span,
                    );
                }
                Ok(Expr::Identifier(name, span))
            }
            TokenKind::LeftParen => {
                // Check if this might be an arrow function: (A, B) => ...
                if self.looks_like_arrow_function() {
                    self.parse_arrow_function_parenthesized(span)
                } else {
                    // Parenthesized expression
                    self.advance(); // consume (
                    let expr = self.parse_expression()?;
                    self.expect(&TokenKind::RightParen)?;
                    Ok(expr)
                }
            }
            TokenKind::LeftBracket => self.parse_list(),
            TokenKind::LeftBrace => self.parse_object(),
            TokenKind::Function => self.parse_function_expression(),
            _ => Err(self.error(&format!(
                "Unexpected token '{}'",
                token.kind
            ))),
        }
    }

    fn looks_like_arrow_function(&self) -> bool {
        // Quick check: look for pattern ( ... ) =>
        if !self.check(&TokenKind::LeftParen) {
            return false;
        }

        let mut i = self.position + 1;
        let mut depth = 1;

        // Scan forward to find matching )
        while i < self.tokens.len() && depth > 0 {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if followed by =>
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::Arrow;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }

        false
    }

    fn parse_arrow_function_parenthesized(&mut self, _span: Span) -> Result<Expr, ParserError> {
        self.advance(); // consume (
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RightParen)?;
        let span = self.current_span();
        self.parse_arrow_function_from_params(params, span)
    }

    fn parse_arrow_function_from_params(
        &mut self,
        params: Vec<FunctionParam>,
        span: Span,
    ) -> Result<Expr, ParserError> {
        self.expect(&TokenKind::Arrow)?;

        if self.check(&TokenKind::LeftBrace) {
            let body = self.parse_block()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Block(body),
                span,
            })
        } else {
            let expr = self.parse_expression()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Expression(Box::new(expr)),
                span,
            })
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParserError> {
        let span = self.current_span();
        self.advance(); // consume [

        let mut elements = Vec::new();

        if !self.check(&TokenKind::RightBracket) {
            elements.push(self.parse_expression()?);
            while self.check(&TokenKind::Comma) {
                self.advance(); // consume ,
                if self.check(&TokenKind::RightBracket) {
                    break; // trailing comma
                }
                elements.push(self.parse_expression()?);
            }
        }

        self.expect(&TokenKind::RightBracket)?;

        Ok(Expr::List { elements, span })
    }

    fn parse_object(&mut self) -> Result<Expr, ParserError> {
        let span = self.current_span();
        self.advance(); // consume {

        let mut properties = Vec::new();

        if !self.check(&TokenKind::RightBrace) {
            self.parse_object_property(&mut properties)?;
            while self.check(&TokenKind::Comma) {
                self.advance(); // consume ,
                if self.check(&TokenKind::RightBrace) {
                    break; // trailing comma
                }
                self.parse_object_property(&mut properties)?;
            }
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(Expr::Object { properties, span })
    }

    fn parse_object_property(&mut self, properties: &mut Vec<ObjectProperty>) -> Result<(), ParserError> {
        let key = match &self.current().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                name
            }
            TokenKind::StringLiteral(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => return Err(self.error("Expected property key (identifier or string)")),
        };

        if properties.iter().any(|p| p.key == key) {
            return Err(self.error(&format!("Duplicate object key '{}'", key)));
        }

        self.expect(&TokenKind::Colon)?;
        let value = self.parse_expression()?;

        properties.push(ObjectProperty { key, value });
        Ok(())
    }

    fn parse_function_expression(&mut self) -> Result<Expr, ParserError> {
        let span = self.current_span();
        self.advance(); // consume function

        let params = self.parse_params()?;
        let body = self.parse_block()?;

        Ok(Expr::Function {
            params,
            body,
            span,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<FunctionParam>, ParserError> {
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RightParen)?;
        Ok(params)
    }

    fn parse_param_list(&mut self) -> Result<Vec<FunctionParam>, ParserError> {
        let mut params = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            params.push(self.parse_param()?);
            while self.check(&TokenKind::Comma) {
                self.advance(); // consume ,
                if self.check(&TokenKind::RightParen) {
                    break; // trailing comma
                }
                params.push(self.parse_param()?);
            }
        }

        Ok(params)
    }

    fn parse_param(&mut self) -> Result<FunctionParam, ParserError> {
        let name = self.expect_identifier()?;

        let default_value = if self.check(&TokenKind::Equal) {
            self.advance(); // consume =
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(FunctionParam {
            name,
            default_value,
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParserError> {
        self.expect(&TokenKind::LeftBrace)?;

        // Blocks are never module level (§78: import only at module level)
        let saved_module_level = self.module_level;
        self.module_level = false;

        let mut statements = Vec::new();
        let mut result = Ok(());

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }

        self.module_level = saved_module_level;

        if let Err(e) = result {
            return Err(e);
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(statements)
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParserError> {
        let mut args = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            // Check if this might be an arrow function: (a, b) =>
            // We need to handle this case specially
            args.push(self.parse_expression()?);
            while self.check(&TokenKind::Comma) {
                self.advance(); // consume ,
                if self.check(&TokenKind::RightParen) {
                    break; // trailing comma
                }
                args.push(self.parse_expression()?);
            }
        }

        Ok(args)
    }

    // Helper methods

    fn current(&self) -> Token {
        if self.position < self.tokens.len() {
            self.tokens[self.position].clone()
        } else {
            Token::new(TokenKind::Eof, 0, 0)
        }
    }

    fn current_span(&self) -> Span {
        let token = self.current();
        Span {
            line: token.line,
            column: token.column,
        }
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.position].clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.current().kind == kind
    }

    fn is_at_end(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParserError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!(
                "Expected '{}', got '{}'",
                kind,
                self.current().kind
            )))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParserError> {
        if let TokenKind::Identifier(name) = &self.current().kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(self.error(&format!(
                "Expected identifier, got '{}'",
                self.current().kind
            )))
        }
    }

    fn check_compound_assignment(&self) -> Option<CompoundAssignOp> {
        match &self.current().kind {
            TokenKind::PlusEqual => Some(CompoundAssignOp::Add),
            TokenKind::MinusEqual => Some(CompoundAssignOp::Subtract),
            TokenKind::StarEqual => Some(CompoundAssignOp::Multiply),
            TokenKind::SlashEqual => Some(CompoundAssignOp::Divide),
            TokenKind::PercentEqual => Some(CompoundAssignOp::Modulo),
            _ => None,
        }
    }

    fn at_statement_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RightBrace | TokenKind::Eof
        )
    }

    fn error(&self, message: &str) -> ParserError {
        let token = self.current();
        ParserError::new(message, token.line, token.column)
    }
}

// Extension trait to get span from expressions
impl Expr {
    pub fn span(&self) -> &Span {
        match self {
            Expr::NumberLiteral(_, span) => span,
            Expr::StringLiteral(_, span) => span,
            Expr::BooleanLiteral(_, span) => span,
            Expr::NullLiteral(span) => span,
            Expr::Identifier(_, span) => span,
            Expr::Unary { span, .. } => span,
            Expr::Binary { span, .. } => span,
            Expr::Assignment { span, .. } => span,
            Expr::CompoundAssignment { span, .. } => span,
            Expr::Postfix { span, .. } => span,
            Expr::Call { span, .. } => span,
            Expr::Member { span, .. } => span,
            Expr::Index { span, .. } => span,
            Expr::List { span, .. } => span,
            Expr::Object { span, .. } => span,
            Expr::Function { span, .. } => span,
            Expr::ArrowFunction { span, .. } => span,
            Expr::Ternary { span, .. } => span,
            Expr::Await { span, .. } => span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> Program {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse().unwrap()
    }

    #[test]
    fn test_empty_program() {
        let prog = parse("");
        assert!(prog.statements.is_empty());
    }

    #[test]
    fn test_let_declaration() {
        let prog = parse("let A = 10");
        assert_eq!(prog.statements.len(), 1);
        match &prog.statements[0] {
            Stmt::VariableDecl {
                name,
                is_const,
                initializer,
                ..
            } => {
                assert_eq!(name, "A");
                assert!(!is_const);
                assert!(initializer.is_some());
                match initializer.as_ref().unwrap() {
                    Expr::NumberLiteral(n, _) => assert_eq!(*n, 10.0),
                    _ => panic!("Expected number literal"),
                }
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn test_const_declaration() {
        let prog = parse("const A = 10");
        match &prog.statements[0] {
            Stmt::VariableDecl { is_const, .. } => assert!(*is_const),
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn test_const_without_initializer() {
        let prog = parse("const A");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                name,
                initializer, ..
            } => {
                assert_eq!(name, "A");
                assert!(initializer.is_none());
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn test_number_literal() {
        let prog = parse("42");
        match &prog.statements[0] {
            Stmt::Expression(Expr::NumberLiteral(n, _)) => assert_eq!(*n, 42.0),
            _ => panic!("Expected number expression"),
        }
    }

    #[test]
    fn test_string_literal() {
        let prog = parse(r#""hello""#);
        match &prog.statements[0] {
            Stmt::Expression(Expr::StringLiteral(s, _)) => assert_eq!(s, "hello"),
            _ => panic!("Expected string expression"),
        }
    }

    #[test]
    fn test_boolean_literal() {
        let prog = parse("true false");
        assert_eq!(prog.statements.len(), 2);
        match &prog.statements[0] {
            Stmt::Expression(Expr::BooleanLiteral(b, _)) => assert!(*b),
            _ => panic!("Expected boolean expression"),
        }
        match &prog.statements[1] {
            Stmt::Expression(Expr::BooleanLiteral(b, _)) => assert!(!*b),
            _ => panic!("Expected boolean expression"),
        }
    }

    #[test]
    fn test_null_literal() {
        let prog = parse("null");
        match &prog.statements[0] {
            Stmt::Expression(Expr::NullLiteral(_)) => {}
            _ => panic!("Expected null expression"),
        }
    }

    #[test]
    fn test_identifier() {
        let prog = parse("foo");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Identifier(name, _)) => assert_eq!(name, "foo"),
            _ => panic!("Expected identifier expression"),
        }
    }

    #[test]
    fn test_binary_add() {
        let prog = parse("1 + 2");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, left, right, .. }) => {
                assert_eq!(*op, BinaryOp::Add);
                assert!(matches!(left.as_ref(), Expr::NumberLiteral(1.0, _)));
                assert!(matches!(right.as_ref(), Expr::NumberLiteral(2.0, _)));
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_binary_subtract() {
        let prog = parse("5 - 3");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Subtract);
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_binary_multiply() {
        let prog = parse("3 * 4");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Multiply);
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_binary_divide() {
        let prog = parse("10 / 2");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Divide);
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_binary_modulo() {
        let prog = parse("10 % 3");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Modulo);
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_binary_power() {
        let prog = parse("2 ** 3");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, left, right, .. }) => {
                assert_eq!(*op, BinaryOp::Power);
                assert!(matches!(left.as_ref(), Expr::NumberLiteral(2.0, _)));
                assert!(matches!(right.as_ref(), Expr::NumberLiteral(3.0, _)));
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_power_right_associative() {
        let prog = parse("2 ** 3 ** 2");
        // Should be 2 ** (3 ** 2) = 2 ** 9 = 512
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Power,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::NumberLiteral(2.0, _)));
                match right.as_ref() {
                    Expr::Binary {
                        op: BinaryOp::Power,
                        ..
                    } => {}
                    _ => panic!("Expected right-associative power"),
                }
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test]
    fn test_precedence_mul_before_add() {
        let prog = parse("2 + 3 * 4");
        // Should be 2 + (3 * 4) = 14
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::NumberLiteral(2.0, _)));
                match right.as_ref() {
                    Expr::Binary {
                        op: BinaryOp::Multiply,
                        ..
                    } => {}
                    _ => panic!("Expected multiplication on right"),
                }
            }
            _ => panic!("Expected addition"),
        }
    }

    #[test]
    fn test_precedence_power_before_mul() {
        let prog = parse("2 * 3 ** 2");
        // Should be 2 * (3 ** 2) = 18
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Multiply,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::NumberLiteral(2.0, _)));
                match right.as_ref() {
                    Expr::Binary {
                        op: BinaryOp::Power,
                        ..
                    } => {}
                    _ => panic!("Expected power on right"),
                }
            }
            _ => panic!("Expected multiplication"),
        }
    }

    #[test]
    fn test_comparison() {
        let prog = parse("10 > 5");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Greater);
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_equality() {
        let prog = parse("10 == 10");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Equal);
            }
            _ => panic!("Expected equality"),
        }
    }

    #[test]
    fn test_strict_equality() {
        let prog = parse("10 === 10");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::RawEqual);
            }
            _ => panic!("Expected raw equality"),
        }
    }

    #[test]
    fn test_boolean_and() {
        let prog = parse("true and false");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::And);
            }
            _ => panic!("Expected and expression"),
        }
    }

    #[test]
    fn test_boolean_or() {
        let prog = parse("true or false");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op, .. }) => {
                assert_eq!(*op, BinaryOp::Or);
            }
            _ => panic!("Expected or expression"),
        }
    }

    #[test]
    fn test_precedence_and_before_or() {
        let prog = parse("true or false and true");
        // Should be true or (false and true)
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Or,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::BooleanLiteral(true, _)));
                match right.as_ref() {
                    Expr::Binary {
                        op: BinaryOp::And,
                        ..
                    } => {}
                    _ => panic!("Expected and on right"),
                }
            }
            _ => panic!("Expected or expression"),
        }
    }

    #[test]
    fn test_unary_negate() {
        let prog = parse("-10");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Unary { op, operand, .. }) => {
                assert_eq!(*op, UnaryOp::Negate);
                assert!(matches!(operand.as_ref(), Expr::NumberLiteral(10.0, _)));
            }
            _ => panic!("Expected unary expression"),
        }
    }

    #[test]
    fn test_unary_not() {
        let prog = parse("!true");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Unary { op, operand, .. }) => {
                assert_eq!(*op, UnaryOp::Not);
                assert!(matches!(operand.as_ref(), Expr::BooleanLiteral(true, _)));
            }
            _ => panic!("Expected unary expression"),
        }
    }

    #[test]
    fn test_assignment() {
        let prog = parse("A = 10");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Assignment { target, value, .. }) => {
                assert!(matches!(target.as_ref(), Expr::Identifier(name, _) if name == "A"));
                assert!(matches!(value.as_ref(), Expr::NumberLiteral(10.0, _)));
            }
            _ => panic!("Expected assignment expression"),
        }
    }

    #[test]
    fn test_compound_assignment() {
        let prog = parse("A += 5");
        match &prog.statements[0] {
            Stmt::Expression(Expr::CompoundAssignment { op, target, value, .. }) => {
                assert_eq!(*op, CompoundAssignOp::Add);
                assert!(matches!(target.as_ref(), Expr::Identifier(name, _) if name == "A"));
                assert!(matches!(value.as_ref(), Expr::NumberLiteral(5.0, _)));
            }
            _ => panic!("Expected compound assignment"),
        }
    }

    #[test]
    fn test_postfix_increment() {
        let prog = parse("A++");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Postfix { op, operand, .. }) => {
                assert_eq!(*op, PostfixOp::Increment);
                assert!(matches!(operand.as_ref(), Expr::Identifier(name, _) if name == "A"));
            }
            _ => panic!("Expected postfix expression"),
        }
    }

    #[test]
    fn test_postfix_decrement() {
        let prog = parse("A--");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Postfix { op, operand, .. }) => {
                assert_eq!(*op, PostfixOp::Decrement);
                assert!(matches!(operand.as_ref(), Expr::Identifier(name, _) if name == "A"));
            }
            _ => panic!("Expected postfix expression"),
        }
    }

    #[test]
    fn test_function_call() {
        let prog = parse("add(1, 2)");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Call { callee, args, .. }) => {
                assert!(matches!(callee.as_ref(), Expr::Identifier(name, _) if name == "add"));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_member_access() {
        let prog = parse("user.name");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Member { object, property, .. }) => {
                assert!(matches!(object.as_ref(), Expr::Identifier(name, _) if name == "user"));
                assert_eq!(property, "name");
            }
            _ => panic!("Expected member access"),
        }
    }

    #[test]
    fn test_index_access() {
        let prog = parse("arr[0]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Index { object, index, .. }) => {
                assert!(matches!(object.as_ref(), Expr::Identifier(name, _) if name == "arr"));
                assert!(matches!(index.as_ref(), Expr::NumberLiteral(0.0, _)));
            }
            _ => panic!("Expected index access"),
        }
    }

    #[test]
    fn test_list_literal() {
        let prog = parse("[10, 20, 30]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::List { elements, .. }) => {
                assert_eq!(elements.len(), 3);
            }
            _ => panic!("Expected list literal"),
        }
    }

    #[test]
    fn test_empty_list() {
        let prog = parse("[]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::List { elements, .. }) => {
                assert!(elements.is_empty());
            }
            _ => panic!("Expected empty list"),
        }
    }

    #[test]
    fn test_object_literal() {
        let prog = parse(r#"{ name: "Kael", age: 18 }"#);
        match &prog.statements[0] {
            Stmt::Expression(Expr::Object { properties, .. }) => {
                assert_eq!(properties.len(), 2);
                assert_eq!(properties[0].key, "name");
                assert_eq!(properties[1].key, "age");
            }
            _ => panic!("Expected object literal"),
        }
    }

    #[test]
    fn test_object_with_string_key() {
        let prog = parse(r#"{ "name": "Kael" }"#);
        match &prog.statements[0] {
            Stmt::Expression(Expr::Object { properties, .. }) => {
                assert_eq!(properties[0].key, "name");
            }
            _ => panic!("Expected object literal"),
        }
    }

    #[test]
    fn test_function_declaration() {
        let prog = parse("function test(A, B) { return A + B }");
        match &prog.statements[0] {
            Stmt::FunctionDecl {
                name,
                params,
                body,
                ..
            } => {
                assert_eq!(name, "test");
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, "A");
                assert_eq!(params[1].name, "B");
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_function_with_default_params() {
        let prog = parse(r#"function test(A = 10) { return A }"#);
        match &prog.statements[0] {
            Stmt::FunctionDecl { params, .. } => {
                assert_eq!(params.len(), 1);
                assert!(params[0].default_value.is_some());
            }
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_async_function_declaration() {
        let prog = parse("async function ambilData() { return 1 }");
        match &prog.statements[0] {
            Stmt::FunctionDecl {
                name, is_async, ..
            } => {
                assert_eq!(name, "ambilData");
                assert!(*is_async);
            }
            _ => panic!("Expected async function declaration"),
        }
        // Plain function stays non-async.
        let prog = parse("function plain() { return 1 }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { is_async, .. } => assert!(!*is_async),
            _ => panic!("Expected function declaration"),
        }
        // export async function (§47 + §49).
        let prog = parse("export async function exported() { return 1 }");
        match &prog.statements[0] {
            Stmt::FunctionDecl {
                exported,
                is_async,
                ..
            } => {
                assert!(*exported);
                assert!(*is_async);
            }
            _ => panic!("Expected exported async function declaration"),
        }
    }

    #[test]
    fn test_await_expression() {
        let prog = parse("async function main() { let x = await other() }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { body, .. } => match &body[0] {
                Stmt::VariableDecl {
                    initializer: Some(Expr::Await { expr, .. }),
                    ..
                } => {
                    assert!(matches!(**expr, Expr::Call { .. }));
                }
                other => panic!("Expected await expression, got {:?}", other),
            },
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_function_expression() {
        let prog = parse("let f = function(A) { return A }");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                initializer: Some(Expr::Function { params, body, .. }),
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected function expression"),
        }
    }

    #[test]
    fn test_arrow_function() {
        let prog = parse("let add = (A, B) => A + B");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                initializer: Some(Expr::ArrowFunction { params, body, .. }),
                ..
            } => {
                assert_eq!(params.len(), 2);
                assert!(matches!(body, ArrowBody::Expression(_)));
            }
            _ => panic!("Expected arrow function"),
        }
    }

    #[test]
    fn test_arrow_function_single_param() {
        let prog = parse("let double = A => A * 2");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                initializer: Some(Expr::ArrowFunction { params, body, .. }),
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert!(matches!(body, ArrowBody::Expression(_)));
            }
            _ => panic!("Expected arrow function"),
        }
    }

    #[test]
    fn test_arrow_function_no_params() {
        let prog = parse("let hello = () => \"Hello\"");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                initializer: Some(Expr::ArrowFunction { params, body, .. }),
                ..
            } => {
                assert!(params.is_empty());
                assert!(matches!(body, ArrowBody::Expression(_)));
            }
            _ => panic!("Expected arrow function"),
        }
    }

    #[test]
    fn test_arrow_function_block_body() {
        let prog = parse("let f = (A) => { return A }");
        match &prog.statements[0] {
            Stmt::VariableDecl {
                initializer: Some(Expr::ArrowFunction { params, body, .. }),
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert!(matches!(body, ArrowBody::Block(_)));
            }
            _ => panic!("Expected arrow function"),
        }
    }

    #[test]
    fn test_return() {
        let prog = parse("function f() { return 10 }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { body, .. } => {
                match &body[0] {
                    Stmt::Return { value, .. } => {
                        assert!(value.is_some());
                    }
                    _ => panic!("Expected return statement"),
                }
            }
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_bare_return() {
        let prog = parse("function f() { return }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { body, .. } => {
                match &body[0] {
                    Stmt::Return { value, .. } => {
                        assert!(value.is_none());
                    }
                    _ => panic!("Expected return statement"),
                }
            }
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_if_statement() {
        let prog = parse("if true { 10 }");
        match &prog.statements[0] {
            Stmt::If {
                condition,
                then_body,
                else_branch,
                ..
            } => {
                assert!(matches!(condition, Expr::BooleanLiteral(true, _)));
                assert_eq!(then_body.len(), 1);
                assert!(else_branch.is_none());
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test]
    fn test_if_else() {
        let prog = parse("if true { 10 } else { 20 }");
        match &prog.statements[0] {
            Stmt::If {
                else_branch: Some(ElseBranch::Block(body)),
                ..
            } => {
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected if-else statement"),
        }
    }

    #[test]
    fn test_if_elseif_else() {
        let prog = parse("if true { 10 } else if false { 20 } else { 30 }");
        match &prog.statements[0] {
            Stmt::If {
                else_branch: Some(else_branch),
                ..
            } => {
                match else_branch {
                    ElseBranch::If { then_body, else_branch, .. } => {
                        assert_eq!(then_body.len(), 1);
                        assert!(else_branch.is_some());
                        assert!(matches!(else_branch.as_ref().unwrap().as_ref(), ElseBranch::Block(_)));
                    }
                    _ => panic!("Expected else-if"),
                }
            }
            _ => panic!("Expected if-elseif-else"),
        }
    }

    #[test]
    fn test_while_loop() {
        let prog = parse("while true { 10 }");
        match &prog.statements[0] {
            Stmt::While {
                condition,
                body,
                ..
            } => {
                assert!(matches!(condition, Expr::BooleanLiteral(true, _)));
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected while statement"),
        }
    }

    #[test]
    fn test_loop() {
        let prog = parse("loop { 10 }");
        match &prog.statements[0] {
            Stmt::Loop { body, .. } => {
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected loop statement"),
        }
    }

    #[test]
    fn test_break() {
        let prog = parse("loop { break }");
        match &prog.statements[0] {
            Stmt::Loop { body, .. } => {
                assert!(matches!(&body[0], Stmt::Break { .. }));
            }
            _ => panic!("Expected loop"),
        }
    }

    #[test]
    fn test_continue() {
        let prog = parse("loop { continue }");
        match &prog.statements[0] {
            Stmt::Loop { body, .. } => {
                assert!(matches!(&body[0], Stmt::Continue { .. }));
            }
            _ => panic!("Expected loop"),
        }
    }

    #[test]
    fn test_switch_case() {
        let prog = parse("switch A { case 1 { \"one\" } case 2 { \"two\" } }");
        match &prog.statements[0] {
            Stmt::Switch {
                value,
                cases,
                default_case,
                ..
            } => {
                assert!(matches!(value, Expr::Identifier(name, _) if name == "A"));
                assert_eq!(cases.len(), 2);
                assert!(default_case.is_none());
            }
            _ => panic!("Expected switch statement"),
        }
    }

    #[test]
    fn test_switch_with_default() {
        let prog = parse("switch A { case 1 { } default { } }");
        match &prog.statements[0] {
            Stmt::Switch { default_case, .. } => {
                assert!(default_case.is_some());
            }
            _ => panic!("Expected switch statement"),
        }
    }

    #[test]
    fn test_try_catch() {
        let prog = parse("try { 10 } catch e { e }");
        match &prog.statements[0] {
            Stmt::Try {
                body,
                catch_param,
                catch_body,
                ..
            } => {
                assert_eq!(body.len(), 1);
                assert_eq!(catch_param.as_deref(), Some("e"));
                assert!(catch_body.is_some());
            }
            _ => panic!("Expected try statement"),
        }
    }

    #[test]
    fn test_try_catch_finally() {
        let prog = parse("try { 10 } catch e { e } finally { 20 }");
        match &prog.statements[0] {
            Stmt::Try {
                finally_body, ..
            } => {
                assert!(finally_body.is_some());
            }
            _ => panic!("Expected try statement"),
        }
    }

    #[test]
    fn test_ternary() {
        let prog = parse("true ? 1 : 2");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Ternary {
                condition,
                then_branch,
                else_branch,
                ..
            }) => {
                assert!(matches!(condition.as_ref(), Expr::BooleanLiteral(true, _)));
                assert!(matches!(then_branch.as_ref(), Expr::NumberLiteral(1.0, _)));
                assert!(matches!(else_branch.as_ref(), Expr::NumberLiteral(2.0, _)));
            }
            _ => panic!("Expected ternary expression"),
        }
    }

    #[test]
    fn test_chained_member_access() {
        let prog = parse("a.b.c");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Member { object, property, .. }) => {
                assert_eq!(property, "c");
                match object.as_ref() {
                    Expr::Member {
                        object: inner,
                        property: inner_prop,
                        ..
                    } => {
                        assert_eq!(inner_prop, "b");
                        assert!(matches!(inner.as_ref(), Expr::Identifier(name, _) if name == "a"));
                    }
                    _ => panic!("Expected nested member access"),
                }
            }
            _ => panic!("Expected member access"),
        }
    }

    #[test]
    fn test_chained_index_access() {
        let prog = parse("a[0][1]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Index { object, index, .. }) => {
                assert!(matches!(index.as_ref(), Expr::NumberLiteral(1.0, _)));
                match object.as_ref() {
                    Expr::Index {
                        object: inner,
                        index: inner_index,
                        ..
                    } => {
                        assert!(matches!(inner_index.as_ref(), Expr::NumberLiteral(0.0, _)));
                        assert!(matches!(inner.as_ref(), Expr::Identifier(name, _) if name == "a"));
                    }
                    _ => panic!("Expected nested index access"),
                }
            }
            _ => panic!("Expected index access"),
        }
    }

    #[test]
    fn test_mixed_chained_access() {
        let prog = parse("a[0].b");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Member { object, property, .. }) => {
                assert_eq!(property, "b");
                assert!(matches!(object.as_ref(), Expr::Index { .. }));
            }
            _ => panic!("Expected member access"),
        }
    }

    #[test]
    fn test_call_chain() {
        let prog = parse("a().b()[0]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Index { object, .. }) => {
                match object.as_ref() {
                    Expr::Call { callee, .. } => {
                        assert!(matches!(callee.as_ref(), Expr::Member { .. }));
                    }
                    _ => panic!("Expected call"),
                }
            }
            _ => panic!("Expected index access"),
        }
    }

    #[test]
    fn test_nested_function() {
        let prog = parse("function outer() { function inner() { return 10 } return inner() }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { body, .. } => {
                assert_eq!(body.len(), 2);
                assert!(matches!(&body[0], Stmt::FunctionDecl { name, .. } if name == "inner"));
            }
            _ => panic!("Expected function declaration"),
        }
    }

    #[test]
    fn test_parenthesized_expression() {
        let prog = parse("(1 + 2) * 3");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Multiply,
                left,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::Binary { op: BinaryOp::Add, .. }));
            }
            _ => panic!("Expected multiplication"),
        }
    }

    #[test]
    fn test_multiple_statements() {
        let prog = parse("let A = 10\nlet B = 20");
        assert_eq!(prog.statements.len(), 2);
    }

    #[test]
    fn test_invalid_syntax() {
        let result = std::panic::catch_unwind(|| {
            parse("let = 10");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_brace() {
        let result = std::panic::catch_unwind(|| {
            parse("if true { 10");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_paren() {
        let result = std::panic::catch_unwind(|| {
            parse("function test(A { return A }");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_trailing_comma_list() {
        let prog = parse("[1, 2, 3,]");
        match &prog.statements[0] {
            Stmt::Expression(Expr::List { elements, .. }) => {
                assert_eq!(elements.len(), 3);
            }
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_trailing_comma_object() {
        let prog = parse(r#"{ a: 1, b: 2, }"#);
        match &prog.statements[0] {
            Stmt::Expression(Expr::Object { properties, .. }) => {
                assert_eq!(properties.len(), 2);
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_trailing_comma_params() {
        let prog = parse("function test(A, B,) { }");
        match &prog.statements[0] {
            Stmt::FunctionDecl { params, .. } => {
                assert_eq!(params.len(), 2);
            }
            _ => panic!("Expected function"),
        }
    }

    #[test]
    fn test_chained_comparisons() {
        let prog = parse("1 < 2 and 3 > 2");
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::And,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::Binary { op: BinaryOp::Less, .. }));
                assert!(matches!(right.as_ref(), Expr::Binary { op: BinaryOp::Greater, .. }));
            }
            _ => panic!("Expected and expression"),
        }
    }

    #[test]
    fn test_complex_precedence() {
        let prog = parse("1 + 2 * 3 > 5 and true or false");
        // Should be ((1 + (2 * 3)) > 5) and true) or false
        match &prog.statements[0] {
            Stmt::Expression(Expr::Binary { op: BinaryOp::Or, left, .. }) => {
                match left.as_ref() {
                    Expr::Binary {
                        op: BinaryOp::And,
                        ..
                    } => {}
                    _ => panic!("Expected and expression"),
                }
            }
            _ => panic!("Expected or expression"),
        }
    }
}
