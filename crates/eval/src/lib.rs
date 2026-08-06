//! The MIR interpreter: one evaluation core, two drivers.
//!
//! [`const_value`] drives it as a salsa query (const mode): pure, fueled,
//! impure builtins refused — this is the `static x = const { … }` semantics.
//! The runner/debugger drives the same core with a [`Mode`] that performs
//! I/O. Statics force as constants from *either* driver — the const/runtime
//! boundary is the static-initializer boundary, not which binary you're in —
//! so a miscompiled expression misbehaves identically in both. That sharing
//! is the point: same MIR, same machine, same bugs.
//!
//! Most errors carry `(ItemLoc, ExprId)` provenance so the CLI and debugger
//! can map a crash back to source through the body source map. A few are
//! `origin: None` because they are raised outside expression execution:
//! run-mode `print` I/O failures, items with no root body (broken source,
//! already reported by the parser), and pre-execution checks (dangling item
//! references, arity mismatches, the const mode's own `print` guard).

mod machine;
#[cfg(test)]
mod tests;

use base_db::Db;
use hir::{Builtin, ExprId, ItemId, ItemLoc};
use mir::BodyId;

pub use hir::IntValue;
pub use machine::{ConstMode, Frame, Machine, Mode, RunMode, StepEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    /// A typed integer: the value plus its width ([`hir::IntValue`]) —
    /// element-granular typed memory applied to scalars, so every
    /// arithmetic operation enforces the type's range right where it runs.
    Int(IntValue),
    Str(String),
    Bool(bool),
    Fn(FnValue),
    Builtin(Builtin),
    /// A record value: fields sorted by name, matching `Ty::Record` and
    /// `mir::AggregateKind::Record`'s canonical order. The derived
    /// `PartialEq` is exactly structural equality on that sorted list, so
    /// `==`/`!=` on records fall out of the machine's generic operand
    /// equality for free.
    Record {
        fields: Vec<(String, Value)>,
    },
    /// A fixed-size array value: the elements in order. The homogeneous
    /// carrier `[T; N]` values run on — plain data (copyable, no heap),
    /// so it composes freely with records, const evaluation and freezing
    /// into statics. Derived `PartialEq` is elementwise structural
    /// equality, like `Record`'s.
    Array(Vec<Value>),
    /// A *variant-typed* value: the bare payload tuple, in declaration
    /// order. This is the uniform carrier for every payload arity —
    /// `Shape::Point` is `Tuple([])`, `Shape::Circle(3)` is
    /// `Tuple([Int(3)])`, `Shape::Pair(1, "a")` is `Tuple([Int(1),
    /// Str("a")])`. Deliberately tag-free: no enum, no variant index —
    /// for code that stays on one variant (the state-machine case) the
    /// enum is fully erased at runtime. The static variant type is the
    /// only thing that says what this is.
    Tuple(Vec<Value>),
    /// An *enum-typed* (tagged) value: which declaration, which variant,
    /// plus the payload. Only ever produced by the widening conversion
    /// (`mir::Rvalue::WidenToEnum`) — the tag exists exactly from that
    /// edge on. `name` duplicates what `(decl, index)` already determine,
    /// carried so display needs no database; derived equality is
    /// therefore still identity-plus-payload.
    Variant {
        decl: ItemLoc,
        index: u32,
        name: String,
        payload: Vec<Value>,
    },
    /// A raw pointer: an allocation identity plus an ELEMENT-GRANULAR path
    /// into it — never a byte offset, never an integer. Provenance by
    /// construction: an [`AllocId`] is never reused, so a dangling pointer
    /// stays recognizably dangling forever. The derived `PartialEq` on
    /// `(alloc, path)` is exactly pointer `==`/`!=`, falling out of the
    /// machine's structural operand equality for free. Displays as
    /// `&raw <opaque>` — no integer addresses exist to leak.
    Ptr {
        alloc: AllocId,
        path: Vec<PathElem>,
        /// Which node of the allocation's aliasing tree this pointer
        /// speaks through. See [`Provenance`] — deliberately OUTSIDE this
        /// value's equality and hash.
        tag: Provenance,
    },
    /// MACHINE-INTERNAL tracked-uninit (the ruled A04 poison): the value of
    /// a fresh `alloc_array` element before its first write. No surface
    /// program can name or construct it — no literal, no type, no
    /// conversion produces one — and no surface program can OBSERVE one
    /// either: every value-read that would touch it (a deref copy, `==`,
    /// `print`, an aggregate copy into pure value land) is detected UB at
    /// the read ("read of uninitialized memory"). Exactly two operations
    /// see it and live: writing an element (replaces the poison), and the
    /// `copy` builtin (transports it silently, memmove-style — a copy of a
    /// partially-written buffer must not lie). Miri's uninit tracking on
    /// Must's typed memory, one `Value` variant.
    Uninit,
}

