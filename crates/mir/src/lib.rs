//! MIR: a control-flow-graph IR lowered from typed HIR — the single
//! representation that const eval, the runner/debugger, and future codegen
//! all consume, so they share one set of semantics (and one set of bugs).
//!
//! Two invariants:
//!
//! - **Lowering is total.** Every body lowers, even ill-typed ones. Where an
//!   erroneous value would be produced, a [`TerminatorKind::Trap`] carries
//!   the upstream diagnostic and structurally continues — the full CFG
//!   always exists, so flow analyses run past errors; at runtime execution
//!   aborts at the trap. Traps never invent messages: every one borrows from
//!   a diagnostic some upstream analysis already reported.
//! - **Narrowing-readiness.** Compiler temps are minted fresh per evaluated
//!   expression and conditions stay materialized as [`Rvalue::BinaryOp`]
//!   statements feeding [`TerminatorKind::SwitchBool`], so a future
//!   value-range analysis can recover the predicate on each CFG edge. (User
//!   locals are ordinary mutable places — this is not SSA and won't become
//!   SSA when mutation lands.)
//!
//! Like the rest of hir-land, MIR is range-free: statements and terminators
//! carry [`ExprId`] provenance, and ranges are recovered through the body
//! source map only at the diagnostic/debugger boundary.

mod lower;
pub mod pretty;
#[cfg(test)]
mod tests;

use base_db::Db;
use hir::body::BinOp;
use hir::{BindingId, Builtin, ExprId, ItemId, ItemLoc, Ty};
use la_arena::{Arena, Idx};

pub type LocalId = Idx<LocalData>;
pub type BlockId = Idx<BlockData>;
pub type BodyId = Idx<MirBody>;

/// Everything lowered from one item: the root (const-initializer) body plus
/// one body per `fn` literal it contains.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MirLowered {
    pub bodies: Arena<MirBody>,
    /// The implicit `const { … }` evaluating the item's initializer (a
    /// `static f = fn { … }` root just produces the [`Const::Fn`] value).
    /// `None` when the item has no initializer at all.
    pub root: Option<BodyId>,
    /// Findings of lowering itself — MIR is a diagnostic *producer* like any
    /// other analysis; the aggregator attaches ranges.
    pub diagnostics: Vec<MirDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBody {
    /// `locals[0]` is always the return place, followed by the parameters.
    pub locals: Arena<LocalData>,
    pub blocks: Arena<BlockData>,
    /// Parameter locals, in order. Empty for const bodies.
    pub params: Vec<LocalId>,
    pub entry: BlockId,
}

impl MirBody {
    pub fn return_local(&self) -> LocalId {
        self.locals
            .iter()
            .next()
            .map(|(id, _)| id)
            .expect("every MIR body has a return local")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalData {
    pub ty: Ty,
    /// User-visible name for params and `let`s; `None` for compiler temps
    /// (the debugger's variables panel shows only named locals).
    pub name: Option<String>,
    /// The binding this local was created for (debugger provenance).
    pub binding: Option<BindingId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    /// Provenance: the HIR expression this was lowered from.
    pub origin: ExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Assign { dest: LocalId, rvalue: Rvalue },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Copy(LocalId),
    Const(Const),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Const {
    Unit,
    Int(u128),
    Str(String),
    Bool(bool),
    /// The value of a top-level item, const-evaluated lazily on first use.
    Item(ItemLoc),
    Builtin(Builtin),
    /// A `fn` literal; its code is in [`MirLowered::bodies`] of the same item.
    Fn(BodyId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub origin: ExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminatorKind {
    Goto {
        target: BlockId,
    },
    /// The discriminant is (almost always) a temp whose single defining
    /// statement is the materialized condition — that def is what a
    /// narrowing analysis reads to learn per-edge facts.
    SwitchBool {
        discr: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// Calls end blocks: they can trap or diverge. `target: None` means the
    /// callee's type says it never returns; lowering continues in a fresh,
    /// predecessor-less block so the CFG stays total.
    Call {
        callee: Operand,
        args: Vec<Operand>,
        dest: LocalId,
        target: Option<BlockId>,
    },
    Return,
    /// A deferred error: structurally produces `dest` and continues at
    /// `target`; at runtime, aborts with `message`. The message is borrowed
    /// from an upstream diagnostic — lowering never invents one.
    Trap {
        message: String,
        dest: LocalId,
        target: BlockId,
    },
    /// Never executed; the placeholder terminator of blocks lowering
    /// abandoned (e.g. the continuation it opened after a diverging call).
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirDiagnostic {
    /// A `fn` literal references a local of an enclosing function. Real
    /// captures need a designed capture story (capture lists, by-value vs
    /// by-reference); until then this is an honest "not yet".
    UnsupportedCapture { expr: ExprId, name: String },
}

impl MirDiagnostic {
    /// Shared with the trap planted at the same expression, so the runtime
    /// crash and the squiggle say the same thing.
    pub fn message(&self) -> String {
        match self {
            MirDiagnostic::UnsupportedCapture { name, .. } => format!(
                "`{name}` is a local of an enclosing function; \
                 captures are not supported yet"
            ),
        }
    }

    pub fn expr(&self) -> ExprId {
        match self {
            MirDiagnostic::UnsupportedCapture { expr, .. } => *expr,
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn mir_lowered<'db>(db: &'db dyn Db, item: ItemId<'db>) -> MirLowered {
    lower::lower_item(db, item)
}
