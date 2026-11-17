//! Expression nodes of the AST.

use super::{Arm, Block, Ident, Type};
use crate::span::Span;

/// Identity of one expression node, dense from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(pub u32);

impl Expr {
    /// For a `[ ]` expression, the id of the call that writes through it:
    /// a type's `set_index`, or the lookup of a map entry's slot. The parser
    /// leaves this id and the next two unused for exactly these calls.
    pub fn write_id(&self) -> ExprId {
        ExprId(self.id.0 + 1)
    }

    /// For a `[ ]` read through a type's `index`, the id of the copy of the
    /// element handed back to `set_index`.
    pub fn element_id(&self) -> ExprId {
        ExprId(self.id.0 + 2)
    }

    /// For a `[ ]` read through `index` or a map, the id of the operator
    /// method a compound assignment to it calls (`a[i] += v`).
    pub fn update_id(&self) -> ExprId {
        ExprId(self.id.0 + 3)
    }

    /// This expression with id `id` instead, for a call the checker
    /// records under one of the ids above.
    pub fn with_id(&self, id: ExprId) -> Expr {
        Expr { id, ..self.clone() }
    }
}

/// One expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// Unique, dense id; key of the checker's type table.
    pub id: ExprId,
    /// Source location (parentheses around it are not included).
    pub span: Span,
    /// Which expression.
    pub kind: ExprKind,
    /// Type arguments written after a name, a member name or a struct
    /// literal's name: `max<i64>`, `xs.map<string>`, `Pair<u8> { … }`.
    /// Boxed: rare, and every `Expr` stays small.
    pub type_args: Option<Box<TypeArgs>>,
}

/// Type arguments written at a use, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeArgs {
    /// In order; `None` for `_`, left to inference.
    pub args: Vec<Option<super::Type>>,
    /// From `<` to `>`.
    pub span: Span,
}

/// The expression forms of the language.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(u64),
    /// `(a, b, …)`, two or more elements.
    Tuple(Vec<Expr>),
    /// A `char` literal's code point.
    Char(u32),
    Float(f64),
    Str(String),
    /// `"a {x} b"`: literal text and interpolated expressions in order.
    Interp(Vec<Part>),
    /// `format("{} has {:>4}", a, b)` with a literal template, boxed
    /// so `Expr` stays small.
    Format(Box<FormatCall>),
    /// `e?`: the `Ok`/`Some` value, or an early return.
    Try(Box<Expr>),
    /// `fun(x) = e` or `fun(x: T): R { … }`. Boxed: it is large and rare.
    Closure(Box<Closure>),
    Bool(bool),
    /// A variable, function or intrinsic name.
    Name(String),
    Unary(UnOp, Box<Expr>),
    /// `lhs op rhs`; the `Span` is the operator token, where overflow panics point.
    Binary(BinOp, Span, Box<Expr>, Box<Expr>),
    /// `expr as Type`.
    Cast(Box<Expr>, Type),
    /// `callee(args)`.
    Call(Box<Expr>, Vec<Expr>),
    /// `base[index]`; the `Span` is the `[`, where a bounds panic points.
    Index(Box<Expr>, Span, Box<Expr>),
    /// `base.field` (also `.len`).
    Field(Box<Expr>, Ident),
    /// `Name { field: value, … }` or `m.Name { … }`. Boxed
    /// so every `Expr` stays small: the parser recurses once per nesting
    /// level, and E0206's depth limit assumes small frames.
    StructLit(Box<StructLiteral>),
    /// `[a, b, c]`.
    Array(Vec<Expr>),
    /// `[value; count]`.
    Repeat(Box<Expr>, Box<Expr>),
    /// `out place`, `ptr e` or `mut place`: a call argument for an extern's
    /// parameter of that mode.
    Pass(crate::ast::PassMode, Box<Expr>),
    /// `if cond { then } [else …]`; the else branch is an `If` or `Block` expression.
    If {
        cond: Box<Expr>,
        then: Block,
        else_: Option<Box<Expr>>,
    },
    /// `if let pattern = scrutinee { then } [else …]`, boxed whole
    /// so `Expr` stays small (the parser recurses on it).
    IfLet(Box<IfLet>),
    /// A block used as an expression.
    Block(Block),
    /// `[label:] loop { body }`: repeats until a `break`
    /// targets it, whose value is the loop's. The body is boxed so `Expr`
    /// stays small (the parser recurses on it).
    Loop {
        label: Option<Ident>,
        body: Box<Block>,
    },
    /// `match scrutinee { arms }`.
    Match(Box<Expr>, Vec<Arm>),
    /// `s.spawn(call)` (the scope `s` first), or `spawn Actor { … }`
    /// (none): the call or literal, wrapped by the parser in a closure with
    /// no parameters, which the new green thread runs. Boxed: rare.
    Spawn(Option<Box<Expr>>, Box<Expr>),
    /// `scope s { body }`: a block whose tasks, started through `s`, all
    /// finish before it does.
    Scope(Box<ScopeExpr>),
    /// `shield within ms { body }`, as the block it means:
    /// `{ $os.shield_enter(ms); defer $os.shield_exit(); { body } }`.
    Shield(Box<Block>),
    /// `select { arms }`.
    Select(Vec<SelectArm>),
}

