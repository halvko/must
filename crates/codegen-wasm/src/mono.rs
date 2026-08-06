//! Monomorphization: from decl-keyed, erased MIR to a finite set of
//! concrete function instances reachable from the entry point.
//!
//! Licensed by the specialization law — selection is lifetime-erased and
//! every instance is always-applicable, so running one specialized copy
//! per instantiation must agree with the interpreter's one-body-many-frames
//! execution. The differential harness is that law made executable.
//!
//! Three things happen here, and each of them exists because MIR
//! deliberately threw information away:
//!
//! 1. **Type arguments are re-derived.** MIR is decl-keyed and erased, and
//!    carries no type arguments at all — `Rvalue::Instantiate` is
//!    const-args-only, because the interpreter never needs more. A backend
//!    that must know how wide a `T` is recovers the missing arguments by
//!    UNIFYING the callee's declared parameter types against the concrete
//!    argument types at the call site (and its return type against the
//!    caller's destination), which works precisely because the caller
//!    instance is itself already concrete.
//!
//! 2. **Function values are resolved statically.** Must has no closures
//!    yet, so every function value is a compile-time constant: an item, a
//!    literal, or an instantiation of one. A tiny flow-insensitive
//!    constant propagation over locals therefore recovers the callee of
//!    every call — including calls through a *hidden dictionary
//!    parameter*, which is exactly how trait dispatch dies into a direct
//!    call. Values it cannot pin down are refused, never compiled to an
//!    indirect call.
//!
//! 3. **Const evaluation is reused, not reimplemented.** `static`s,
//!    `const { … }` blocks and turbofish const arguments are forced
//!    through the interpreter's own const machine, so a compiled program
//!    and an interpreted one agree on compile-time values by construction.

use base_db::Db;
use eval::{ConstMode, FnValue, Machine, Value};
use hir::item_tree::GenericParamKind;
use hir::{Builtin, ConstArgValue, ExprId, GenericArg, ItemLoc, Ty};
use mir::{BlockId, BodyId, Const, LocalId, MirBody, Operand, ProjElem, Rvalue, TerminatorKind};
use rustc_hash::FxHashMap;

use crate::layout::{self, Unsupported};
use crate::wasm::ValType;

/// A refusal, located: what we cannot compile and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub what: String,
    pub origin: Option<(ItemLoc, ExprId)>,
}

impl Refusal {
    pub fn new(what: impl Into<String>, loc: &ItemLoc, expr: ExprId) -> Refusal {
        Refusal {
            what: what.into(),
            origin: Some((loc.clone(), expr)),
        }
    }

    pub fn from_layout(err: Unsupported, loc: &ItemLoc, expr: ExprId) -> Refusal {
        Refusal::new(err.0, loc, expr)
    }

    pub fn message(&self) -> String {
        format!("{} is not supported by the wasm backend yet", self.what)
    }
}

/// A host import whose signature has no wasm representation, refused BY THE
/// IMPORT'S NAME with the layout rule as the parenthetical. Which boundary is
/// unavailable is the useful half; which layout rule objected is the detail.
///
/// It points at the DECLARATION, not at the call that reached it: the
/// signature is the declaration's, and that is where the fix goes.
fn extern_refusal(name: &str, at: &(ItemLoc, ExprId), err: Unsupported) -> CallTarget {
    CallTarget::Refused(Refusal::new(
        format!("{} in the host import `{name}`'s signature", err.0),
        &at.0,
        at.1,
    ))
}

/// A fully-instantiated callable: which body, plus everything an instance
/// of it is keyed by. Wraps the interpreter's own [`eval::FnValue`] — the
/// item, body and const arguments an instance is keyed by are identical to
/// the runtime's — and adds the type arguments a monomorphizing backend
/// additionally needs the widths for, which MIR itself erased.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnRef {
    pub value: eval::FnValue,
    /// Per binder position: the type argument, once known. `None` where
    /// nothing at any call site pinned it down (an unused parameter);
    /// a `None` that a layout actually needs is refused, never guessed.
    pub type_args: Vec<Option<Ty>>,
}

/// The statically-known value of a local, for the one kind of value that
/// has to be known statically: a callee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StaticVal {
    Unknown,
    Fn(FnRef),
    Builtin(Builtin),
    /// A HOST IMPORT — `static name = extern fn(...) -> T;`. Statically
    /// known by construction: an import is a declaration, and the
    /// declaration is the whole value, so it resolves through data and
    /// branches like any constant. The DECLARED signature rides along (an
    /// import is never generic, so it is ground): the wasm signature is
    /// built from it, not re-derived from what a call site happened to pass.
    ExternFn {
        decl: ItemLoc,
        sig: hir::FnTy,
    },
}

impl StaticVal {
    fn join(self, other: StaticVal) -> StaticVal {
        if self == other {
            self
        } else {
            StaticVal::Unknown
        }
    }
}