/// A pointer's aliasing-tree node, wrapped so that it takes NO part in
/// pointer equality or hashing.
///
/// This wrapper is a hazard fence, and the hazard is specific. `Value`'s
/// derived `PartialEq` IS the language's `==` on pointers, and derived
/// equality on a tag-carrying `Ptr` would make two pointers to the same
/// place compare UNEQUAL merely because they were borrowed differently —
/// turning an aliasing-model bookkeeping detail into an observable program
/// result, which is exactly what the model must never do. The same argument
/// applies to hashing (memo keys). So `Provenance` compares equal to every
/// other `Provenance`, always, and hashes to nothing.
///
/// `None` marks a raw pointer minted straight off a bare local's name (not
/// through a safe borrow): it resolves lazily, against whichever node is
/// that allocation's ROOT at the moment of the access — a safe borrow taken
/// after the pointer was minted still governs it. Only an allocation no
/// safe borrow has EVER covered has no root, and the access is then a true
/// no-op.
#[derive(Debug, Clone, Copy, Default)]
pub struct Provenance(pub Option<BorrowTag>);

impl PartialEq for Provenance {
    fn eq(&self, _: &Provenance) -> bool {
        true
    }
}

impl Eq for Provenance {}

impl std::hash::Hash for Provenance {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// A node in an allocation's aliasing tree — an index into the machine's
/// node table. Per-machine-run, like [`AllocId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BorrowTag(pub u32);

/// Identity of one abstract-memory allocation, unique per machine run and
/// NEVER reused (that non-reuse is the whole UB-detection story: a freed or
/// dead allocation's id keeps naming it). Per-machine-run, so pointers are
/// excluded from anything memoized across runs (see the const-escape rule
/// in `Machine::force_item`) and from the const-arg domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AllocId(pub u64);

/// One step of a pointer's path into its allocation — element-granular
/// (a record field by canonical sorted index; an array element by index),
/// NEVER a byte offset: v1 observes no layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathElem {
    /// A record field, by the canonical sorted-field index (the same order
    /// `Ty::Record` and `mir::Place::projection` use).
    Field(u32),
    /// An array element (`a[i].&raw mut`, `p.*.buf[i].&raw mut`). Minted
    /// WITHOUT a bounds check — validity is a deref-time judgement — so
    /// an out-of-range step here is exactly what the deref-time
    /// out-of-bounds-pointer UB detection catches. The heap's
    /// pointer-arithmetic builtins (`alloc_array`, `add`) reuse this same
    /// shape.
    Index(u64),
}

/// A function value: which item's lowered MIR holds its code, and which of
/// that item's bodies it is.
///
/// `Hash` is derived (including the `BodyId` arena index) for a whole-
/// program, non-salsa consumer only — `codegen_wasm::mono::InstanceKey` —
/// which re-runs from scratch on every compile and so has no stale-index
/// hazard. Do NOT let this derive tempt [`Value`]'s own hand-written
/// `Hash` impl into hashing `Value::Fn(_)` by its `FnValue`: that impl
/// keys the salsa-backed [`Instance`] memo, which persists across edits,
/// and a churning arena index there would be a correctness bug, not a
/// style choice — see that impl's doc comment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnValue {
    pub item: ItemLoc,
    pub body: BodyId,
    /// The instance's evaluated const arguments, dense over the item's
    /// *const* params in binder order (`mir::Const::ConstParam`'s index
    /// space) — empty for non-generic functions. Type arguments never
    /// appear: they don't affect lowering (TR06), so the runtime identity
    /// of an instance is `(item, const_args)` alone. Frames executing this
    /// value resolve `ConstParam` operands here; fn literals *nested* in a
    /// generic body inherit the enclosing frame's values at construction,
    /// so const params behave like auto-captured constants.
    pub const_args: Vec<Value>,
}

