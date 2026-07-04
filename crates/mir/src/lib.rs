//! MIR: a control-flow-graph IR lowered from typed HIR — the single
//! representation that const eval, the runner/debugger, and future codegen
//! all consume, so they share one set of semantics (and one set of bugs).
//!
//! Two invariants:
//!
//! - **Lowering is total.** Every body lowers, even ill-typed ones. Where an
//!   erroneous value would be produced, a [`TerminatorKind::Trap`] carries
//!   an error message and structurally continues — the full CFG always
//!   exists, so flow analyses run past errors; at runtime execution aborts
//!   at the trap. Inference-class traps borrow the upstream diagnostic's
//!   text verbatim; other classes are hand-written at the trap site and
//!   kept consistent with hir's messages by convention.
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
    /// Every explicit `const { … }` block, with the zero-parameter body it
    /// lowered to — the check-time evaluation surface: const blocks are
    /// compile-time wherever they sit, even inside functions nothing calls.
    /// Inner blocks precede the blocks enclosing them (lowering completes
    /// inside out).
    pub const_blocks: Vec<(ExprId, BodyId)>,
    /// Every turbofish const argument (`rep::<3>` — the `3`), with the
    /// zero-parameter body its value expression lowered to. Const args are
    /// compile-time positions wherever their mention sits, exactly like
    /// `const` blocks — the same check-time evaluation surface
    /// (`eval::const_arg_values`), and the operands a
    /// [`Rvalue::Instantiate`] forces at instantiation time. Keyed by the
    /// argument's *value expression* (each has its own node, so failures
    /// squiggle the argument, not the whole mention).
    pub const_args: Vec<(ExprId, BodyId)>,
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

/// A writable location: a local, optionally projected into by a chain of
/// field indices. Indices use the same canonical sorted field order as
/// [`Ty::Record`] and [`AggregateKind::Record`] — the write-side twin of
/// [`Rvalue::Field`] (reads stay operand-based; only writes need to name a
/// nested destination). Field *names* are resolved to indices at lowering,
/// like every other projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub local: LocalId,
    /// Field-index path into the local's (possibly nested) record value;
    /// empty means the whole local.
    pub projection: Vec<u32>,
}