/// Identity of one generated wasm function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceKey {
    pub func: FnRef,
    /// The statically-known value of each parameter — the component that
    /// specializes a body on the function values passed into it. For a
    /// bounded generic these are its dictionary entries, which is how a
    /// dictionary parameter stops existing at runtime.
    pub arg_statics: Vec<StaticVal>,
}

/// What a `Call` terminator does, once resolved.
#[derive(Debug, Clone)]
pub enum CallTarget {
    Instance(InstanceKey),
    Print,
    Panic,
    /// A call of a host import, resolved to the name the module imports
    /// under and the wasm signature it imports with. The signature is
    /// computed HERE, where the instance's concrete types are in hand, so
    /// `compile` can declare every import before the first defined function
    /// without re-deriving anything.
    Extern {
        name: String,
        params: Vec<ValType>,
        results: Vec<ValType>,
        /// Where the import is DECLARED — the caret for a refusal raised
        /// while the import section is being built, long after this. A
        /// declaration is what such a refusal asks the user to change, so
        /// the call that reached it is not the useful site (and stands in
        /// only when the declaring item is too broken to have one).
        decl: (ItemLoc, ExprId),
    },
    Refused(Refusal),
}

/// Everything the emitter needs about one instance.
pub struct Analysis {
    /// Concrete type per MIR local (the instance's arguments substituted
    /// through), indexed by `LocalId`'s arena index.
    pub locals: Vec<Ty>,
    pub calls: FxHashMap<BlockId, CallTarget>,
    return_static: StaticVal,
}

/// Monomorphization state, shared by analysis and emission.
pub struct Mono<'db> {
    pub db: &'db dyn Db,
    machine: Machine<'db, ConstMode>,
    analyses: FxHashMap<InstanceKey, std::rc::Rc<Analysis>>,
    in_progress: Vec<InstanceKey>,
    depth: u32,
    /// [`Mono::register`]'s current depth-first path, one entry per source
    /// item still on the call stack (an item may appear more than once:
    /// that is what a growing chain of instantiations looks like).
    register_path: Vec<ItemLoc>,
    /// How many entries currently on `register_path` are RE-entries — an
    /// item that already appears earlier on the same path. See
    /// [`MAX_INSTANCE_DEPTH`].
    reentry_depth: u32,
    /// Registered (reachable) instances in discovery order — the order
    /// functions are emitted in, and therefore deterministic.
    pub order: Vec<InstanceKey>,
    index: FxHashMap<InstanceKey, u32>,
}

/// A generic body that instantiates itself with ever-larger arguments
/// would monomorphize forever; a cap turns that into an honest error.
const MAX_INSTANCES: usize = 20_000;

/// How deep [`Mono::analyze`] may nest inside itself before it stops
/// recursing and answers "unknown" instead.
///
/// What is counted: analysis frames, one per callee whose static return
/// value the analysis of its caller needs. What bounds it: this constant
/// alone — the recursion follows call edges, which may be cyclic. The
/// budget it must fit: analysis nests INSIDE [`Mono::register`]'s own
/// walk, so both spend the same stack, and [`MAX_PATH_DEPTH`]'s
/// measurement is of the two together.
const MAX_ANALYSIS_DEPTH: u32 = 200;

/// How many entries on [`Mono::register`]'s current depth-first path may
/// be RE-entries — an item that already sits somewhere earlier on the
/// same path — before the walk is refused as polymorphic recursion.
///
/// What is counted: re-entries of ANY item, not one item's own
/// recurrence. What bounds the path: a cycle through `k` distinct items
/// (direct self-recursion is the `k = 1` case) adds one re-entry per step
/// past its first `k` frames, so it is refused after roughly `k + 128`
/// frames whatever `k` is. Two shapes cost nothing here and are bounded
/// elsewhere: ordinary recursion, where a second call with the SAME
/// arguments is the SAME instance and [`Mono::register`] returns it from
/// its cache before this count is consulted; and a chain of distinct
/// items that never calls back into the path, which only
/// [`MAX_PATH_DEPTH`] bounds. The budget it must fit: the same one, via
/// that constant — a path here is at most `k + 128` frames long, and it
/// cannot outrun [`MAX_PATH_DEPTH`] in any case.
const MAX_INSTANCE_DEPTH: u32 = 128;

/// Hard ceiling on [`Mono::register`]'s current depth-first path length,
/// whether or not anything on the path repeats.
///
/// What is counted: the path's raw length, one entry per call frame the
/// walk is currently inside. What bounds the path: this constant alone —
/// it is the only bound on a chain of all-distinct items, the shape
/// [`MAX_INSTANCE_DEPTH`] never sees a re-entry in.
///
/// The budget it must fit: [`crate::STACK_BUDGET`], the 8 MiB stack every
/// caller of [`crate::compile`] promises. The heaviest shape is a mutual
/// cycle of generic items, each call re-instantiating the next at a fresh
/// type, with the [`MAX_ANALYSIS_DEPTH`]-deep analysis nested inside the
/// walk; refusing that at this cap was measured to peak well inside half
/// the budget. What a frame costs is not a constant of the language — it
/// grows as the compiler does — so the margin is checked rather than
/// remembered: the two deepest tests in `tests/structure.rs` run on
/// `STACK_BUDGET / 2`, and abort instead of refusing the day the walk
/// outgrows the 2x margin the budget promises. To re-measure, shrink that
/// stack until they do. The cap sits far below what the budget would pay
/// for both because no legitimately-written call graph is 220 levels deep
/// and because the spare half absorbs a heavier frame on a target we have
/// not measured.
const MAX_PATH_DEPTH: usize = 220;