/// `scope name { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeExpr {
    /// The scope's handle, `s` in `s.spawn(…)`.
    pub name: Ident,
    /// `scope<E>`: the error type its tasks fail with, when written.
    pub error: Option<super::Type>,
    /// Its settings, in order: `independent`, `independent(f)`, `limit n`.
    pub words: Vec<ScopeWord>,
    pub body: Block,
}

/// A scope's setting: a word, and its value (`limit 10`, `independent(f)`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeWord {
    pub name: Ident,
    pub arg: Option<Expr>,
}

/// Prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `-`
    Neg,
    /// `!`
    Not,
    /// `~`, bitwise not.
    BitNot,
}

/// Binary operators, lowest precedence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// The bit operators: `|`, `^`, `&`, `<<`, `>>`.
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
}

impl BinOp {
    /// The operator as written in source.
    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Or => "||",
            And => "&&",
            Eq => "==",
            Ne => "!=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            BitOr => "|",
            BitXor => "^",
            BitAnd => "&",
            Shl => "<<",
            Shr => ">>",
        }
    }

    /// `& | ^ << >>`: integer operands of one type.
    pub fn is_bitwise(self) -> bool {
        use BinOp::*;
        matches!(self, BitOr | BitXor | BitAnd | Shl | Shr)
    }

    /// Returns the precedence level, from 1 (`||`) to 10 (`* / %`); the bit
    /// operators sit between comparisons and `+ -`, as in Rust.
    pub fn level(self) -> u8 {
        use BinOp::*;
        match self {
            Or => 1,
            And => 2,
            Eq | Ne => 3,
            Lt | Le | Gt | Ge => 4,
            BitOr => 5,
            BitXor => 6,
            BitAnd => 7,
            Shl | Shr => 8,
            Add | Sub => 9,
            Mul | Div | Rem => 10,
        }
    }
}

/// A closure literal.
#[derive(Debug, Clone, PartialEq)]
pub struct Closure {
    /// Parameter names and their types, if written.
    pub params: Vec<(super::Ident, Option<super::Type>)>,
    /// The return type, if written.
    pub ret: Option<super::Type>,
    /// The body; `= e` is a block with tail `e`, as for functions.
    pub body: super::Block,
}

/// A `format` call with a literal template.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatCall {
    /// The template as the lexer kept it: escapes decoded, braces as written.
    pub template: String,
    /// The template literal, quotes included.
    pub template_span: Span,
    /// The values for its holes, in order.
    pub args: Vec<Expr>,
}

/// The contents of a struct literal.
#[derive(Debug, Clone, PartialEq)]
pub struct StructLiteral {
    /// `m` in `m.Name { … }`.
    pub module: Option<Ident>,
    /// The struct's name.
    pub name: Ident,
    /// `field: value` pairs in source order.
    pub fields: Vec<(Ident, Expr)>,
    /// `..base` after the fields: where every field not written comes
    /// from.
    pub base: Option<Box<Expr>>,
    /// The collection reading of the braces, `Map { "a": 1 }` or
    /// `Set { 1, 2 }`; `None` with `..base`, which only a struct takes.
    pub collection: Option<Box<Collection>>,
}

