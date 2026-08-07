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
use hir::{BindingId, Builtin, ExprId, IntValue, ItemId, ItemLoc, Ty};
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
    /// Whether the local's address is taken somewhere in the body
    /// ([`Rvalue::AddrOf`] names it as the base of a place that does NOT
    /// lead with a deref — a deref-rooted `p.*.x.&raw mut` only *reads*
    /// its root, so it marks nothing) — the two-tier locals fact: only
    /// addressable locals are ever promoted into the interpreter's
    /// abstract memory; everything else stays on the plain per-frame
    /// value map, so pointer-free bodies pay nothing.
    pub addressable: bool,
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

/// A location: a local, optionally projected into by a chain of field
/// indices, computed element indices, and pointer derefs. Field steps use
/// the same canonical sorted field order as [`Ty::Record`] and
/// [`AggregateKind::Record`]; field *names* are resolved to indices at
/// lowering, like every other projection. Element indices stay operands,
/// evaluated where the place is used (and bounds-checked there — an
/// out-of-range element step is an ordinary trap, never silent
/// corruption). A [`ProjElem::Deref`] step follows a raw pointer: the
/// place stops naming this frame's storage and names the pointee's
/// allocation instead — liveness (and, for writes, writability) is checked
/// when the place is actually read or written, and a violation there is
/// detected UB, not a trap. Places appear as assignment destinations, as
/// [`Operand::Copy`] sources, and as the operand of [`Rvalue::AddrOf`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub local: LocalId,
    /// Projection path into the local's (possibly nested) value; empty
    /// means the whole local.
    pub projection: Vec<ProjElem>,
}

