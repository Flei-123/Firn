// SPDX-License-Identifier: GPL-2.0-only
//! Abstract syntax tree of the v0 subset (SPEC §10.1).
//!
//! Every expression carries a unique `ExprId`; the type checker builds the
//! type table over it (`sema::TypeInfo::expr_types`), which lowering uses.

use crate::diag::Span;

pub type ExprId = u32;

/// Type syntax (not yet resolved — `Named` may well be a struct).
/// **ROUND 79** — the length written as `_`: it is taken from the
/// initializer (`var m: [u8; _] = "hello"`). Gap 10 of `docs/ROUND66.md`:
/// round 66 counted about two hundred message texts by hand, and an
/// off-by-one there is not a compile error but a silently padded text with a
/// second, independent length beside it. The parser replaces this value with
/// the real length as soon as it has seen the initializer; it can therefore
/// never reach the type checker.
pub const LEN_INFER: u64 = u64::MAX;

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
    /// **ROUND 72** — `+%` `-%` `*%` (SPEC §13, `L9`): explicit wrapping,
    /// wanted on purpose (hashes, checksums, timestamps) rather than
    /// accepted as an escape hatch from checked arithmetic.
    AddWrap,
    SubWrap,
    MulWrap,
    /// **ROUND 72** — `+|` `-|` `*|`: explicit saturating (clamped to the
    /// type's own MIN/MAX rather than wrapping around).
    AddSat,
    SubSat,
    MulSat,
}