/// The braces after a type name read as a collection: the elements as
/// written, and the calls they stand for. The checker picks this reading
/// or the struct's fields; both share the value expressions.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// `key: value` (`Some` value) or an item, in order.
    pub elements: Vec<(Expr, Option<Expr>)>,
    /// True when every element is `name: value`: the struct reading can
    /// take them as fields.
    pub as_fields: bool,
    /// `T.from_entries([k, …], [v, …])`, when no element is an item.
    pub entries: Option<Expr>,
    /// `T.from_items([a, …])`, when no element is `key: value`.
    pub items: Option<Expr>,
}

/// One piece of an interpolated string.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Literal text, escapes and `{{`/`}}` already decoded.
    Text(String),
    /// A `{expr}`, or `{expr:spec}` with a format specifier.
    Expr(Expr, Option<FormatText>),
}

/// A format specifier as written after an interpolation's `:`.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatText {
    /// The text after the `:`.
    pub text: String,
    /// From the `:` to the end of the text.
    pub span: Span,
}

impl Part {
    /// The interpolated expression, or `None` for literal text.
    pub fn expr(&self) -> Option<&Expr> {
        match self {
            Part::Expr(e, _) => Some(e),
            Part::Text(_) => None,
        }
    }
}

/// One arm of a `select`: what it waits for, then `=>` and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectArm {
    /// What the arm waits for.
    pub kind: SelectKind,
    /// The arm's value.
    pub body: Expr,
    /// From the arm's first token to the end of its body.
    pub span: Span,
}

/// What a `select` arm waits for.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectKind {
    /// `name = chan.recv()`, or `_ = chan.recv()`: `name` is the
    /// `Option<T>` received. `call` is the whole `chan.recv()`.
    Recv { binding: super::Ident, call: Expr },
    /// `chan.send(value)`: `call` is the whole call.
    Send(Expr),
    /// `after(ms)`: the most to wait, in milliseconds.
    After(Expr),
    /// `default`: when nothing is ready, so the `select` never waits.
    Default(Span),
}

impl SelectKind {
    /// The channel of a `recv` or `send` arm.
    pub fn channel(&self) -> Option<&Expr> {
        match self {
            SelectKind::Recv { call, .. } | SelectKind::Send(call) => match &call.kind {
                ExprKind::Call(callee, _) => match &callee.kind {
                    ExprKind::Field(chan, _) => Some(chan),
                    _ => None,
                },
                _ => None,
            },
            SelectKind::After(_) | SelectKind::Default(_) => None,
        }
    }
}

/// The call a `spawn`'s closure runs: the tail of its body.
pub fn spawned_call(closure: &Expr) -> &Expr {
    match &closure.kind {
        ExprKind::Closure(c) => c
            .body
            .tail
            .as_deref()
            .expect("the parser gives a spawn's closure the call as its tail"),
        _ => unreachable!("the parser wraps a spawn's call in a closure"),
    }
}

/// The parts of an `if let`.
#[derive(Debug, Clone, PartialEq)]
pub struct IfLet {
    pub pattern: super::Pattern,
    pub scrutinee: Expr,
    pub then: Block,
    pub else_: Option<Box<Expr>>,
}

/// The deadline and the body of `shield within ms { body }`, from the block
/// it means: the first statement's argument, and the tail.
pub fn shield_parts(block: &Block) -> (&Expr, &Block) {
    let within = match block.stmts.first().map(|s| &s.kind) {
        Some(super::StmtKind::Expr { expr, .. }) => match &expr.kind {
            ExprKind::Call(_, args) => &args[0],
            _ => unreachable!("the parser made `$os.shield_enter(ms)`"),
        },
        _ => unreachable!("the parser made `$os.shield_enter(ms)`"),
    };
    let body = match block.tail.as_deref().map(|t| &t.kind) {
        Some(ExprKind::Block(body)) => body,
        _ => unreachable!("the parser made the body the tail"),
    };
    (within, body)
}
