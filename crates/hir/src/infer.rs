//! Type inference: a pure per-body query.
//!
//! Other items are seen only through [`signature`]; builtins through a fixed
//! table. The [`InferCtx`] owns an ena unification table plus a
//! [`Constraints`] store: axioms (annotations, call parameter types) unify
//! eagerly with their [`Cause`] recorded, while joins (`if`/`else` branch
//! agreement) are deferred and solved after traversal so blame can be
//! attributed with all axioms known — see [`crate::constraint`] for the
//! model. Trait obligations and further deferred constraint kinds slot into
//! the same store later without changing the query graph.

use base_db::{Db, SourceFile};
use ena::unify::InPlaceUnificationTable;
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::body::{BindingId, Body, ExprData, ExprId, LiteralData, Stmt, body};
use crate::constraint::{self, Cause, Constraints, Join, Witness, resolve_fully};
use crate::item_tree::Constness;
use crate::scopes::{Builtin, Resolution, resolutions};
use crate::ty::{
    Ty, TyVar, TyVarValue, lower_type_ref, signature, signature_needs_annotation, type_underlying,
};
use crate::{ItemId, ItemLoc, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceResult {
    pub type_of_expr: ArenaMap<ExprId, Ty>,
    pub type_of_binding: ArenaMap<BindingId, Ty>,
    pub diagnostics: Vec<InferenceDiagnostic>,
}

/// Range-free (keyed by HIR ids); ranges are attached by the diagnostics
/// layer through the body source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceDiagnostic {
    TypeMismatch {
        expr: ExprId,
        expected: Ty,
        actual: Ty,
        /// Why the expected type was required — rendered as "because of
        /// this" hints. Possibly several: a join mismatch cites every
        /// sibling branch that agreed on the type plus the axiom (annotation
        /// or call) that demanded it.
        reasons: Vec<Cause>,
    },
    /// Every branch of a join agrees on a type — the construct as a whole
    /// is what conflicts with the axiom that decided the expected type, so
    /// it carries one diagnostic instead of a squiggle per branch.
    AllBranchesMismatch {
        /// The joining construct (the whole `if` expression).
        expr: ExprId,
        expected: Ty,
        /// The type every branch produced.
        actual: Ty,
        /// The axiom that demanded `expected`, when recorded.
        reasons: Vec<Cause>,
    },
    /// The two branches of an `if`/`else` produced different types and
    /// nothing (no annotation, no use) breaks the tie.
    IfBranchMismatch {
        /// The else branch expression (carries the squiggle).
        else_expr: ExprId,
        /// The then branch expression (carries the related hint).
        then_expr: ExprId,
        then_ty: Ty,
        else_ty: Ty,
    },
    NotCallable {
        /// The callee expression.
        expr: ExprId,
        ty: Ty,
    },
    ArgCountMismatch {
        /// The call expression.
        expr: ExprId,
        expected: usize,
        found: usize,
    },
    /// A use of an item whose signature can't be determined because its
    /// definition lacks annotations. Permanent for exported symbols (their
    /// contract must be written); interprocedural inference will lift it
    /// for private items. See [`signature_needs_annotation`].
    NeedsAnnotation {
        /// The referencing expression.
        expr: ExprId,
        /// The item whose type couldn't be inferred.
        item: ItemLoc,
    },
    /// An assignment whose target resolves to a local declared without
    /// `mut`. Squiggle on the target; the binding's declaration carries the
    /// related hint (and is where a later quick fix will insert `mut`).
    AssignToImmutable {
        /// The assignment's target expression.
        target: ExprId,
        binding: BindingId,
        /// The binding's name, carried here so [`Self::message`] can render
        /// without the body in hand (same reason `NeedsAnnotation` carries
        /// an [`ItemLoc`]).
        name: String,
    },
    /// An assignment whose target resolves to a top-level item. `static`s
    /// have an identity but cannot be reassigned; `const`s don't even have
    /// a single place an assignment could write to — the message
    /// distinguishes the two.
    AssignToItem {
        /// The assignment's target expression.
        target: ExprId,
        item: ItemLoc,
        constness: Constness,
    },
    /// An assignment whose target resolves to a builtin function.
    AssignToBuiltin {
        /// The assignment's target expression.
        target: ExprId,
        builtin: Builtin,
    },
    /// A record literal checked against a record type that requires fields
    /// the literal doesn't have. One diagnostic on the whole literal naming
    /// every missing field — the fix (adding fields) happens there.
    RecordLitMissingFields {
        /// The record literal expression.
        expr: ExprId,
        /// The missing `(name, type)` pairs, sorted by name.
        fields: Vec<(String, Ty)>,
    },
    /// A record literal field its expected record type has no room for
    /// (exact field-set equality: extra fields are errors, not dropped).
    /// One diagnostic per extra field; the renderer narrows the squiggle to
    /// the field's name.
    RecordLitExtraField {
        /// The extra field's value expression.
        expr: ExprId,
        name: String,
        /// The expected record type that lacks the field.
        expected: Ty,
    },
    /// A field access on a type that has no such field: a record without
    /// it, or a non-record. The renderer narrows the squiggle to the field
    /// name.
    NoSuchField {
        /// The field-access expression.
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
    },
    /// A field access whose receiver's type is still undetermined when the
    /// access is checked. Structural records use exact equality, so a field
    /// name can never determine the receiver's type backwards — the way out
    /// is an annotation.
    FieldOnUnknownType {
        /// The field-access expression (where MIR will trap).
        expr: ExprId,
        /// The receiver — carries the squiggle: the annotation goes there.
        receiver: ExprId,
    },
    /// A bare use of a `type` item in expression position. Types are not
    /// first-class values; the one expression position a type name may
    /// appear in is as a construction head (`Foo(...)`), which never
    /// reaches this.
    TypeNotValue {
        /// The referencing expression (the name — carries the squiggle).
        expr: ExprId,
        /// The type's name, carried so [`Self::message`] renders without
        /// the body in hand (same reason `AssignToImmutable` carries one).
        name: String,
    },
    /// A construction call `Foo(...)` with anything but exactly one
    /// argument. A constructor is a plain function taking the underlying
    /// record value — nothing more.
    TypeCtorArgCount {
        /// The call expression.
        expr: ExprId,
        /// The `type` item being constructed.
        item: ItemLoc,
        found: usize,
    },
    /// A `break` with no enclosing `loop`. Function literals and `const`
    /// blocks reset the loop context — they are units of their own, so a
    /// `break` inside one never exits a loop outside it.
    BreakOutsideLoop {
        /// The break expression.
        expr: ExprId,
    },
    /// A `continue` with no enclosing `loop`; same boundaries as
    /// [`Self::BreakOutsideLoop`].
    ContinueOutsideLoop {
        /// The continue expression.
        expr: ExprId,
    },
}

