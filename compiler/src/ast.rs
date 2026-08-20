//! Abstract syntax tree of the v0 subset (SPEC §10.1).
//!
//! Every expression carries a unique `ExprId`; the type checker builds the
//! type table over it (`sema::TypeInfo::expr_types`), which lowering uses.

use crate::diag::Span;

pub type ExprId = u32;

/// Type syntax (not yet resolved — `Named` may well be a struct).
#[derive(Clone, Debug)]
pub enum TypeExpr {
    Named(String, Span),
    Ptr { mutable: bool, inner: Box<TypeExpr>, span: Span },
    Array { elem: Box<TypeExpr>, len: u64, span: Span },
    /// **Round 58** — `fn(T1, T2) -> R`, a function as a value.
    /// `ret == None` is the function without a result (`fn(i32)`).
    Fn { params: Vec<TypeExpr>, ret: Option<Box<TypeExpr>>, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named(_, s) => *s,
            TypeExpr::Ptr { span, .. } => *span,
            TypeExpr::Array { span, .. } => *span,
            TypeExpr::Fn { span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LAnd,
    LOr,
}

impl BinOp {
    pub fn is_cmp(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
    pub fn is_logic(self) -> bool {
        matches!(self, BinOp::LAnd | BinOp::LOr)
    }
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::And => "&",
            BinOp::Or => "|",
            BinOp::Xor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::LAnd => "&&",
            BinOp::LOr => "||",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    /// `&x` — address of
    AddrOf,
    /// `*p` — dereference
    Deref,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    /// Float literal as the bit pattern of one IEEE-754 binary64.
    Float(u64),
    Int(i128),
    Bool(bool),
    Ident(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Field access `base.field`
    Field(Box<Expr>, String, Span),
    /// Index `base[idx]`
    Index(Box<Expr>, Box<Expr>),
    /// Call `f(args)` — direct function names only (stage 0)
    Call(String, Vec<Expr>, Span),
    /// `syscall(nr, a1..a6)`
    Syscall(Vec<Expr>),
    /// `expr as T`
    Cast(Box<Expr>, TypeExpr),
    /// `Point{ x: 1, y: 2 }`
    StructLit(String, Vec<(String, Expr, Span)>, Span),
    /// `[1, 2, 3]`
    ArrayLit(Vec<Expr>),
    /// Repeat literal `[value; N]`; `N` is a constant expression.
    ArrayRepeat(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    /// `let`/`var`: `mutable` tells the two apart.
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeExpr>,
        init: Expr,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    If {
        cond: Expr,
        then: Block,
        els: Option<Box<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// `for`-loop over a half-open, ascending range `start..end`.
    For {
        name: String,
        start: Expr,
        end: Expr,
        body: Block,
        /// Position of the loop variable (for error messages)
        name_span: Span,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    /// `defer <stmt>` or `errdefer <stmt>` — runs when the enclosing block
    /// is left, in reverse order of declaration (SPEC §5.1). The `bool`
    /// is `true` for `errdefer`: the statement then runs ONLY when the
    /// function is left through an error.
    Defer(Box<Stmt>, bool, Span),
    Expr(Expr),
    Block(Block),
    /// Produced by the parser during error recovery only; it is ignored.
    Error(Span),
}

impl Stmt {
    /// Source position of the statement (used by `--emit=ast`).
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Break(span)
            | Stmt::Continue(span)
            | Stmt::Defer(_, _, span)
            | Stmt::Error(span) => *span,
            Stmt::Expr(e) => e.span,
            Stmt::Block(b) => b.span,
        }
    }

    /// Short name of the statement kind (for the overview of `--emit=ast`).
    pub fn kind_name(&self) -> &'static str {
        match self {
            Stmt::Let { mutable: false, .. } => "let",
            Stmt::Let { mutable: true, .. } => "var",
            Stmt::Assign { .. } => "assign",
            Stmt::If { .. } => "if",
            Stmt::While { .. } => "while",
            Stmt::For { .. } => "for",
            Stmt::Break(_) => "break",
            Stmt::Defer(_, true, _) => "errdefer",
            Stmt::Defer(..) => "defer",
            Stmt::Continue(_) => "continue",
            Stmt::Return { .. } => "return",
            Stmt::Expr(_) => "expr",
            Stmt::Block(_) => "block",
            Stmt::Error(_) => "<fehlerhaft>",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

/// An attribute `#[attr]` or `#[attr(arg)]` in front of a declaration.
/// The valid spellings live in `attrs.rs` — there and nowhere else.
#[derive(Clone, Debug)]
pub struct Attr {
    pub name: String,
    pub args: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
    pub attrs: Vec<Attr>,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, TypeExpr, Span)>,
    pub span: Span,
    pub attrs: Vec<Attr>,
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: Span,
}

/// `import path.module` — path parts without suffix, relative to the root file.
#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub path: Vec<String>,
    /// Name under which the module is addressed by the source (last part).
    pub alias: String,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub profile: Option<(String, Span)>,
    /// `import` declarations of this file (module system, `modules.rs`).
    pub imports: Vec<ImportDecl>,
    /// `export { a, b }` — empty means: everything is visible.
    pub exports: Vec<(String, Span)>,
    pub funcs: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
    pub consts: Vec<ConstDecl>,
    /// `comptime { … }` at top level: runs BEFORE the type check and can
    /// produce source text through `emit_*` that the same run compiles
    /// (SPEC §6.4).
    pub comptime_blocks: Vec<(Block, Span)>,
    /// Number of ExprIds handed out (= size of the type table).
    pub expr_count: u32,
}