/// Identity of one instantiation of a generic item — TR06's applicative
/// key: same item + same (canonical) args = the same instance everywhere,
/// with no call-site component (arena-indexed `ExprId`s churn under edits;
/// range-free identity is the firewall invariant). Today this keys the
/// machine's per-instance memo for compile-time bodies (`const` blocks and
/// const arguments evaluated under a generic frame).
///
/// Deliberately CONST-ARGS-ONLY (TR06): type params never affect lowering
/// — MIR consults types only for structure the body projects, and a rigid
/// param is opaque — so the runtime key omits them; nothing the
/// interpreter does ever distinguishes `id::<usize>` from `id::<str>`. A
/// consumer that lays values out (a backend, say) DOES need the widths
/// those omitted type arguments carry and so cannot key on `Instance`
/// alone; see `codegen-wasm`'s own instance key for how one such consumer
/// copes.
///
/// Lives in `eval` because [`GenericArgValue`] wraps [`Value`] — mir sits
/// below eval in the crate graph, and hoisting `Value` out of the
/// interpreter for a key nothing below eval consumes would invert the
/// layering for no benefit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instance {
    pub item: ItemLoc,
    pub args: Vec<GenericArgValue>,
}

/// One canonical generic argument of an [`Instance`]. Const-only in v1 —
/// see [`Instance`] for why type arguments don't enter the runtime key; the
/// variant exists (rather than a bare `Value`) so that adding a type-level
/// arm later is an extension, not a re-keying.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArgValue {
    Const(Value),
}

/// [`Value`] is hashable so it can sit in instance-identity keys
/// ([`Instance`], the machine's per-instance memos). Hand-written for one
/// reason: [`Value::Fn`] must never be a correctness lever in a key — an
/// [`FnValue`] carries a `BodyId` arena index that renumbers under body
/// edits, which is exactly why fn values are excluded from the const-arg
/// domain (TR06: concrete data types only) and rejected at the hir level (see
/// `hir::diag::FN_CONST_ARG`) before hashing could ever matter. The `Fn`
/// arm therefore hashes as its discriminant alone — a lawful collision
/// (equal values still hash equal), never an identity.
impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Unit => {}
            Value::Int(v) => v.hash(state),
            Value::Str(s) => s.hash(state),
            Value::Bool(b) => b.hash(state),
            // Discriminant only — see the impl comment.
            Value::Fn(_) => {}
            Value::Builtin(b) => b.hash(state),
            Value::Record { fields } => fields.hash(state),
            // Lawful but never an identity: array values are excluded from
            // the const-arg domain (see `hir::diag::ARRAY_CONST_ARG`), so
            // this hash can never key an instance.
            Value::Array(values) => values.hash(state),
            Value::Tuple(values) => values.hash(state),
            Value::Variant {
                decl,
                index,
                name,
                payload,
            } => {
                decl.hash(state);
                index.hash(state);
                name.hash(state);
                payload.hash(state);
            }
            // Lawful but never an identity: like `Fn`, pointers are
            // excluded from the const-arg domain (an `AllocId` is
            // per-machine-run), so this hash can never key an instance.
            // The TAG is deliberately absent: see `Provenance`. Two
            // pointers to the same place must key the same, whatever
            // borrows they travelled through.
            Value::Ptr { alloc, path, .. } => {
                alloc.hash(state);
                path.hash(state);
            }
            // Unreachable in a key (uninit never escapes memory — reads
            // trap); the discriminant alone is lawful regardless.
            Value::Uninit => {}
        }
    }
}