impl From<LocalId> for Place {
    fn from(local: LocalId) -> Place {
        Place {
            local,
            projection: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Assign { dest: Place, rvalue: Rvalue },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    /// Builds a compound value from its parts. `ops` line up with
    /// `kind`'s canonical field order — for [`AggregateKind::Record`],
    /// sorted by name, matching [`Ty::Record`]'s canonical order. Lowering
    /// evaluates field initializers in *source* order (into whatever
    /// operands that takes) before assembling this in canonical order, so
    /// side effects follow the written program even though the aggregate
    /// itself doesn't.
    Aggregate {
        kind: AggregateKind,
        ops: Vec<Operand>,
    },
    /// Reads one field out of a record value by position. `index` is into
    /// the same sorted field order [`AggregateKind::Record`] and
    /// [`Ty::Record`] use — the receiver's type is resolved at lowering, so
    /// the field name is already gone by the time it reaches MIR.
    Field {
        base: Operand,
        index: u32,
    },
    /// Constructs the fn value of a generic instantiation (`rep::<3>`):
    /// the mentioned item's own fn value with the *evaluated* const
    /// arguments attached (`FnValue.const_args` on the eval side) — an
    /// Aggregate-like construction, evaluated where the mention sits. The
    /// operands are `Const::ConstBlock` references to the argument bodies
    /// in [`MirLowered::const_args`]: a const argument is a compile-time
    /// expression exactly like a `const { … }` block, so it shares that
    /// machinery (forced once, memoized per enclosing instance).
    ///
    /// Deliberately carries NO type arguments: under rigid-param checking
    /// type params never affect lowering (TR06 — MIR consults types only for
    /// structure the body projects, and a param is opaque), so the runtime
    /// key of an instance is its const args alone. The FULL instance key
    /// (type args included) exists at the *type* level only — a
    /// monomorphizing backend will need it; the interpreter never does.
    Instantiate {
        /// The generic item being instantiated.
        item: ItemLoc,
        /// One operand per *const* param, in binder order (dense: type
        /// params claim no slot — they need nothing at runtime).
        const_args: Vec<Operand>,
    },
    /// The variant → enum widening conversion: takes a *variant-typed*
    /// (tag-free payload) value and injects the tag, producing an
    /// *enum-typed* (tagged) value. This op is the only place a tag is
    /// ever created — same-variant code paths never see one. Planted
    /// exactly where inference recorded a widening edge
    /// (`InferenceResult::widened`); `index`/`variant` identify the
    /// variant within `decl` (`variant` is the display name, carried so
    /// runtime values render without a database).
    WidenToEnum {
        op: Operand,
        decl: ItemLoc,
        index: u32,
        variant: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateKind {
    /// Field names sorted by name — the same canonical order as
    /// [`Ty::Record`]'s fields, so `ops[i]` is the value of `fields[i]`.
    Record(Vec<String>),
    /// A *variant-typed* value: the bare payload tuple, in declaration
    /// order. Deliberately carries no enum or variant identity — a
    /// variant-typed value is fully erased at runtime (the state-machine
    /// story); the tag exists only after [`Rvalue::WidenToEnum`].
    VariantPayload,
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
    /// A `const { … }` block — or a turbofish const argument, which is the
    /// same thing (a compile-time expression body; see
    /// [`MirLowered::const_args`]): the referenced body (in the same
    /// item's [`MirLowered::bodies`]) is forced at compile time —
    /// evaluated once per machine run and memoized (per enclosing
    /// instance, when the body reads const params), never executed as
    /// runtime code.
    ConstBlock(BodyId),
    /// A read of the enclosing generic binder's const parameter, resolved
    /// from the executing frame's instance (`FnValue.const_args`) — the
    /// TR06 substitution model: ONE MIR per generic item, materialized per
    /// frame. The index is *dense over const params only* (type params
    /// claim no slot — they never affect lowering), i.e. `const N` in
    /// `fn::<T, const N: usize>` is `ConstParam(0)`; lowering converts
    /// from the binder index `Resolution::ConstParam` carries.
    ConstParam(u32),
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
    /// Dispatch over a *tagged* (enum-typed) value: read the tag, jump to
    /// the arm whose variant index it is, or to `otherwise` (a catch-all
    /// arm's block — or the non-exhaustiveness trap, carrying the same
    /// message as the editor diagnostic). Matching a *variant-typed*
    /// scrutinee never emits this: the value can only be its one variant,
    /// so lowering destructures it directly — zero runtime dispatch (the
    /// state-machine payoff).
    SwitchVariant {
        discr: Operand,
        /// The enum declaration the tag must belong to (the machine
        /// sanity-checks it — a mismatch is an internal error, inference
        /// would have rejected the program).
        decl: ItemLoc,
        /// `(variant index, target)`, first pattern wins; at most one
        /// entry per index.
        arms: Vec<(u32, BlockId)>,
        otherwise: BlockId,
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
    /// `target`; at runtime, aborts with `message`. Inference-class traps
    /// borrow the message from the upstream diagnostic; other classes are
    /// hand-written and kept consistent with hir's messages by convention.
    Trap {
        message: String,
        dest: LocalId,
        target: BlockId,
    },
    /// A deferred *const-context* error guarding a call const-check
    /// rejected at initializer level. An item initializer is a const
    /// context with exactly one runtime escape: the runner's synthetic
    /// entry evaluates its initializer as run-mode code (const depth 0).
    /// So this aborts with `message` when executed in a const context, and
    /// falls through to `target` (where the guarded call sits) otherwise.
    /// Violations inside `const fn` bodies and `const` blocks don't use
    /// this — those are const contexts under every execution, so they get
    /// unconditional [`TerminatorKind::Trap`]s instead.
    ConstTrap {
        message: String,
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