impl InferenceDiagnostic {
    /// The expression the diagnostic is reported on.
    pub fn expr(&self) -> ExprId {
        match self {
            InferenceDiagnostic::TypeMismatch { expr, .. }
            | InferenceDiagnostic::AllBranchesMismatch { expr, .. }
            | InferenceDiagnostic::NotCallable { expr, .. }
            | InferenceDiagnostic::ArgCountMismatch { expr, .. }
            | InferenceDiagnostic::NeedsAnnotation { expr, .. }
            | InferenceDiagnostic::RecordLitMissingFields { expr, .. }
            | InferenceDiagnostic::RecordLitExtraField { expr, .. }
            | InferenceDiagnostic::NoSuchField { expr, .. }
            | InferenceDiagnostic::TypeNotValue { expr, .. }
            | InferenceDiagnostic::TypeCtorArgCount { expr, .. }
            | InferenceDiagnostic::BreakOutsideLoop { expr }
            | InferenceDiagnostic::ContinueOutsideLoop { expr } => *expr,
            InferenceDiagnostic::FieldOnUnknownType { receiver, .. } => *receiver,
            InferenceDiagnostic::IfBranchMismatch { else_expr, .. } => *else_expr,
            InferenceDiagnostic::AssignToImmutable { target, .. }
            | InferenceDiagnostic::AssignToItem { target, .. }
            | InferenceDiagnostic::AssignToBuiltin { target, .. } => *target,
        }
    }