impl BinOp {
    pub fn is_cmp(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
    pub fn is_logic(self) -> bool {
        matches!(self, BinOp::LAnd | BinOp::LOr)
    }
    /// **ROUND 72** — is this one of the six explicit wrap/saturate forms?
    /// `lower.rs` uses this to route straight to `Op::BinWrapSat` — these
    /// operators are never checked, regardless of the build level.
    pub fn wrap_sat(self) -> Option<(crate::fir::WrapSatKind, crate::fir::BinOp)> {
        use crate::fir::{BinOp as F, WrapSatKind as K};
        match self {
            BinOp::AddWrap => Some((K::Wrap, F::Add)),
            BinOp::SubWrap => Some((K::Wrap, F::Sub)),
            BinOp::MulWrap => Some((K::Wrap, F::Mul)),
            BinOp::AddSat => Some((K::Sat, F::Add)),
            BinOp::SubSat => Some((K::Sat, F::Sub)),
            BinOp::MulSat => Some((K::Sat, F::Mul)),
            _ => None,
        }
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
            BinOp::AddWrap => "+%",
            BinOp::SubWrap => "-%",
            BinOp::MulWrap => "*%",
            BinOp::AddSat => "+|",
            BinOp::SubSat => "-|",
            BinOp::MulSat => "*|",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    /// `~x` — the bitwise complement (round 68). Separate from `Not`,
    /// because `!` is the LOGICAL negation of a `bool` and the two must not
    /// be confused: `!0u8` is a type error, `~0u8` is 255.
    BitNot,
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
    ///
    /// **ROUND 71** — the literal is UNTYPED here. The type checker decides
    /// from the context: with an `f32` around it, it becomes an `f32`,
    /// otherwise `f64` (SPEC §8.6). Without a context there is no rounding
    /// twice — the bit pattern of the binary64 is only narrowed to binary32
    /// when the type checker really asks for it.
    /// The second number is the binary32 bit pattern of the SAME text,
    /// correctly rounded (round 71) -- the way through the binary64 would
    /// not be.
    Float(u64, u32),
    /// **ROUND 71** — float literal WITH the suffix `f` (`1.5f`), as the bit
    /// pattern of one IEEE-754 binary32. It is always an `f32`, no matter
    /// what the context says.
    FloatF32(u32),
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
    /// **ROUND 70** — a TEXT LITERAL (`"…"`, `b"…"`, `u"…"`).
    ///
    /// The node carries the array literal of its octets/code units inside
    /// and lets the CONTEXT decide what it is: with an array type wanted it
    /// is that array literal (unchanged since round 39), otherwise it is a
    /// `str` (`strtype.rs`). `true` = `u"…"`, whose elements are `u16`.
    Text(bool, Box<Expr>),
    /// **Round 58** — a closure literal (`fnval.rs`).
    Lambda(Box<LambdaDecl>),
}

/// **Round 58** — an anonymous function in an expression.
///
/// ```text
/// fn(a: i32) -> i32 { return a + 1 }        // captures nothing
/// gc fn(a: i32) -> i32 { return a + n }     // captures 'n', on the GC heap
/// ```
///
/// The `gc` in front is not decoration: a closure that captures values needs
/// storage for them, and that storage is a GC object. Whoever writes it says
/// so — and gets an `AllocError!fn(…)` in return, exactly as with
/// `gc C{ … }`.
#[derive(Clone, Debug)]
pub struct LambdaDecl {
    /// Serial number within one compilation; the generated function is
    /// called `__closure#<id>`.
    pub id: u32,
    /// `true` for the form `gc fn(…)`: the record lies in the GC heap.
    pub heap: bool,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
    /// **ROUND 79** — position of the closing `}`. That is where the frame
    /// of a function dies, and the escape analysis (`escape.rs`) has to name
    /// that place in its message: a pointer that outlives the frame is only
    /// understandable when the reader is shown where the frame ends.
    pub end: Span,
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
    /// **ROUND 70** - the compound assignment `x op= e`.
    ///
    /// It is its OWN statement and not a rewrite into `x = x + e`, and that
    /// is the whole point: the left side may be evaluated only ONCE. With
    /// `a[f()] += 1` a rewrite in the parser would run `f()` twice
    /// (`tests/1338_assign_op_once.fi` nails it down).
    AssignOp {
        target: Expr,
        op: BinOp,
        value: Expr,
        span: Span,
    },
    /// **ROUND 70** - `x++` / `x--`, `up = true` for `++`.
    ///
    /// A STATEMENT, never an expression. `y = x++` does not exist here: the
    /// difference between prefix and postfix inside an expression is one of
    /// the most productive sources of error in C, and in C++ the order of
    /// evaluation around it is even undefined. As a pure statement the
    /// meaning is unambiguous (SPEC 12.7).
    Step {
        target: Expr,
        up: bool,
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
            | Stmt::AssignOp { span, .. }
            | Stmt::Step { span, .. }
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
            Stmt::AssignOp { .. } => "assign",
            Stmt::Step { .. } => "assign",
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

/// **Round 75** — `extern fn`: a declaration without a Firn body that
/// names a function defined elsewhere (SPEC §14.5). `link_name` is the
/// symbol the linker looks for: `None` means the bare Firn name (no
/// `_F0.` mangling — that is the whole point, see `modules::symbol`),
/// `Some(n)` an explicitly different C symbol
/// (`extern fn foo(...) -> T = "c_name"`).
#[derive(Clone, Debug)]
pub struct ExternInfo {
    pub link_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    /// Empty for `extern fn` (`extern` below is `Some`) — the parser
    /// enforces that syntactically (no `{ ... }` after an `extern fn`
    /// header, a `;` closes it instead).
    pub body: Block,
    pub span: Span,
    pub attrs: Vec<Attr>,
    /// **Round 75** — `Some` marks the declaration as `extern fn`
    /// (SPEC §14.5). `None` is an ordinary Firn function with a real body.
    pub extern_info: Option<ExternInfo>,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, TypeExpr, Span)>,
    pub span: Span,
    pub attrs: Vec<Attr>,
}

/// **ROUND 89** — a global variable (SPEC §14.1.statics).
///
/// `static mut COUNT: u64 = 0` / `static TABLE: [u8; 256] = [...]`. The
/// value must be evaluable at COMPILE TIME; there is no run time
/// initialisation and therefore no initialisation order (`statics.rs`).
#[derive(Clone, Debug)]
pub struct StaticDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub value: Expr,
    /// `static mut` — may be assigned to.
    pub mutable: bool,
    pub span: Span,
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
    /// **ROUND 89** — `static` declarations of this file (`statics.rs`).
    pub statics: Vec<StaticDecl>,
    /// `comptime { … }` at top level: runs BEFORE the type check and can
    /// produce source text through `emit_*` that the same run compiles
    /// (SPEC §6.4).
    pub comptime_blocks: Vec<(Block, Span)>,
    /// Number of ExprIds handed out (= size of the type table).
    pub expr_count: u32,
}
