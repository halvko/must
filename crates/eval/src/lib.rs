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

pub use machine::{ConstMode, Frame, Machine, Mode, RunMode, StepEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(u128),
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
}

/// A function value: which item's lowered MIR holds its code, and which of
/// that item's bodies it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnValue {
    pub item: ItemLoc,
    pub body: BodyId,
}

impl Value {
    pub fn display(&self) -> String {
        match self {
            Value::Unit => "()".to_owned(),
            Value::Int(v) => v.to_string(),
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
                    .map(|(name, value)| format!("{name}: {}", value.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {parts} }}")
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub kind: EvalErrorKind,
    pub message: String,
    /// Where it happened, for mapping back to source.
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
    /// Const-mode refusals: impure builtins, cycles, the recursion limit.
    NotConst,
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
        .map(|&(expr, body)| (expr, machine.force_const_block(&loc, body)))
        .collect()
}