impl<'db> Mono<'db> {
    pub fn new(db: &'db dyn Db) -> Mono<'db> {
        Mono {
            db,
            machine: Machine::for_const(db),
            analyses: FxHashMap::default(),
            in_progress: Vec::new(),
            depth: 0,
            register_path: Vec::new(),
            reentry_depth: 0,
            order: Vec::new(),
            index: FxHashMap::default(),
        }
    }

    /// Force a top-level item's compile-time value — the same query the
    /// editor's hover and the interpreter use.
    pub fn force_item(&mut self, loc: &ItemLoc) -> Result<Value, String> {
        self.machine
            .force_item(loc.clone())
            .map_err(|err| err.message)
    }

    pub fn force_const_block(
        &mut self,
        loc: &ItemLoc,
        body: BodyId,
        env: Vec<Value>,
    ) -> Result<Value, String> {
        self.machine
            .force_const_block(loc, body, env)
            .map_err(|err| err.message)
    }

    fn body(&self, loc: &ItemLoc, body: BodyId) -> &'db MirBody {
        &mir::mir_lowered(self.db, loc.to_id(self.db)).bodies[body]
    }

    /// The item's generic binder, or an empty binder for a plain item.
    fn generics(&self, loc: &ItemLoc) -> Vec<GenericParamKind> {
        hir::item_data(self.db, loc.to_id(self.db))
            .as_ref()
            .map(|data| data.generics.iter().map(|p| p.kind.clone()).collect())
            .unwrap_or_default()
    }

    /// The substitution an instance applies to every type in its body:
    /// one generic argument per binder position, type arguments from the
    /// instance key and const arguments from the interpreter's own
    /// const-arg values.
    fn subst_args(&self, func: &FnRef) -> Vec<GenericArg> {
        let generics = self.generics(&func.value.item);
        let mut consts = func.value.const_args.iter();
        let mut types = func.type_args.iter();
        generics
            .iter()
            .map(|kind| match kind {
                GenericParamKind::Type => {
                    GenericArg::Ty(types.next().cloned().flatten().unwrap_or(Ty::Error))
                }
                // Regions are ERASED — they never reach an `FnRef`, so
                // there is no argument to consume here and never will be.
                // Monomorphization is exactly where the specialization law
                // would break if one ever did.
                GenericParamKind::Region => GenericArg::Region(hir::Region::Erased),
                GenericParamKind::Const(_) => GenericArg::Const(
                    consts
                        .next()
                        .map(const_arg_value)
                        .unwrap_or(ConstArgValue::Error),
                ),
            })
            .collect()
    }

    /// Analyse an instance: concrete local types, resolved call targets,
    /// and what its return value is statically. Total (never fails) and
    /// memoized; refusals are recorded per call site so that unreachable
    /// code is never a reason to refuse a program.
    pub fn analyze(&mut self, key: &InstanceKey) -> std::rc::Rc<Analysis> {
        if let Some(analysis) = self.analyses.get(key) {
            return analysis.clone();
        }
        if self.in_progress.contains(key) || self.depth > MAX_ANALYSIS_DEPTH {
            // A cycle (recursion) or a runaway: the honest answer for the
            // question being asked here ("what does this return,
            // statically?") is "unknown", and the instance itself is
            // analysed properly by its own outermost invocation.
            return std::rc::Rc::new(Analysis {
                locals: Vec::new(),
                calls: FxHashMap::default(),
                return_static: StaticVal::Unknown,
            });
        }
        self.in_progress.push(key.clone());
        self.depth += 1;
        let analysis = std::rc::Rc::new(self.analyze_uncached(key));
        self.depth -= 1;
        self.in_progress.pop();
        self.analyses.insert(key.clone(), analysis.clone());
        analysis
    }

