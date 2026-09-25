#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VariableDecl {
        name: String,
        is_const: bool,
        initializer: Option<Expr>,
        exported: bool,
        span: Span,
    },
    FunctionDecl {
        name: String,
        params: Vec<FunctionParam>,
        body: Vec<Stmt>,
        exported: bool,
        /// `async function` (§49).
        is_async: bool,
        span: Span,
    },
    /// `import { a, b as c } from "./path"` — module-level only (§46, §78, §69).
    Import {
        /// (export_name, local_name). Without alias, local == export (§69).
        names: Vec<(String, String)>,
        path: String,
        span: Span,
    },
    /// `export { a, b }` or `export { a } from "./path"` (§47, §67).
    ExportList {
        names: Vec<String>,
        /// Re-export source path when present (§67).
        from: Option<String>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_branch: Option<ElseBranch>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Loop {
        body: Vec<Stmt>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Switch {
        value: Expr,
        cases: Vec<SwitchCase>,
        default_case: Option<Vec<Stmt>>,
        span: Span,
    },
    Try {
        body: Vec<Stmt>,
        catch_param: Option<String>,
        catch_body: Option<Vec<Stmt>>,
        finally_body: Option<Vec<Stmt>>,
        span: Span,
    },
    Expression(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_branch: Option<Box<ElseBranch>>,
    },
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub value: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionParam {
    pub name: String,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    NumberLiteral(f64, Span),
    StringLiteral(String, Span),
    BooleanLiteral(bool, Span),
    NullLiteral(Span),
    Identifier(String, Span),

    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Assignment {
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    CompoundAssignment {
        op: CompoundAssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    Postfix {
        op: PostfixOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        property: String,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    List {
        elements: Vec<Expr>,
        span: Span,
    },
    Object {
        properties: Vec<ObjectProperty>,
        span: Span,
    },
    Function {
        params: Vec<FunctionParam>,
        body: Vec<Stmt>,
        span: Span,
    },
    ArrowFunction {
        params: Vec<FunctionParam>,
        body: ArrowBody,
        span: Span,
    },
    Ternary {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        span: Span,
    },

    /// `await expr` (§49, §101).
    Await {
        expr: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrowBody {
    Expression(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectProperty {
    pub key: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Negate,
    Plus,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    RawEqual,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompoundAssignOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PostfixOp {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
}
