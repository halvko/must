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
//! Errors carry `(ItemLoc, ExprId)` provenance so the CLI and debugger can
//! map a crash back to source through the body source map.

mod machine;
#[cfg(test)]
mod tests;

use base_db::Db;
use hir::{Builtin, ExprId, ItemId, ItemLoc};
use mir::BodyId;

pub use machine::{ConstMode, Machine, Mode, RunMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(u128),
    Str(String),
    Bool(bool),
    Fn(FnValue),
    Builtin(Builtin),
}

/// A function value: which item's lowered MIR holds its code, and which of
/// that item's bodies it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
