//! Abstrakter Syntaxbaum der v0-Teilmenge (SPEC §10.1).
//!
//! Jeder Ausdruck traegt eine eindeutige `ExprId`; der Typpruefer legt darueber
//! die Typtabelle (`sema::TypeInfo::expr_types`) an, die das Lowering benutzt.

use crate::diag::Span;

pub type ExprId = u32;

/// Typ-Syntax (noch nicht aufgeloest — `Named` kann ein Struct sein).
#[derive(Clone, Debug)]
pub enum TypeExpr {
    Named(String, Span),
    Ptr { mutable: bool, inner: Box<TypeExpr>, span: Span },
    Array { elem: Box<TypeExpr>, len: u64, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named(_, s) => *s,
            TypeExpr::Ptr { span, .. } => *span,
            TypeExpr::Array { span, .. } => *span,
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
    /// `&x` — Adresse von
    AddrOf,
    /// `*p` — Dereferenzierung
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
    Int(i128),
    Bool(bool),
    Ident(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Feldzugriff `base.name`
    Field(Box<Expr>, String, Span),
    /// Index `base[idx]`
    Index(Box<Expr>, Box<Expr>),
    /// Aufruf `name(args)` — nur direkte Funktionsnamen (Stufe 0)
    Call(String, Vec<Expr>, Span),
    /// `syscall(nr, a1..a6)`
    Syscall(Vec<Expr>),
    /// `expr as T`
    Cast(Box<Expr>, TypeExpr),
    /// `Point{ x: 1, y: 2 }`
    StructLit(String, Vec<(String, Expr, Span)>, Span),
    /// `[1, 2, 3]`
    ArrayLit(Vec<Expr>),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    /// `let`/`var`: `mutable` unterscheidet beide.
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
    Expr(Expr),
    Block(Block),
    /// Nur vom Parser bei Fehlerwiederherstellung erzeugt; wird ignoriert.
    Error(Span),
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, TypeExpr, Span)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub profile: Option<(String, Span)>,
    pub funcs: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
    pub consts: Vec<ConstDecl>,
    /// Anzahl vergebener ExprIds (= Groesse der Typtabelle).
    pub expr_count: u32,
}