    /// The human-readable message. Shared between editor diagnostics and MIR
    /// trap terminators, so a deferred error crashes at runtime with exactly
    /// the text the squiggle showed.
    pub fn message(&self) -> String {
        match self {
            InferenceDiagnostic::TypeMismatch {
                expected, actual, ..
            } => {
                let base = format!(
                    "type mismatch: expected `{}`, found `{}`",
                    expected.display(),
                    actual.display()
                );
                // A nominal/structural near-miss: the found record may even
                // be the declared shape, but a named type never coerces —
                // say how to actually make one.
                if let (Ty::Named(loc), Ty::Record(_)) = (expected, actual) {
                    format!(
                        "{base}; `{name}` is a distinct type — construct it with `{name}(...)`",
                        name = loc.display_name()
                    )
                } else {
                    base
                }
            }
            InferenceDiagnostic::AllBranchesMismatch {
                expected, actual, ..
            } => format!(
                "every branch produces `{}`, but `{}` is needed",
                actual.display(),
                expected.display()
            ),
            InferenceDiagnostic::IfBranchMismatch {
                then_ty, else_ty, ..
            } => format!(
                "`if` branches have incompatible types: `{}` vs `{}`; \
                 add a type annotation to decide between them",
                then_ty.display(),
                else_ty.display()
            ),
            InferenceDiagnostic::NotCallable { ty, .. } => {
                format!("expression of type `{}` is not callable", ty.display())
            }
            InferenceDiagnostic::ArgCountMismatch {
                expected, found, ..
            } => format!("expected {expected} argument(s), found {found}"),
            InferenceDiagnostic::NeedsAnnotation { item, .. } => {
                let display = if item.name.is_empty() {
                    "this item"
                } else {
                    &item.name
                };
                format!(
                    "cannot infer the type of `{display}` across items; \
                     add a type annotation to its definition"
                )
            }
            InferenceDiagnostic::AssignToImmutable { name, .. } => {
                format!("cannot assign to `{name}`: it is not declared `mut`")
            }
            InferenceDiagnostic::AssignToItem {
                item, constness, ..
            } => match constness {
                Constness::Static => format!(
                    "cannot assign to `{}`: `static` items cannot be reassigned",
                    item.display_name()
                ),
                Constness::Const => format!(
                    "cannot assign to `{}`: a `const` is copied into each use, \
                     so there is no single place to assign to",
                    item.display_name()
                ),
            },
            InferenceDiagnostic::AssignToBuiltin { builtin, .. } => {
                format!(
                    "cannot assign to `{}`: it is a builtin function",
                    builtin.name()
                )
            }
            InferenceDiagnostic::BreakOutsideLoop { .. } => {
                "`break` outside of a loop: there is no enclosing `loop` to exit".to_owned()
            }
            InferenceDiagnostic::ContinueOutsideLoop { .. } => {
                "`continue` outside of a loop: there is no enclosing `loop` to restart".to_owned()
            }
            InferenceDiagnostic::RecordLitMissingFields { fields, .. } => {
                let list = fields
                    .iter()
                    .map(|(name, ty)| format!("`{name}: {}`", ty.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if fields.len() == 1 {
                    format!("record literal is missing field {list}")
                } else {
                    format!("record literal is missing fields {list}")
                }
            }
            InferenceDiagnostic::RecordLitExtraField { name, expected, .. } => {
                format!(
                    "no field `{name}` in expected type `{}`",
                    expected.display()
                )
            }
            InferenceDiagnostic::NoSuchField {
                name, receiver_ty, ..
            } => {
                format!("no field `{name}` on `{}`", receiver_ty.display())
            }
            InferenceDiagnostic::FieldOnUnknownType { .. } => {
                "cannot determine the type of this expression; add a type annotation".to_owned()
            }
            InferenceDiagnostic::TypeNotValue { name, .. } => {
                format!("`{name}` is a type, not a value")
            }
            InferenceDiagnostic::TypeCtorArgCount { item, found, .. } => format!(
                "`{}` takes exactly one argument (its underlying `struct` value), found {found}",
                item.display_name()
            ),
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn infer<'db>(db: &'db dyn Db, item: ItemId<'db>) -> InferenceResult {
    let body = body(db, item);
    let file = item.file(db);
    let mut table = InPlaceUnificationTable::new();
    let no_group = FxHashMap::default();
    let mut ctx;

    if let Some(root) = body.root {
        // Check the body against the item's annotation, if any.
        let ty_ref = crate::item_data(db, item)
            .as_ref()
            .and_then(|it| it.type_ref.as_ref());
        let expected = lower_type_ref(db, file, ty_ref.unwrap_or(&TypeRef::Hole), &mut table);
        let cause = ty_ref.is_some().then_some(Cause::ItemAnnotation);
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
        ctx.infer_expr_with(root, &expected, cause);
    } else {
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
    }

    ctx.finish()
}

pub(crate) struct InferCtx<'a, 'db> {
    db: &'db dyn Db,
    /// The file the body's annotations resolve type names in.
    file: SourceFile,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    table: &'a mut InPlaceUnificationTable<TyVar>,
    result: InferenceResult,
    /// Signature variables of this item's binding group (interprocedural
    /// inference): references to these items use the shared variable
    /// instead of the `signature` query, which would cycle.
    in_group: &'a FxHashMap<ItemLoc, Ty>,
    /// Deferred joins and cause provenance, solved by [`InferCtx::solve`].
    constraints: Constraints,
    /// Function-literal nesting depth (0 at the item initializer's top
    /// level): joins are tagged with it so the solver settles each function
    /// internally before anything outside consumes its type.
    scope_depth: usize,
    /// Joins being assembled during traversal (LIFO — an inner
    /// statement-position `if` completes before the construct enclosing
    /// it). See [`JoinSink`].
    join_sinks: Vec<JoinSink>,
    /// The sink the expression currently being inferred contributes to if
    /// it is a joining construct: `Some` exactly in *witness position* — a
    /// branch tail of an enclosing `if`, reached through transparent
    /// wrappers (block tails, `const` blocks). Everywhere else (`let`
    /// initializers, call arguments, statements, conditions, …) it is
    /// `None` and an `if` there resolves as its own join.
    witness_sink: Option<usize>,
    /// The loop-context stack: for each `loop` currently being inferred
    /// (innermost last), the index of the join sink its `break` values are
    /// witnesses of. Function literals and `const` blocks RESET it (a
    /// `break` never escapes those boundaries — they are units of their
    /// own); a `break`/`continue` with this empty is the outside-a-loop
    /// error.
    loop_sinks: Vec<usize>,
}

/// A join under construction. The root `if` of a nest opens one; every
/// `if`/`else` reached in witness position below it contributes its leaf
/// witnesses here instead of forming a join of its own, so the whole nest
/// resolves as ONE flat join where its value meets a non-join consumer.
/// Future joining constructs contribute witnesses through the same
/// mechanism ([`InferCtx::contribute_witness`]).
struct JoinSink {
    /// Fresh variable standing for the whole nest's type.
    result: Ty,
    witnesses: Vec<Witness>,
}

impl<'a, 'db> InferCtx<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        file: SourceFile,
        body: &'db Body,
        resolutions: &'db ArenaMap<ExprId, Resolution>,
        table: &'a mut InPlaceUnificationTable<TyVar>,
        in_group: &'a FxHashMap<ItemLoc, Ty>,
    ) -> InferCtx<'a, 'db> {
        InferCtx {
            db,
            file,
            body,
            resolutions,
            table,
            result: InferenceResult::default(),
            in_group,
            constraints: Constraints::default(),
            scope_depth: 0,
            join_sinks: Vec::new(),
            witness_sink: None,
            loop_sinks: Vec::new(),
        }
    }

    /// Solve the deferred constraints. Must run after traversal in *every*
    /// mode: group inference discards the resulting diagnostics (the
    /// per-item query re-derives them) but needs the unifications for
    /// join-typed signatures to come out concrete.
    pub(crate) fn solve(&mut self) {
        let diagnostics = self.constraints.solve(self.table);
        self.result.diagnostics.extend(diagnostics);
    }

    fn finish(mut self) -> InferenceResult {
        self.solve();
        let mut result = std::mem::take(&mut self.result);
        for (_, ty) in result.type_of_expr.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        for (_, ty) in result.type_of_binding.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        for diag in result.diagnostics.iter_mut() {
            match diag {
                InferenceDiagnostic::TypeMismatch {
                    expected, actual, ..
                }
                | InferenceDiagnostic::AllBranchesMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_fully(self.table, expected);
                    *actual = resolve_fully(self.table, actual);
                }
                InferenceDiagnostic::NotCallable { ty, .. } => {
                    *ty = resolve_fully(self.table, ty);
                }
                InferenceDiagnostic::IfBranchMismatch {
                    then_ty, else_ty, ..
                } => {
                    *then_ty = resolve_fully(self.table, then_ty);
                    *else_ty = resolve_fully(self.table, else_ty);
                }
                InferenceDiagnostic::RecordLitMissingFields { fields, .. } => {
                    for (_, ty) in fields.iter_mut() {
                        *ty = resolve_fully(self.table, ty);
                    }
                }
                InferenceDiagnostic::RecordLitExtraField { expected, .. } => {
                    *expected = resolve_fully(self.table, expected);
                }
                InferenceDiagnostic::NoSuchField { receiver_ty, .. } => {
                    *receiver_ty = resolve_fully(self.table, receiver_ty);
                }
                InferenceDiagnostic::ArgCountMismatch { .. }
                | InferenceDiagnostic::NeedsAnnotation { .. }
                | InferenceDiagnostic::AssignToImmutable { .. }
                | InferenceDiagnostic::AssignToItem { .. }
                | InferenceDiagnostic::AssignToBuiltin { .. }
                | InferenceDiagnostic::FieldOnUnknownType { .. }
                | InferenceDiagnostic::TypeNotValue { .. }
                | InferenceDiagnostic::TypeCtorArgCount { .. }
                | InferenceDiagnostic::BreakOutsideLoop { .. }
                | InferenceDiagnostic::ContinueOutsideLoop { .. } => {}
            }
        }
        result
    }

    fn fresh_var(&mut self) -> Ty {
        Ty::Infer(self.table.new_key(TyVarValue::Unknown))
    }

    /// Infer `expr`; if `expected` is given, check against it (recording a
    /// diagnostic on mismatch and recovering with the expected type).
    pub(crate) fn infer_expr(&mut self, expr: ExprId, expected: &Ty) -> Ty {
        self.infer_expr_with(expr, expected, None)
    }

    /// As [`InferCtx::infer_expr`], with the [`Cause`] that makes `expected`
    /// expected — recorded when the expectation binds a type variable, and
    /// cited when the check fails outright.
    fn infer_expr_with(&mut self, expr: ExprId, expected: &Ty, cause: Option<Cause>) -> Ty {
        // A join sink propagates only through transparent wrappers (block
        // tails, `const` blocks) into an `if`'s witness position. Any other
        // expression is a leaf of the enclosing join, and its
        // subexpressions are ordinary non-witness positions — an `if`
        // inside a call argument or a statement resolves as its own join.
        let sink = self.witness_sink.take();
        let ty = match &self.body.exprs[expr] {
            ExprData::Missing => Ty::Error,
            ExprData::Literal(LiteralData::Int(_)) => Ty::Int,
            ExprData::Literal(LiteralData::Str(_)) => Ty::Str,
            ExprData::Literal(LiteralData::Bool(_)) => Ty::Bool,
            ExprData::NameRef(name) => match self.resolutions.get(expr) {
                // A type is not a first-class value. The one legal
                // expression position for a type name — the head of a
                // construction call — is intercepted in the `Call` arm and
                // never infers the callee, so reaching this *is* the error.
                Some(Resolution::TypeItem(_)) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::TypeNotValue {
                            expr,
                            name: name.clone(),
                        });
                    Ty::Error
                }
                Some(Resolution::Local(binding)) => self
                    .result
                    .type_of_binding
                    .get(*binding)
                    .cloned()
                    .unwrap_or(Ty::Error),
                Some(Resolution::Item(loc)) => {
                    // A member of this item's own binding group resolves to
                    // its shared signature variable — that's interprocedural
                    // inference happening.
                    if let Some(member_sig) = self.in_group.get(loc) {
                        member_sig.clone()
                    } else {
                        let target = loc.to_id(self.db);
                        let sig = signature(self.db, target);
                        // Inference couldn't determine the signature from
                        // the definition: that's only visible from uses (an
                        // unused undetermined item is fine), so the
                        // diagnostic lives here.
                        if signature_needs_annotation(self.db, target) {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NeedsAnnotation {
                                    expr,
                                    item: loc.clone(),
                                });
                        }
                        sig
                    }
                }
                // No one signature a use could take on; the duplicate
                // definitions carry the diagnostic.
                Some(Resolution::Ambiguous(_)) => Ty::Error,
                Some(Resolution::Builtin(builtin)) => builtin_type(*builtin),
                None => Ty::Error, // unresolved: already diagnosed by name resolution
            },
            ExprData::Call { callee, args } => {
                // A construction call: the type name used as a plain
                // constructor function taking the underlying record —
                // `Foo(struct { x: 1 })`. Intercepted before the callee is
                // inferred (a bare type name in expression position is an
                // error; as a construction head it is the one legal use).
                if let Some(Resolution::TypeItem(loc)) = self.resolutions.get(*callee).cloned() {
                    return self.infer_construction(expr, *callee, &loc, args, expected, cause);
                }
                let fresh = self.fresh_var();
                let callee_ty = self.infer_expr(*callee, &fresh);
                match self.resolve_shallow(&callee_ty) {
                    // The callee's type is still being inferred (an
                    // in-group signature, e.g. mutual recursion): calling
                    // it commits it to a function of this shape.
                    Ty::Infer(var) => {
                        let params: Vec<Ty> = args.iter().map(|_| self.fresh_var()).collect();
                        let ret = self.fresh_var();
                        if self.unify(&Ty::Infer(var), &Ty::fn_type(params.clone(), ret.clone())) {
                            for (i, &arg) in args.iter().enumerate() {
                                self.infer_expr(arg, &params[i]);
                            }
                            ret
                        } else {
                            // The commitment contradicts what the callee's
                            // signature is already committed to: poison it
                            // so the member reports via the needs-annotation
                            // path instead of publishing a guess.
                            self.table.union_value(var, TyVarValue::Known(Ty::Error));
                            for &arg in args {
                                let arg_fresh = self.fresh_var();
                                self.infer_expr(arg, &arg_fresh);
                            }
                            Ty::Error
                        }
                    }
                    Ty::Fn(f) => {
                        if f.params.len() != args.len() {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::ArgCountMismatch {
                                    expr,
                                    expected: f.params.len(),
                                    found: args.len(),
                                });
                        }
                        for (i, &arg) in args.iter().enumerate() {
                            // The parameter type is an axiom: the cause is
                            // recorded when it binds a variable (blaming a
                            // branch through this call) and cited on direct
                            // mismatches (where the renderer drops it as
                            // self-evident: the call encloses the argument).
                            let param = f.params.get(i).cloned().unwrap_or(Ty::Error);
                            self.infer_expr_with(
                                arg,
                                &param,
                                Some(Cause::CallSite { call: expr, arg }),
                            );
                        }
                        f.ret.clone()
                    }
                    Ty::Error => {
                        for &arg in args {
                            let arg_fresh = self.fresh_var();
                            self.infer_expr(arg, &arg_fresh);
                        }
                        Ty::Error
                    }
                    // Evaluating the callee already diverges, so the call
                    // diverges; `{error}` here would be an error type with
                    // no diagnostic to explain it.
                    Ty::Never => {
                        for &arg in args {
                            let arg_fresh = self.fresh_var();
                            // TODO: If we cannot infer the type of something, but we can see it's
                            // unreachable, it would be ok for it to be a warning rather than an
                            // error
                            self.infer_expr(arg, &arg_fresh);
                        }
                        Ty::Never
                    }
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NotCallable {
                                expr: *callee,
                                ty: other,
                            });
                        for &arg in args {
                            let fresh_var = self.fresh_var();
                            self.infer_expr(arg, &fresh_var);
                        }
                        Ty::Error
                    }
                }
            }
            ExprData::Bin { op, lhs, rhs } => {
                use crate::body::BinOp::*;
                match op {
                    Some(Add | Sub | Mul | Div) => {
                        self.infer_expr_with(*lhs, &Ty::Int, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &Ty::Int, Some(Cause::Operator(expr)));
                        Ty::Int
                    }
                    Some(Lt | Le | Gt | Ge) => {
                        self.infer_expr_with(*lhs, &Ty::Int, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &Ty::Int, Some(Cause::Operator(expr)));
                        Ty::Bool
                    }
                    // Equality works on any type; the operands just have to
                    // agree with each other — whatever the first operand
                    // concluded, the second must match.
                    Some(Eq | Ne) => {
                        let fresh = self.fresh_var();
                        self.infer_expr(*lhs, &fresh);
                        self.infer_expr_with(*rhs, &fresh, Some(Cause::Operand(*lhs)));
                        Ty::Bool
                    }
                    // No operator token means broken source with its own
                    // parse error.
                    None => {
                        let lhs_fresh = self.fresh_var();
                        let rhs_fresh = self.fresh_var();
                        self.infer_expr(*lhs, &lhs_fresh);
                        self.infer_expr(*rhs, &rhs_fresh);
                        Ty::Error
                    }
                }
            }
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.infer_expr_with(*condition, &Ty::Bool, Some(Cause::Condition(expr)));
                let Some(else_branch) = else_branch else {
                    // Without an `else`, the value when the condition is
                    // false is `()`, so the then-branch must be too.
                    self.infer_expr_with(*then_branch, &Ty::Unit, Some(Cause::MissingElse(expr)));
                    let ty = self.check(expr, Ty::Unit, expected, cause);
                    self.result.type_of_expr.insert(expr, ty.clone());
                    return ty;
                };
                // Statement vs. witness position, decided by `sink`.
                // Reached through a transparent tail chain from an
                // enclosing `if`'s branch, this `if` is a witness: its
                // leaves join the enclosing sink and the whole nest
                // resolves as one flat join where the outermost `if`'s
                // value meets a non-join consumer. Anywhere else the join
                // resolves here: open a fresh sink.
                let (sink_index, is_root) = match sink {
                    Some(index) => (index, false),
                    None => {
                        let result = self.fresh_var();
                        self.join_sinks.push(JoinSink {
                            result,
                            witnesses: Vec::new(),
                        });
                        (self.join_sinks.len() - 1, true)
                    }
                };
                // Each branch gets an independent fresh variable so outer
                // expectation pressure never leaks in: the branches' honest
                // types are what the join judges.
                let then_fresh = self.fresh_var();
                self.witness_sink = Some(sink_index);
                let then_ty = self.infer_expr(*then_branch, &then_fresh);
                self.contribute_witness(sink_index, *then_branch, &then_ty);
                let else_fresh = self.fresh_var();
                self.witness_sink = Some(sink_index);
                let else_ty = self.infer_expr(*else_branch, &else_fresh);
                self.contribute_witness(sink_index, *else_branch, &else_ty);
                let diverges = matches!(self.resolve_shallow(&then_ty), Ty::Never)
                    && matches!(self.resolve_shallow(&else_ty), Ty::Never);
                if !is_root {
                    // A nested `if` types as the enclosing join's result:
                    // hover on it shows the whole nest's resolved type —
                    // there is no intermediate "the inner if has type …"
                    // verdict of its own.
                    if diverges {
                        Ty::Never
                    } else {
                        self.join_sinks[sink_index].result.clone()
                    }
                } else {
                    let JoinSink { result, witnesses } =
                        self.join_sinks.pop().expect("sink pushed above");
                    match witnesses.len() {
                        // Every leaf diverges: so does the `if`.
                        0 => {
                            self.unify(&result, &Ty::Never);
                            Ty::Never
                        }
                        // One surviving leaf (the rest diverged): its type
                        // is the `if`'s type outright, no agreement left to
                        // defer. Unified into `result` so nested `if`s that
                        // routed the leaf here still resolve.
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.unify(&result, &ty);
                            ty
                        }
                        // Leaf agreement is a join: deferred so axioms
                        // arriving later in the traversal (an annotation
                        // above, the call this feeds into below) pick the
                        // winner before the leaves are played against each
                        // other.
                        _ => {
                            self.constraints.push_join(Join {
                                expr,
                                depth: self.scope_depth,
                                result: result.clone(),
                                witnesses,
                            });
                            result
                        }
                    }
                }
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { binding, init } => {
                            let has_annotation = self.body.bindings[*binding].type_ref.is_some();
                            let declared = self.body.bindings[*binding]
                                .type_ref
                                .as_ref()
                                .map(|it| lower_type_ref(self.db, self.file, it, self.table))
                                .unwrap_or_else(|| self.fresh_var());
                            let binding_cause = has_annotation.then_some(Cause::Binding(*binding));
                            let ty = self.infer_expr_with(*init, &declared, binding_cause);
                            self.result.type_of_binding.insert(*binding, ty);
                        }
                        Stmt::Assign { target, value } => {
                            // The target is an ordinary read for typing
                            // purposes — it just so happens to also be
                            // written. Its own expectation is free (no
                            // outer axiom constrains what's being assigned
                            // to); the resulting type is what the value
                            // must match, same as a `let` with a declared
                            // type flows its annotation down as `expected`
                            // with `Cause::Binding`.
                            let fresh = self.fresh_var();
                            let target_ty = self.infer_expr(*target, &fresh);
                            // Enforcement: the only legal target is a `mut`
                            // local. Unresolved names already carry the
                            // unresolved-name diagnostic, non-name targets a
                            // validation error, and ambiguous names the
                            // duplicate-definition diagnostics — none of
                            // those gets a second squiggle here.
                            let cause = match self.resolutions.get(*target) {
                                Some(Resolution::Local(binding)) => {
                                    let data = &self.body.bindings[*binding];
                                    if !data.mutable {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::AssignToImmutable {
                                                target: *target,
                                                binding: *binding,
                                                name: data.name.clone(),
                                            },
                                        );
                                    }
                                    Some(Cause::Binding(*binding))
                                }
                                Some(Resolution::Item(loc)) => {
                                    let constness = crate::item_data(self.db, loc.to_id(self.db))
                                        .as_ref()
                                        .and_then(|it| it.kind.constness())
                                        .unwrap_or(Constness::Static);
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::AssignToItem {
                                            target: *target,
                                            item: loc.clone(),
                                            constness,
                                        },
                                    );
                                    None
                                }
                                // The target read already reported "is a
                                // type, not a value" (the `NameRef` arm ran
                                // on it); a second assignment-specific
                                // squiggle on the same name adds nothing.
                                Some(Resolution::TypeItem(_)) => None,
                                Some(Resolution::Builtin(builtin)) => {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::AssignToBuiltin {
                                            target: *target,
                                            builtin: *builtin,
                                        },
                                    );
                                    None
                                }
                                Some(Resolution::Ambiguous(_)) | None => None,
                            };
                            self.infer_expr_with(*value, &target_ty, cause);
                        }
                        Stmt::Expr(e) => {
                            let fresh_var = self.fresh_var();
                            self.infer_expr(*e, &fresh_var);
                        }
                    }
                }
                match tail {
                    // Propagate the expectation so mismatches point at the
                    // tail expression, then skip re-checking at block level.
                    Some(tail) => {
                        // The join sink survives only along the tail chain
                        // (witness position); the statements above were
                        // ordinary non-witness positions.
                        self.witness_sink = sink;
                        let ty = self.infer_expr_with(*tail, expected, cause);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        return ty;
                    }
                    None => Ty::Unit,
                }
            }
            // Fully transparent for typing: same expectation and cause flow
            // straight through to `body`, so a mismatch is reported (and
            // blamed) exactly as if the `const` wrapper weren't there. This
            // expression still gets its own entry in `type_of_expr` (the
            // early return below), so hover on the `const { ... }` itself
            // shows the right type.
            ExprData::ConstBlock { body: inner } => {
                // Transparent for the join sink too, matching
                // `peel_blocks`: an `if` at a `const` block's core is still
                // in witness position.
                // NOT transparent for the loop context: a `const` block is
                // a compile-time unit of its own (MIR lowers it to a
                // separate body), so a `break` inside it cannot exit a loop
                // outside it.
                self.witness_sink = sink;
                let saved_loops = std::mem::take(&mut self.loop_sinks);
                let ty = self.infer_expr_with(*inner, expected, cause);
                self.loop_sinks = saved_loops;
                self.result.type_of_expr.insert(expr, ty.clone());
                return ty;
            }
            ExprData::RecordLit { fields } => {
                // Bidirectional: when the context already expects a record,
                // the expected field types flow into the field initializers
                // (with the same cause, so a wrong field blames the field's
                // expression and cites the annotation/call that demanded the
                // type — same shape as call arguments and annotated lets).
                if let Ty::Record(expected_rec) = self.resolve_shallow(expected) {
                    let has = |name: &str| fields.iter().any(|(n, _)| n.as_str() == name);
                    let missing: Vec<(String, Ty)> = expected_rec
                        .fields
                        .iter()
                        .filter(|(name, _)| !has(name))
                        .cloned()
                        .collect();
                    if !missing.is_empty() {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::RecordLitMissingFields {
                                expr,
                                fields: missing,
                            });
                    }
                    let mut seen: Vec<&str> = Vec::new();
                    for (name, field_expr) in fields {
                        match expected_rec.field_ty(name) {
                            Some(field_ty) => {
                                self.infer_expr_with(*field_expr, &field_ty.clone(), cause);
                            }
                            None => {
                                // An extra field (exact field-set equality:
                                // nothing is dropped silently). Duplicates of
                                // one extra name get a single diagnostic —
                                // validation already flags the duplication.
                                if !seen.contains(&name.as_str()) {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::RecordLitExtraField {
                                            expr: *field_expr,
                                            name: name.clone(),
                                            expected: Ty::Record(expected_rec.clone()),
                                        },
                                    );
                                }
                                let fresh = self.fresh_var();
                                self.infer_expr(*field_expr, &fresh);
                            }
                        }
                        seen.push(name.as_str());
                    }
                    // Field mismatches were reported above (or the sets
                    // match); either way the literal recovers with the
                    // expected type so nothing cascades.
                    let ty = Ty::Record(expected_rec);
                    self.result.type_of_expr.insert(expr, ty.clone());
                    return ty;
                }
                // No record expectation: infer every field and conclude a
                // record type, then let the ordinary check judge it (binding
                // a free variable, or reporting a plain mismatch against a
                // non-record expectation).
                let field_tys = fields
                    .iter()
                    .map(|(name, field_expr)| {
                        let fresh = self.fresh_var();
                        (name.clone(), self.infer_expr(*field_expr, &fresh))
                    })
                    .collect();
                Ty::record(field_tys)
            }
            ExprData::Field { receiver, name } => {
                let fresh = self.fresh_var();
                let receiver_ty = self.infer_expr(*receiver, &fresh);
                if name.is_empty() {
                    // `a.` — the parse error covers it.
                    Ty::Error
                } else {
                    match self.resolve_shallow(&receiver_ty) {
                        Ty::Record(rec) => match rec.field_ty(name) {
                            Some(field_ty) => field_ty.clone(),
                            None => {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::NoSuchField {
                                        expr,
                                        name: name.clone(),
                                        receiver_ty: Ty::Record(rec),
                                    });
                                Ty::Error
                            }
                        },
                        // A named type projects through to its declared
                        // shape: `p.x` on a `Foo` works exactly as on the
                        // underlying record.
                        Ty::Named(loc) => {
                            match type_underlying(self.db, loc.to_id(self.db)) {
                                Some(Ty::Record(rec)) => match rec.field_ty(name) {
                                    Some(field_ty) => field_ty.clone(),
                                    None => {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::NoSuchField {
                                                expr,
                                                name: name.clone(),
                                                receiver_ty: Ty::Named(loc),
                                            },
                                        );
                                        Ty::Error
                                    }
                                },
                                // Broken declaration: its own diagnostic
                                // sits at the declaration site; stay silent.
                                _ => Ty::Error,
                            }
                        }
                        // Evaluating the receiver already diverges (same
                        // reasoning as a diverging callee).
                        Ty::Never => Ty::Never,
                        // Structural equality can't run backwards from a
                        // field name, so an undetermined receiver stays
                        // undetermined: ask for the annotation.
                        Ty::Infer(_) => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::FieldOnUnknownType {
                                    expr,
                                    receiver: *receiver,
                                });
                            Ty::Error
                        }
                        // Errors are infectious and silent — a broken
                        // receiver must not cascade into field diagnostics.
                        broken if broken.contains_error() => Ty::Error,
                        other => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NoSuchField {
                                    expr,
                                    name: name.clone(),
                                    receiver_ty: other,
                                });
                            Ty::Error
                        }
                    }
                }
            }
            ExprData::FnLiteral {
                params,
                ret_type,
                body: fn_body,
                // Const-checking is a separate pass (`const_check`);
                // `is_const` doesn't affect typing here.
                is_const: _,
            } => {
                let param_tys: Vec<Ty> = params
                    .iter()
                    .map(|&param| {
                        let ty = match &self.body.bindings[param].type_ref {
                            Some(type_ref) => {
                                lower_type_ref(self.db, self.file, type_ref, self.table)
                            }
                            None => self.fresh_var(),
                        };
                        self.result.type_of_binding.insert(param, ty.clone());
                        ty
                    })
                    .collect();
                let ret = match ret_type {
                    Some(type_ref) => lower_type_ref(self.db, self.file, type_ref, self.table),
                    None => self.fresh_var(),
                };
                let ret_cause = ret_type.is_some().then_some(Cause::ReturnAnnotation(expr));
                // The function is a unit that must be internally
                // consistent: its joins solve (by depth) before any outer
                // join consumes its type, and they never flatten into one —
                // the sink was not restored for the body, so a function
                // literal in witness position is one opaque leaf and its
                // body-tail `if` is a root join of its own (the return type
                // is the boundary the join resolves against).
                // The loop context resets the same way: a `break` in the
                // body never exits a loop enclosing the literal (fns bound
                // everything).
                self.scope_depth += 1;
                let saved_loops = std::mem::take(&mut self.loop_sinks);
                self.infer_expr_with(*fn_body, &ret, ret_cause);
                self.loop_sinks = saved_loops;
                self.scope_depth -= 1;
                Ty::fn_type(param_tys, ret)
            }
            ExprData::Loop { body: loop_body } => {
                // The BREAK VALUES are the witnesses of one join whose
                // result is the loop's type. Statement vs. witness
                // position, exactly as for `if`: a loop in witness position
                // contributes its break values to the enclosing join,
                // anywhere else it resolves a join of its own.
                let (sink_index, is_root) = match sink {
                    Some(index) => (index, false),
                    None => {
                        let result = self.fresh_var();
                        self.join_sinks.push(JoinSink {
                            result,
                            witnesses: Vec::new(),
                        });
                        (self.join_sinks.len() - 1, true)
                    }
                };
                let witnesses_before = self.join_sinks[sink_index].witnesses.len();
                // The body's own tail value is discarded — running off the
                // body's end continues the loop — so it is inferred free:
                // only `break` produces the loop's value.
                self.loop_sinks.push(sink_index);
                let body_fresh = self.fresh_var();
                self.infer_expr(*loop_body, &body_fresh);
                self.loop_sinks.pop();
                // Everything contributed to the sink during the body came
                // from this loop's breaks (nested constructs in statement
                // position open sinks of their own, LIFO): no contribution
                // means no break ever carries a value out — the loop never
                // finishes, `!`, and the never machinery (widening, no
                // vote) takes it from there.
                let broke_with_value =
                    self.join_sinks[sink_index].witnesses.len() > witnesses_before;
                if !is_root {
                    // A nested loop types as the enclosing join's result,
                    // exactly like a nested `if`.
                    if broke_with_value {
                        self.join_sinks[sink_index].result.clone()
                    } else {
                        Ty::Never
                    }
                } else {
                    let JoinSink { result, witnesses } =
                        self.join_sinks.pop().expect("sink pushed above");
                    match witnesses.len() {
                        // No break carries a value out: an infinite loop.
                        0 => {
                            self.unify(&result, &Ty::Never);
                            Ty::Never
                        }
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.unify(&result, &ty);
                            ty
                        }
                        _ => {
                            self.constraints.push_join(Join {
                                expr,
                                depth: self.scope_depth,
                                result: result.clone(),
                                witnesses,
                            });
                            result
                        }
                    }
                }
            }
            ExprData::Break { value } => match self.loop_sinks.last().copied() {
                Some(sink_index) => {
                    match value {
                        Some(value) => {
                            // The value sits in witness position of the
                            // loop's join: an `if`/`loop` at its core
                            // flattens its leaves into the same join, so
                            // blame speaks about the leaves.
                            let fresh = self.fresh_var();
                            self.witness_sink = Some(sink_index);
                            let value_ty = self.infer_expr(*value, &fresh);
                            self.contribute_witness(sink_index, *value, &value_ty);
                        }
                        // `break;` is a witness of type `()`.
                        None => self.contribute_witness(sink_index, expr, &Ty::Unit),
                    }
                    // The break expression itself never produces a value.
                    Ty::Never
                }
                None => {
                    // Outside any loop (or across a fn-literal/`const`
                    // block boundary). The value is still inferred so its
                    // contents get types and diagnostics.
                    if let Some(value) = value {
                        let fresh = self.fresh_var();
                        self.infer_expr(*value, &fresh);
                    }
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::BreakOutsideLoop { expr });
                    Ty::Error
                }
            },
            ExprData::Continue => {
                if self.loop_sinks.is_empty() {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::ContinueOutsideLoop { expr });
                    Ty::Error
                } else {
                    Ty::Never
                }
            }
        };

        let ty = self.check(expr, ty, expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// A construction call `Foo(arg)`: type-check the single argument
    /// against the declared underlying record bidirectionally (the declared
    /// field types flow into a literal argument's fields, blame cites the
    /// declaration via [`Cause::Constructor`]) and produce [`Ty::Named`].
    fn infer_construction(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        loc: &ItemLoc,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        // `None` when the declaration is broken (RHS not a `struct`
        // literal): the declaration site carries the diagnostic, so the
        // argument is checked against `{error}` — infectious and silent.
        let underlying = type_underlying(self.db, loc.to_id(self.db)).unwrap_or(Ty::Error);
        // The constructor *is* a function value conceptually; give the
        // callee name that type so hover on `Foo` in `Foo(...)` is honest.
        self.result.type_of_expr.insert(
            callee,
            Ty::fn_type(vec![underlying.clone()], Ty::Named(loc.clone())),
        );
        if args.len() != 1 {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::TypeCtorArgCount {
                    expr,
                    item: loc.clone(),
                    found: args.len(),
                });
        }
        for (i, &arg) in args.iter().enumerate() {
            if i == 0 {
                self.infer_expr_with(arg, &underlying, Some(Cause::Constructor(expr)));
            } else {
                // Surplus arguments (already diagnosed): infer freely so
                // their contents still get types and diagnostics.
                let fresh = self.fresh_var();
                self.infer_expr(arg, &fresh);
            }
        }
        let ty = self.check(expr, Ty::Named(loc.clone()), expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// Register the value of a branch as a witness of the join being
    /// assembled in `sink` — the witness-contribution seam every joining
    /// construct plugs into.
    ///
    /// Two kinds of branch contribute nothing: a diverging branch (it
    /// doesn't vote, it widens — the join is decided by the surviving
    /// leaves alone), and a branch whose tail is itself an `if`/`else` or a
    /// `loop` (it was inferred with this sink as its witness position, so
    /// its leaves — a loop's break values — are already in; that's the
    /// flattening).
    fn contribute_witness(&mut self, sink: usize, branch: ExprId, ty: &Ty) {
        if matches!(self.resolve_shallow(ty), Ty::Never) {
            return;
        }
        let blame = peel_blocks(self.body, branch);
        if matches!(
            self.body.exprs[blame],
            ExprData::If {
                else_branch: Some(_),
                ..
            } | ExprData::Loop { .. }
        ) {
            return;
        }
        self.join_sinks[sink].witnesses.push(Witness {
            blame,
            ty: ty.clone(),
        });
    }

    /// Check `actual` against `expected`; on mismatch, report on `expr` and
    /// recover with the expected type (trust the annotation).
    ///
    /// A successful unification that binds a type variable records `cause`
    /// in the constraint store — that's how the join solver later knows why
    /// an `if`'s result type was decided.
    fn check(&mut self, expr: ExprId, actual: Ty, expected: &Ty, cause: Option<Cause>) -> Ty {
        // `!` coerces to anything — but only on the actual side.
        if matches!(self.resolve_shallow(&actual), Ty::Never) {
            match self.resolve_shallow(expected) {
                Ty::Never => {}
                Ty::Infer(_) => return actual,
                _ => return expected.clone(),
            }
        }
        if self.constraints.unify(self.table, &actual, expected, cause) {
            actual
        } else {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::TypeMismatch {
                    expr,
                    expected: expected.clone(),
                    actual,
                    reasons: cause.into_iter().collect(),
                });
            expected.clone()
        }
    }

    pub(crate) fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        self.constraints.unify(self.table, a, b, None)
    }

    fn resolve_shallow(&mut self, ty: &Ty) -> Ty {
        constraint::resolve_shallow(self.table, ty)
    }
}

/// Dig through block and `const` block wrappers to the value-producing
/// sub-expression that should carry a squiggle — both are transparent, so
/// blame belongs on whatever is actually inside.
fn peel_blocks(body: &Body, mut expr: ExprId) -> ExprId {
    loop {
        expr = match &body.exprs[expr] {
            ExprData::Block {
                tail: Some(tail), ..
            } => *tail,
            ExprData::ConstBlock { body: inner } => *inner,
            _ => return expr,
        };
    }
}

fn builtin_type(builtin: Builtin) -> Ty {
    match builtin {
        Builtin::Print => Ty::fn_type(vec![Ty::Str], Ty::Unit),
        Builtin::Panic => Ty::fn_type(vec![Ty::Str], Ty::Never),
    }
}