    fn analyze_uncached(&mut self, key: &InstanceKey) -> Analysis {
        let loc = key.func.value.item.clone();
        let body = self.body(&loc, key.func.value.body);
        let args = self.subst_args(&key.func);

        let locals: Vec<Ty> = body
            .locals
            .iter()
            .map(|(_, data)| layout::substitute(&data.ty, &loc, &args))
            .collect();

        // --- the fn-value constant propagation -----------------------
        //
        // Flow-insensitive: a local's value is the join of every
        // assignment to it (plus its incoming argument, for parameters).
        // Sound by construction — a local that is written two different
        // function values anywhere is Unknown everywhere, which refuses
        // rather than guesses.
        let mut statics: Vec<StaticVal> = vec![StaticVal::Unknown; body.locals.len()];
        let mut seen: Vec<bool> = vec![false; body.locals.len()];
        for (param, value) in body.params.iter().zip(&key.arg_statics) {
            let index = raw(param);
            statics[index] = value.clone();
            seen[index] = true;
        }
        // Two sweeps of a monotone (three-point) lattice reach a fixpoint
        // for straight-line propagation; loop until nothing moves so
        // chains through several locals settle too.
        for _ in 0..body.locals.len().max(4) {
            let mut changed = false;
            for (_, block) in body.blocks.iter() {
                for statement in &block.statements {
                    let mir::StatementKind::Assign { dest, rvalue } = &statement.kind;
                    if !dest.projection.is_empty() {
                        continue;
                    }
                    let value = self.static_of_rvalue(key, &statics, rvalue);
                    let slot = raw(dest.local);
                    let merged = if seen[slot] {
                        statics[slot].clone().join(value)
                    } else {
                        value
                    };
                    if !seen[slot] || merged != statics[slot] {
                        statics[slot] = merged;
                        seen[slot] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // --- call resolution, on the settled values ------------------
        let mut calls = FxHashMap::default();
        let mut return_static = StaticVal::Unknown;
        let mut first_return = true;
        for (block_id, block) in body.blocks.iter() {
            let origin = block.terminator.origin;
            match &block.terminator.kind {
                TerminatorKind::Call {
                    callee, args, dest, ..
                } => {
                    let target =
                        self.resolve_call(key, &locals, &statics, callee, args, *dest, origin);
                    // A call's result may itself be a function value (the
                    // `fn { print }()` shape): fold the callee's return
                    // value back into the caller's local.
                    if let CallTarget::Instance(callee_key) = &target {
                        let callee_key = callee_key.clone();
                        let callee = self.analyze(&callee_key);
                        let slot = raw(dest);
                        let value = callee.return_static.clone();
                        statics[slot] = if seen[slot] {
                            statics[slot].clone().join(value)
                        } else {
                            value
                        };
                        seen[slot] = true;
                    }
                    calls.insert(block_id, target);
                }
                TerminatorKind::Return => {
                    let slot = raw(body.return_local());
                    let value = statics.get(slot).cloned().unwrap_or(StaticVal::Unknown);
                    return_static = if first_return {
                        value
                    } else {
                        return_static.join(value)
                    };
                    first_return = false;
                }
                _ => {}
            }
        }
        if first_return {
            return_static = StaticVal::Unknown;
        }

        Analysis {
            locals,
            calls,
            return_static,
        }
    }

    fn static_of_rvalue(
        &mut self,
        key: &InstanceKey,
        statics: &[StaticVal],
        rvalue: &Rvalue,
    ) -> StaticVal {
        match rvalue {
            Rvalue::Use(op) => self.static_of_operand(key, statics, op),
            // `rep::<3>` — the item's fn value with evaluated const
            // arguments attached, exactly as the interpreter builds it.
            Rvalue::Instantiate { item, const_args } => {
                let mut values = Vec::with_capacity(const_args.len());
                for op in const_args {
                    match self.const_operand(key, op) {
                        Some(value) => values.push(value),
                        None => return StaticVal::Unknown,
                    }
                }
                match self.force_item(item) {
                    Ok(Value::Fn(f)) => StaticVal::Fn(FnRef {
                        value: FnValue {
                            const_args: values,
                            ..f
                        },
                        type_args: Vec::new(),
                    }),
                    _ => StaticVal::Unknown,
                }
            }
            _ => StaticVal::Unknown,
        }
    }

    fn static_of_operand(
        &mut self,
        key: &InstanceKey,
        statics: &[StaticVal],
        op: &Operand,
    ) -> StaticVal {
        match op {
            Operand::Copy(place) if place.projection.is_empty() => statics
                .get(raw(place.local))
                .cloned()
                .unwrap_or(StaticVal::Unknown),
            // A function value hiding inside an aggregate: nothing tracks
            // it, so any call through it is refused (never miscompiled).
            Operand::Copy(_) => StaticVal::Unknown,
            Operand::Const(Const::Builtin(builtin)) => StaticVal::Builtin(*builtin),
            Operand::Const(Const::ExternFn { decl, sig }) => StaticVal::ExternFn {
                decl: decl.clone(),
                sig: sig.clone(),
            },
            // A `fn` literal in THIS body: it inherits the enclosing
            // instance's arguments — the binder scopes the whole item, so
            // const params behave like auto-captured constants (the
            // interpreter's rule) and so do type params here.
            Operand::Const(Const::Fn(body)) => StaticVal::Fn(FnRef {
                value: FnValue {
                    item: key.func.value.item.clone(),
                    body: *body,
                    const_args: key.func.value.const_args.clone(),
                },
                type_args: key.func.type_args.clone(),
            }),
            Operand::Const(Const::Item(item)) => match self.force_item(item) {
                Ok(Value::Fn(f)) => StaticVal::Fn(FnRef {
                    value: f,
                    type_args: Vec::new(),
                }),
                Ok(Value::Builtin(builtin)) => StaticVal::Builtin(builtin),
                Ok(Value::ExternFn { decl, sig }) => StaticVal::ExternFn { decl, sig },
                _ => StaticVal::Unknown,
            },
            Operand::Const(Const::ConstBlock(body)) => match self.const_block(key, *body) {
                Some(Value::Fn(f)) => StaticVal::Fn(FnRef {
                    value: f,
                    type_args: Vec::new(),
                }),
                Some(Value::Builtin(builtin)) => StaticVal::Builtin(builtin),
                _ => StaticVal::Unknown,
            },
            Operand::Const(_) => StaticVal::Unknown,
        }
    }

    /// The compile-time value of a `const { … }` block (or const
    /// argument) under this instance, memoized per instance.
    pub fn const_block(&mut self, key: &InstanceKey, body: BodyId) -> Option<Value> {
        // The const machine memoizes per (instance, body) itself, so this
        // needs no memo of its own: a `const { … }` block inside a hot
        // function is evaluated exactly once per instantiation, exactly as
        // in the interpreter.
        self.force_const_block(
            &key.func.value.item,
            body,
            key.func.value.const_args.clone(),
        )
        .ok()
    }

    fn const_operand(&mut self, key: &InstanceKey, op: &Operand) -> Option<Value> {
        match op {
            Operand::Const(Const::ConstBlock(body)) => self.const_block(key, *body),
            Operand::Const(Const::Int(v)) => Some(Value::Int(*v)),
            Operand::Const(Const::Bool(b)) => Some(Value::Bool(*b)),
            Operand::Const(Const::Char(c)) => Some(Value::Char(*c)),
            Operand::Const(Const::Str(s)) => Some(Value::Str(s.clone())),
            Operand::Const(Const::ConstParam(index)) => {
                key.func.value.const_args.get(*index as usize).cloned()
            }
            _ => None,
        }
    }

    /// Resolve one `Call` terminator to a concrete target.
    #[allow(clippy::too_many_arguments)]
    fn resolve_call(
        &mut self,
        key: &InstanceKey,
        locals: &[Ty],
        statics: &[StaticVal],
        callee: &Operand,
        args: &[Operand],
        dest: LocalId,
        origin: ExprId,
    ) -> CallTarget {
        let callee_val = self.static_of_operand(key, statics, callee);
        let func = match callee_val {
            // Exhaustive over `hir::Builtin`, with no wildcard: a builtin
            // added later fails to compile here rather than silently
            // falling into "refused" (or, worse, silently compiling)
            // without anyone deciding which it should be.
            StaticVal::Builtin(builtin) => {
                match builtin {
                    Builtin::Print => return CallTarget::Print,
                    Builtin::Panic => return CallTarget::Panic,
                    Builtin::AllocArray
                    | Builtin::DeallocArray
                    | Builtin::Add
                    | Builtin::Offset
                    | Builtin::Copy
                    | Builtin::Dangling
                    | Builtin::ReadLine
                    | Builtin::NextChar => {}
                }
                return CallTarget::Refused(Refusal::new(
                    format!("the `{}` builtin", builtin.name()),
                    &key.func.value.item,
                    origin,
                ));
            }
            // A host import: the module grows an import entry and the call
            // becomes an ordinary `call` of it. The signature is the
            // DECLARATION's — the constant recorded it (`Const::ExternFn`)
            // precisely so no host, and no backend, has to re-derive it from
            // what a call site passed, and an import is never generic, so it
            // is ground here. If any part of it has no wasm representation
            // the refusal names the IMPORT rather than the type: a reader
            // needs to know which boundary is unavailable, not which layout
            // rule said so.
            StaticVal::ExternFn { decl, sig } => {
                let name = decl.display_name().to_owned();
                // Every diagnostic about an import belongs on its
                // declaration, so resolve that site once here. An item with
                // no initializer expression is broken and already carries
                // its own error; fall back to the call rather than pair an
                // item with an expression out of another item's body.
                let decl = match hir::body::body(self.db, decl.to_id(self.db)).root {
                    Some(root) => (decl, root),
                    None => (key.func.value.item.clone(), origin),
                };
                let mut params = Vec::new();
                for ty in &sig.params {
                    match layout::slots(self.db, ty) {
                        Ok(count) => {
                            params.extend(std::iter::repeat_n(ValType::I64, count as usize))
                        }
                        Err(err) => return extern_refusal(&name, &decl, err),
                    }
                }
                let results = match layout::slots(self.db, &sig.ret) {
                    Ok(count) => vec![ValType::I64; count as usize],
                    Err(err) => return extern_refusal(&name, &decl, err),
                };
                return CallTarget::Extern {
                    name,
                    params,
                    results,
                    decl,
                };
            }
            StaticVal::Fn(func) => func,
            StaticVal::Unknown => {
                return CallTarget::Refused(Refusal::new(
                    "a call through a function value that is not statically known \
                     (function values stored in data, or merged from several \
                     branches)",
                    &key.func.value.item,
                    origin,
                ));
            }
        };

        // The argument types, concretely — the raw material for
        // recovering the callee's type arguments.
        let mut arg_types = Vec::with_capacity(args.len());
        for op in args {
            match self.operand_ty(key, locals, op) {
                Ok(ty) => arg_types.push(ty),
                Err(err) => {
                    return CallTarget::Refused(Refusal::from_layout(
                        err,
                        &key.func.value.item,
                        origin,
                    ));
                }
            }
        }
        let dest_ty = locals.get(raw(dest)).cloned().unwrap_or(Ty::Error);

        let callee_body = self.body(&func.value.item, func.value.body);
        let generics = self.generics(&func.value.item);
        let type_slots = generics
            .iter()
            .filter(|kind| matches!(kind, GenericParamKind::Type))
            .count();
        let mut type_args: Vec<Option<Ty>> = func.type_args.clone();
        type_args.resize(type_slots, None);
        // Unification is authoritative; an inherited argument only
        // survives where nothing at this call site says otherwise.
        let mut derived: Vec<Option<Ty>> = vec![None; type_slots];
        for (param, concrete) in callee_body.params.iter().zip(&arg_types) {
            let declared = &callee_body.locals[*param].ty;
            unify(
                &func.value.item,
                &generics,
                declared,
                concrete,
                &mut derived,
            );
        }
        let declared_ret = &callee_body.locals[callee_body.return_local()].ty;
        unify(
            &func.value.item,
            &generics,
            declared_ret,
            &dest_ty,
            &mut derived,
        );
        for (slot, derived) in type_args.iter_mut().zip(derived) {
            if let Some(ty) = derived {
                *slot = Some(ty);
            }
        }

        let arg_statics = args
            .iter()
            .map(|op| self.static_of_operand(key, statics, op))
            .collect();

        CallTarget::Instance(InstanceKey {
            func: FnRef {
                value: func.value,
                type_args,
            },
            arg_statics,
        })
    }

    /// The concrete type of an operand inside an analysed instance.
    pub fn operand_ty(
        &mut self,
        key: &InstanceKey,
        locals: &[Ty],
        op: &Operand,
    ) -> Result<Ty, Unsupported> {
        match op {
            Operand::Const(Const::ExternFn { .. }) => {
                return Err(Unsupported::new(
                    "a host import used as a value (imports are callable, not data)",
                ));
            }
            Operand::Copy(place) => {
                let mut ty = locals
                    .get(raw(place.local))
                    .cloned()
                    .ok_or_else(|| Unsupported::new("a local with no type"))?;
                for elem in &place.projection {
                    ty = match elem {
                        // A tagged enum's payload column has no knowable
                        // type (see `layout::FieldRead`); MIR never
                        // projects into one, and refusing beats guessing.
                        ProjElem::Field(index) => layout::field(self.db, &ty, *index)?
                            .ty()
                            .cloned()
                            .ok_or_else(|| {
                                Unsupported::new(format!(
                                    "projecting into a payload of the tagged enum `{}`",
                                    ty.display()
                                ))
                            })?,
                        ProjElem::Index(_) => layout::element(self.db, &ty)?.0,
                        ProjElem::Deref => {
                            return Err(Unsupported::new(
                                "a raw-pointer dereference (heap and pointer primitives \
                                 are out of scope for this backend)",
                            ));
                        }
                    };
                }
                Ok(ty)
            }
            Operand::Const(Const::Unit) => Ok(Ty::Unit),
            Operand::Const(Const::Int(value)) => Ok(Ty::Int(value.kind())),
            Operand::Const(Const::Str(_)) => Ok(Ty::Str),
            Operand::Const(Const::Bool(_)) => Ok(Ty::Bool),
            Operand::Const(Const::Char(_)) => Ok(Ty::Char),
            Operand::Const(Const::Item(item)) => Ok(hir::signature(self.db, item.to_id(self.db))),
            // A `fn` LITERAL. It occupies zero slots whatever its
            // signature is — its identity travels in the static-value
            // lattice — but the type answered here still has to be the
            // literal's REAL substituted signature (X10), never a dummy
            // placeholder: [`unify`] reads it to recover the callee's type
            // arguments, and a placeholder return type would unify away
            // the actual one (`call0(fn() -> usize { 5 })` would then
            // "derive" `T = ()` instead of `T = usize`).
            Operand::Const(Const::Fn(body)) => {
                let inner = self.body(&key.func.value.item, *body);
                let args = self.subst_args(&key.func);
                let params = inner
                    .params
                    .iter()
                    .map(|param| {
                        layout::substitute(&inner.locals[*param].ty, &key.func.value.item, &args)
                    })
                    .collect();
                let ret = layout::substitute(
                    &inner.locals[inner.return_local()].ty,
                    &key.func.value.item,
                    &args,
                );
                Ok(Ty::fn_type(params, ret))
            }
            // A builtin's signature is a checker special case (`add`
            // preserves its pointer's flavor, `alloc_array` is generic in
            // a type MIR never carries), so there is no one `Ty::Fn` to
            // hand over. This shape is therefore chosen to TEACH NOTHING:
            // `Ty::Error` is skipped by [`unify`] wherever it lands, and
            // a fn type of any shape occupies zero slots.
            Operand::Const(Const::Builtin(_)) => Ok(Ty::fn_type(Vec::new(), Ty::Error)),
            Operand::Const(Const::ConstBlock(body)) => {
                let inner = self.body(&key.func.value.item, *body);
                let ty = inner.locals[inner.return_local()].ty.clone();
                Ok(layout::substitute(
                    &ty,
                    &key.func.value.item,
                    &self.subst_args(&key.func),
                ))
            }
            // A const parameter's value carries its own width.
            Operand::Const(Const::ConstParam(index)) => {
                match key.func.value.const_args.get(*index as usize) {
                    Some(Value::Int(v)) => Ok(Ty::Int(v.kind())),
                    Some(Value::Bool(_)) => Ok(Ty::Bool),
                    Some(Value::Str(_)) => Ok(Ty::Str),
                    Some(Value::Char(_)) => Ok(Ty::Char),
                    _ => Err(Unsupported::new("a const parameter of an unsupported type")),
                }
            }
        }
    }

    /// Register an instance (and everything it calls) for emission.
    /// Depth-first from the entry, so the emitted function order is
    /// deterministic; returns the assigned function slot.
    pub fn register(&mut self, key: &InstanceKey) -> Result<u32, Refusal> {
        if let Some(index) = self.index.get(key) {
            return Ok(*index);
        }
        let item = key.func.value.item.clone();
        if self.register_path.len() >= MAX_PATH_DEPTH {
            return Err(Refusal {
                what: format!(
                    "call chain deeper than {MAX_PATH_DEPTH} at `{}`: a call graph \
                     this deep",
                    elided_instance_name(key)
                ),
                origin: None,
            });
        }
        let is_reentry = self.register_path.contains(&item);
        if is_reentry && self.reentry_depth + 1 > MAX_INSTANCE_DEPTH {
            return Err(Refusal {
                what: format!(
                    "instantiation depth exceeded at `{}`: polymorphic recursion \
                     (a generic body whose own calls need ever-new instances)",
                    elided_instance_name(key)
                ),
                origin: None,
            });
        }
        let index = self.order.len() as u32;
        self.order.push(key.clone());
        self.index.insert(key.clone(), index);
        if self.order.len() > MAX_INSTANCES {
            return Err(Refusal {
                what: format!(
                    "this program (monomorphization produced more than {MAX_INSTANCES} \
                     function instances, which usually means an instantiation that \
                     never bottoms out)"
                ),
                origin: None,
            });
        }
        let analysis = self.analyze(key);
        // Deterministic discovery order: call sites in block index order.
        let mut sites: Vec<(usize, &CallTarget)> = analysis
            .calls
            .iter()
            .map(|(block, target)| (raw(block), target))
            .collect();
        sites.sort_by_key(|(block, _)| *block);
        let mut targets = Vec::new();
        for (_, target) in sites {
            match target {
                CallTarget::Instance(callee) => targets.push(callee.clone()),
                CallTarget::Refused(refusal) => return Err(refusal.clone()),
                // An import declares no instance to register — the module
                // grows an import entry for it in `compile` instead.
                CallTarget::Print | CallTarget::Panic | CallTarget::Extern { .. } => {}
            }
        }
        self.register_path.push(item);
        if is_reentry {
            self.reentry_depth += 1;
        }
        let mut refused = None;
        for callee in targets {
            if let Err(refusal) = self.register(&callee) {
                refused = Some(refusal);
                break;
            }
        }
        if is_reentry {
            self.reentry_depth -= 1;
        }
        self.register_path.pop();
        match refused {
            Some(refusal) => Err(refusal),
            None => Ok(index),
        }
    }

    pub fn func_index(&self, key: &InstanceKey) -> Option<u32> {
        self.index.get(key).copied()
    }

    /// A deterministic, readable symbol for an instance — the name that
    /// lands in the module's `name` section.
    pub fn mangle(&self, key: &InstanceKey) -> String {
        let mut name = String::new();
        if let Some((member, _)) = &key.func.value.item.member {
            name.push_str(&key.func.value.item.name);
            name.push_str("::");
            name.push_str(member);
        } else if key.func.value.item.name.is_empty() {
            name.push_str("<unnamed>");
        } else {
            name.push_str(&key.func.value.item.name);
        }
        if key.func.value.item.disambiguator > 0 {
            name.push_str(&format!("#{}", key.func.value.item.disambiguator));
        }
        // The body distinguishes nested `fn` literals of the same item.
        name.push_str(&format!("$b{}", raw(key.func.value.body) as u32));
        let mut args: Vec<String> = Vec::new();
        for ty in key.func.type_args.iter().flatten() {
            args.push(ty.display());
        }
        for value in &key.func.value.const_args {
            args.push(value.display());
        }
        for (position, value) in key.arg_statics.iter().enumerate() {
            if let StaticVal::Fn(f) = value {
                args.push(format!(
                    "p{position}={}$b{}",
                    f.value.item.display_name(),
                    raw(f.value.body) as u32
                ));
            }
            if let StaticVal::Builtin(builtin) = value {
                args.push(format!("p{position}={}", builtin.name()));
            }
        }
        if !args.is_empty() {
            name.push_str(&format!("::<{}>", args.join(",")));
        }
        name
    }
}

/// An instance's item name with its argument list ELIDED. The refusal that
/// uses it fires precisely because those arguments have grown out of hand,
/// so spelling them would bury the message under the nesting that caused
/// it — unlike [`Mono::mangle`], whose whole job is to spell them.
fn elided_instance_name(key: &InstanceKey) -> String {
    let name = key.func.value.item.display_name();
    if key.func.type_args.iter().flatten().next().is_some() || !key.func.value.const_args.is_empty()
    {
        format!("{name}::<…>")
    } else {
        name.to_owned()
    }
}

/// Turn an evaluated const argument into the annotation-representable
/// const domain, so it can be substituted into types (`[usize; N]`).
fn const_arg_value(value: &Value) -> ConstArgValue {
    match value {
        Value::Int(v) => match u128::try_from(v.to_i128()) {
            Ok(v) => ConstArgValue::Int(v),
            Err(_) => ConstArgValue::Error,
        },
        Value::Bool(b) => ConstArgValue::Bool(*b),
        Value::Str(s) => ConstArgValue::Str(s.as_str().into()),
        Value::Char(c) => ConstArgValue::Char(*c),
        _ => ConstArgValue::Error,
    }
}

/// Recover type arguments: match a declared (possibly `Ty::Param`-carrying)
/// type against a concrete one, binding parameters of `item` as they line
/// up. Best-effort and shape-directed — a mismatch simply teaches nothing
/// (the checker already accepted the program; a shape that does not line
/// up here is an erased position, not an error).
fn unify(
    item: &ItemLoc,
    generics: &[GenericParamKind],
    declared: &Ty,
    concrete: &Ty,
    out: &mut [Option<Ty>],
) {
    match (declared, concrete) {
        (Ty::Param(param), _) if param.item == *item => {
            if matches!(concrete, Ty::Error | Ty::Param(_)) {
                return;
            }
            // `out` is dense over TYPE params; the binder index counts
            // const params too.
            let dense = generics
                .iter()
                .take(param.index as usize)
                .filter(|kind| matches!(kind, GenericParamKind::Type))
                .count();
            if let Some(slot) = out.get_mut(dense)
                && slot.is_none()
            {
                *slot = Some(concrete.clone());
            }
        }
        (Ty::Record(a), Ty::Record(b)) if a.fields.len() == b.fields.len() => {
            for ((_, a), (_, b)) in a.fields.iter().zip(&b.fields) {
                unify(item, generics, a, b, out);
            }
        }
        (Ty::Array { elem: a, .. }, Ty::Array { elem: b, .. }) => {
            unify(item, generics, a, b, out);
        }
        (Ty::RawPtr { pointee: a, .. }, Ty::RawPtr { pointee: b, .. }) => {
            unify(item, generics, a, b, out);
        }
        (Ty::Fn(a), Ty::Fn(b)) if a.params.len() == b.params.len() => {
            for (a, b) in a.params.iter().zip(&b.params) {
                unify(item, generics, a, b, out);
            }
            unify(item, generics, &a.ret, &b.ret, out);
        }
        (Ty::Named(a), Ty::Named(b)) if a.decl == b.decl && a.args.len() == b.args.len() => {
            for (a, b) in a.args.iter().zip(&b.args) {
                if let (GenericArg::Ty(a), GenericArg::Ty(b)) = (a, b) {
                    unify(item, generics, a, b, out);
                }
            }
        }
        // A variant-typed argument widens to its enum at a parameter of
        // enum type, so these two line up as well.
        (Ty::Named(a), Ty::Variant(b)) if a.decl == b.decl && a.args.len() == b.args.len() => {
            for (a, b) in a.args.iter().zip(&b.args) {
                if let (GenericArg::Ty(a), GenericArg::Ty(b)) = (a, b) {
                    unify(item, generics, a, b, out);
                }
            }
        }
        (Ty::Variant(a), Ty::Variant(b)) if a.decl == b.decl && a.args.len() == b.args.len() => {
            for (a, b) in a.args.iter().zip(&b.args) {
                if let (GenericArg::Ty(a), GenericArg::Ty(b)) = (a, b) {
                    unify(item, generics, a, b, out);
                }
            }
        }
        _ => {}
    }
}

/// A local/block id as a plain index — the arenas are dense, so the raw
/// index is the natural way to key parallel vectors.
fn raw<T>(idx: impl std::borrow::Borrow<la_arena::Idx<T>>) -> usize {
    u32::from(idx.borrow().into_raw()) as usize
}