impl Value {
    pub fn display(&self) -> String {
        match self {
            Value::Unit => "()".to_owned(),
            Value::Int(v) => v.to_i128().to_string(),
            Value::Str(s) => format!("{s:?}"),
            Value::Bool(b) => b.to_string(),
            Value::Fn(_) => "fn".to_owned(),
            Value::Builtin(b) => format!("builtin {}", b.name()),
            Value::Record { fields } => {
                if fields.is_empty() {
                    return "{}".to_owned();
                }
                let parts = fields
                    .iter()
                    .map(|(name, value)| format!("{name} = {}", value.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {parts} }}")
            }
            // Rendered whole, like records — no truncation precedent
            // exists in the value renderers, so none is invented here.
            Value::Array(values) => {
                let parts = values
                    .iter()
                    .map(Value::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{parts}]")
            }
            // A payload-less variant-typed value renders like unit — all
            // the runtime has (its *type* names the variant; the debugger
            // shows it alongside).
            Value::Tuple(values) => {
                let parts = values
                    .iter()
                    .map(Value::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({parts})")
            }
            Value::Variant {
                decl,
                name,
                payload,
                ..
            } => {
                let head = format!("{}::{name}", decl.display_name());
                if payload.is_empty() {
                    head
                } else {
                    let parts = payload
                        .iter()
                        .map(Value::display)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{head}({parts})")
                }
            }
            // Never a number: no integer addresses exist to leak, and
            // rendering the (per-run) AllocId would invite reading meaning
            // into it.
            Value::Ptr { .. } => "&raw <opaque>".to_owned(),
            // Unreachable through surface reads (they trap first); shown
            // only by machine-internal surfaces (the debugger's memory
            // view, error plumbing).
            Value::Uninit => "<uninit>".to_owned(),
        }
    }

    /// Whether a raw pointer sits anywhere inside the value — the
    /// const-escape rule's predicate: an `AllocId` means nothing outside
    /// the machine run that minted it, so a memoized compile-time result
    /// may not carry one.
    pub fn contains_ptr(&self) -> bool {
        match self {
            Value::Ptr { .. } => true,
            Value::Record { fields } => fields.iter().any(|(_, value)| value.contains_ptr()),
            Value::Array(values) => values.iter().any(Value::contains_ptr),
            Value::Tuple(values) => values.iter().any(Value::contains_ptr),
            Value::Variant { payload, .. } => payload.iter().any(Value::contains_ptr),
            Value::Unit
            | Value::Int(_)
            | Value::Str(_)
            | Value::Bool(_)
            | Value::Fn(_)
            | Value::Builtin(_)
            | Value::Uninit => false,
        }
    }

    /// Whether tracked-uninit sits anywhere inside the value — the read
    /// gate's predicate (see [`Value::Uninit`]): a value-read that would
    /// carry poison into pure value land is detected UB at the read, which
    /// is exactly what keeps `Uninit` unobservable everywhere else (`==`,
    /// `print`, keys — none of them need their own check).
    pub fn contains_uninit(&self) -> bool {
        match self {
            Value::Uninit => true,
            Value::Record { fields } => fields.iter().any(|(_, value)| value.contains_uninit()),
            Value::Array(values) => values.iter().any(Value::contains_uninit),
            Value::Tuple(values) => values.iter().any(Value::contains_uninit),
            Value::Variant { payload, .. } => payload.iter().any(Value::contains_uninit),
            Value::Unit
            | Value::Int(_)
            | Value::Str(_)
            | Value::Bool(_)
            | Value::Fn(_)
            | Value::Builtin(_)
            | Value::Ptr { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub kind: EvalErrorKind,
    pub message: String,
    /// Where it happened, for mapping back to source.
    pub origin: Option<(ItemLoc, ExprId)>,
    /// Secondary provenance ("allocated here" on a double free), rendered
    /// by drivers below the primary location. Empty for almost every
    /// error; carried as the house blame currency so the driver — not the
    /// machine — turns it into text positions, like `origin`.
    pub notes: Vec<EvalNote>,
}

/// One secondary note on an [`EvalError`] — a labelled extra location
/// (heap-UB errors point at the allocation's birth site with it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalNote {
    pub message: String,
    pub origin: Option<(ItemLoc, ExprId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalErrorKind {
    /// A planted [`mir::TerminatorKind::Trap`] executed: broken code was
    /// reached. The message is the diagnostic the editor already shows, so
    /// it is *not* re-reported as a const-eval diagnostic.
    Trap,
    /// `panic(…)` was called.
    Panic,
    /// A dynamic error the interpreter itself raised (division by zero,
    /// overflow, internal invariant violations).
    Runtime,
    /// Const-mode refusals: impure builtins, cycles, the recursion limit —
    /// and a pointer escaping compile-time evaluation (a memoized result
    /// carrying a `Value::Ptr`: an `AllocId` is per-machine-run, so the
    /// value would be meaningless outside the run that minted it).
    NotConst,
    /// The interpreter detected undefined behavior — a dangling deref, a
    /// write into read-only memory — and stopped deterministically. NOT a
    /// [`Self::Trap`]: a trap re-fires a message the editor already shows
    /// as a squiggle, while UB is discovered dynamically and has no
    /// squiggle to mirror.
    ///
    /// Spec status (write this down wherever the semantics are described):
    /// *"When the interpreter detects undefined behavior it stops with a
    /// message; this is a quality of the interpreter, not a guarantee of
    /// the language — compiled Must may do anything with the same
    /// program."* Exactly Miri's contract: detected-UB traps are
    /// interpreter quality, not language semantics.
    UndefinedBehavior,
    /// A `ConstParam` operand was read with no instance to resolve it
    /// against — a compile-time body inside a *generic* item, forced
    /// standalone at check time ([`const_arg_values`],
    /// [`const_block_values`]). Not an error in the program: the value is
    /// simply not knowable pre-instantiation (TR06 — const arguments live
    /// on the instance, so evaluation depending on `const V` is
    /// per-instance), so the diagnostics layer
    /// skips it like [`Self::Trap`]; every *instantiated* execution
    /// carries the values and never produces this.
    Uninstantiated,
}

/// The item's value as a compile-time constant — the `static x = const { … }`
/// query. Fueled and pure; failures (other than [`EvalErrorKind::Trap`],
/// which already has a diagnostic) surface as editor diagnostics.
#[salsa::tracked(returns(ref))]
pub fn const_value<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Result<Value, EvalError> {
    Machine::for_const(db).force_item(hir::item_loc(db, item))
}

/// The check-time value of every `const { … }` block in the item's lowered
/// MIR — including blocks inside functions nothing ever calls: const blocks
/// are genuinely compile-time, so their failures are diagnostics, not
/// latent crashes. Inner blocks precede the blocks enclosing them; one
/// machine serves the whole item, so shared forcings are done once.
#[salsa::tracked(returns(ref))]
pub fn const_block_values<'db>(
    db: &'db dyn Db,
    item: ItemId<'db>,
) -> Vec<(ExprId, Result<Value, EvalError>)> {
    let loc = hir::item_loc(db, item);
    let mut machine = Machine::for_const(db);
    mir::mir_lowered(db, item)
        .const_blocks
        .iter()
        .map(|&(expr, body)| (expr, machine.force_const_block(&loc, body, Vec::new())))
        .collect()
}

/// The check-time value of every turbofish const argument in the item's
/// lowered MIR — the [`const_block_values`] twin for `rep::<3>`'s `3`
/// (const arguments are compile-time wherever their mention sits, mentions
/// inside uncalled functions included). Keyed by the argument's value
/// expression. Inside a *generic* item an argument may read the enclosing
/// binder's const params ([`EvalErrorKind::Uninstantiated`] — skipped by
/// the diagnostics layer; the value is computed per instance at the
/// mention's execution instead).
#[salsa::tracked(returns(ref))]
pub fn const_arg_values<'db>(
    db: &'db dyn Db,
    item: ItemId<'db>,
) -> Vec<(ExprId, Result<Value, EvalError>)> {
    let loc = hir::item_loc(db, item);
    let mut machine = Machine::for_const(db);
    mir::mir_lowered(db, item)
        .const_args
        .iter()
        .map(|&(expr, body)| (expr, machine.force_const_block(&loc, body, Vec::new())))
        .collect()
}
