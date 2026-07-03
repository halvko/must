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
            | InferenceDiagnostic::TypeCtorArgCount { expr, .. } => *expr,
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
    /// The function literal currently being traversed (`None` at the item
    /// initializer's top level) and its nesting depth: joins are tagged
    /// with these so the solver treats each function as a unit.
    scope: Option<ExprId>,
    scope_depth: usize,
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
            scope: None,
            scope_depth: 0,
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
                | InferenceDiagnostic::TypeCtorArgCount { .. } => {}
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
                // Each branch gets an independent fresh variable so outer
                // expectation pressure never leaks in: the branches' honest
                // types are what the join judges.
                let then_fresh = self.fresh_var();
                let then_ty = self.infer_expr(*then_branch, &then_fresh);
                let else_fresh = self.fresh_var();
                let else_ty = self.infer_expr(*else_branch, &else_fresh);
                // A diverging branch takes the other branch's type.
                if matches!(self.resolve_shallow(&then_ty), Ty::Never) {
                    else_ty
                } else if matches!(self.resolve_shallow(&else_ty), Ty::Never) {
                    then_ty
                } else {
                    // Branch agreement is a join: deferred so axioms arriving
                    // later in the traversal (an annotation above, the call
                    // this feeds into below) pick the winner before the
                    // branches are played against each other.
                    let result_ty = self.fresh_var();
                    self.constraints.push_join(Join {
                        expr,
                        scope: self.scope,
                        depth: self.scope_depth,
                        result: result_ty.clone(),
                        witnesses: vec![
                            Witness {
                                blame: peel_blocks(self.body, *then_branch),
                                ty: then_ty,
                            },
                            Witness {
                                blame: peel_blocks(self.body, *else_branch),
                                ty: else_ty,
                            },
                        ],
                    });
                    result_ty
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
                let ty = self.infer_expr_with(*inner, expected, cause);
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
                // The body's joins belong to this function's scope: the
                // function is a unit that must be internally consistent, so
                // they solve before — and never flatten into — any join
                // outside it.
                let outer = self.scope.replace(expr);
                self.scope_depth += 1;
                self.infer_expr_with(*fn_body, &ret, ret_cause);
                self.scope_depth -= 1;
                self.scope = outer;
                Ty::fn_type(param_tys, ret)
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