/// One step of a [`Place`] projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjElem {
    /// A record field, by canonical sorted-field index.
    Field(u32),
    /// An array element, by a computed `usize` index — bounds-checked when
    /// the place is read or written.
    Index(Operand),
    /// Follow the raw pointer the place names so far: the rest of the
    /// projection continues inside the pointee's allocation. Lowering only
    /// emits this as the *leading* element (everything below the outermost
    /// deref of a source chain is an ordinary read that produces the
    /// pointer), but the machine resolves it at any position.
    Deref,
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
    /// The one write. A destination whose projection contains a
    /// [`ProjElem::Deref`] is a store *through a raw pointer* (`p.* = v;`,
    /// `p.*.x = v;`): the machine resolves it against its abstract memory,
    /// with liveness and writability checked at the store — the misuse
    /// cases are detected UB, not silent corruption.
    Assign { dest: Place, rvalue: Rvalue },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    /// `-x` — unary negation of an integer. The operand's [`IntValue`]
    /// carries its own width, so the machine range-checks the result at
    /// the operation (an overflow — unsigned non-zero, or the signed
    /// minimum — is an ordinary runtime trap; eager in const contexts).
    UnaryNeg(Operand),
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
    /// `a[i]` — reads one element out of an array value by computed index,
    /// bounds-checked at execution: an out-of-range index is an ordinary
    /// runtime trap (the deferred-error story, NOT undefined behavior),
    /// with the same message the compile-time squiggle uses when both
    /// sides are known.
    Index {
        base: Operand,
        index: Operand,
    },
    /// `[e; N]` — builds an array of `count` copies of `elem`. `count` is
    /// a compile-time value by checking (a literal or a const-param read);
    /// it arrives as an ordinary operand.
    Repeat {
        elem: Operand,
        count: Operand,
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
    /// `place.&raw` / `place.&raw mut` of a local (or a temp holding a
    /// `const` use's copy, or a temp holding a pointer for a deref-rooted
    /// place like `p.*.x.&raw mut`): a pointer to the place — the root
    /// plus the projection's element-granular path. Executing it promotes
    /// the root local into the machine's abstract memory (first
    /// address-taking only) — *unless* the projection leads with a
    /// [`ProjElem::Deref`], in which case the root already holds a pointer
    /// and the result is that pointer with the extended path: no
    /// intermediate materialization, no new allocation (that is the whole
    /// point of `.&raw`). Validity of the minted address is NOT checked
    /// here (element steps are not bounds-checked at address-taking, per
    /// the deref-time-validity rule) — an out-of-range address mints
    /// silently and every later deref of it is detected UB. The resulting
    /// value is `(AllocId, path)`, never an integer.
    AddrOf {
        mutable: bool,
        place: Place,
    },
    /// `place.&` / `place.&mut` — a SAFE borrow of a place. Structurally
    /// [`Rvalue::AddrOf`]'s twin, and at runtime the identical value: a
    /// borrow and a raw pointer to the same place are the same machine
    /// word, and the region that told them apart is already erased by the
    /// time MIR exists.
    ///
    /// It is a variant of its own anyway, for two reasons. The interpreter
    /// mints an aliasing-tree NODE here and only here — a raw borrow
    /// deliberately inherits its parent's node instead — so the two
    /// operations differ in exactly one observable way: what they do to
    /// that tree. And every exhaustive consumer is forced to decide what a
    /// safe borrow means for it rather than inheriting the raw answer by
    /// accident (the wasm backend refuses it BY NAME).
    ///
    /// A borrow whose root is a `static` lowers through
    /// [`Rvalue::AddrOfStatic`] instead: a static's allocation is
    /// read-only and shared for the whole run, so there is no exclusivity
    /// to track and no node worth minting (`.&mut` of a `static` is
    /// rejected upstream).
    Borrow {
        mutable: bool,
        place: Place,
    },
    /// `S[.field | [index]]....&raw` of a `static` item: the item's ONE
    /// place — minted once per machine run in the static-allocation table,
    /// so every `S.&raw` is the same address (static=identity,
    /// observable). Always shared (`S.&raw mut` is rejected upstream —
    /// `static mut` stays deferred); the allocation is read-only.
    AddrOfStatic {
        item: ItemLoc,
        /// Path into the static's value — same scheme (and same
        /// no-validity-check-at-minting rule) as [`Place::projection`];
        /// never contains [`ProjElem::Deref`] (a deref-rooted chain lowers
        /// through a pointer temp instead).
        projection: Vec<ProjElem>,
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
    /// `[e1, e2, e3]` — an array value; `ops` are the elements in source
    /// order (which IS the canonical order: elements are positional).
    Array,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// Read a place's current value (a copy — Must values are copied at
    /// every read). Usually a bare local; a projected place reads the
    /// nested field/element, and a [`ProjElem::Deref`] step reads through
    /// a raw pointer (liveness checked there — a dead allocation is
    /// detected UB — then the pointee, or the pointed-to element, is
    /// copied out).
    Copy(Place),
    /// Read a place's current value and END the place's ownership of it: a
    /// MOVE. Produced for reads of a local whose type has no `forget`
    /// capability. Linear types are not the only values hir refuses to
    /// duplicate — `T.&mut` is affine too, and lowers as a plain
    /// [`Operand::Copy`] — but they are the one class whose read the
    /// ALIASING model has to hear about: reading an exclusive borrow out is
    /// a handoff the tree already tracks by that borrow's own node, while
    /// reading a linear out means the OWNER's storage stops holding what a
    /// borrow OF IT named. A move is an invalidation event exactly as a
    /// write is, for that reason and no other.
    ///
    /// It carries the same value [`Operand::Copy`] does, and every consumer
    /// that only wants the value treats the two identically (the wasm
    /// backend does: linearity is check-time only, so the emitted bytes are
    /// the same). Without the distinction, `let b = s.&; s.eat(); b.*.id`
    /// read a stale value with nothing to say about it, while the
    /// write-shaped twin (`s = mk(2);`) was caught.
    Move(Place),
    Const(Const),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Const {
    Unit,
    /// A typed integer constant: inference resolved every literal's width
    /// (an unresolved or out-of-range literal traps instead), so the
    /// constant carries it — the typed-memory philosophy applied to
    /// scalars.
    Int(IntValue),
    Str(String),
    Bool(bool),
    /// One Unicode scalar value. A `char`, not a `u32`, all the way down:
    /// the invariant (never a surrogate) is carried by the type Rust
    /// already checks, so no layer below has to re-establish it.
    Char(char),
    /// The value of a top-level item, const-evaluated lazily on first use.
    Item(ItemLoc),
    Builtin(Builtin),
    /// A `fn` literal; its code is in [`MirLowered::bodies`] of the same item.
    Fn(BodyId),
    /// A HOST IMPORT — the value of `static name = extern fn(...) -> T;`.
    ///
    /// There is no body to point at, so the constant carries the
    /// DECLARATION instead: the item that declares the import — whose name
    /// is the name the host is asked for, deliberately, since the
    /// declaration is the whole contract and no symbol-override surface
    /// exists to disagree with it — and the signature it was asked with.
    ///
    /// The SIGNATURE rides along rather than being re-derived at the call
    /// from argument values or the destination local, because a host has to
    /// judge the DECLARATION — including the parts no argument value can
    /// show it, like a buffer pointer's pointee type. Record, don't
    /// re-derive. Keeping the whole `ItemLoc` rather than just its name is
    /// the same rule once more: a backend that must reject a declaration
    /// can then point at it.
    ExternFn {
        decl: ItemLoc,
        sig: hir::FnTy,
    },
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
