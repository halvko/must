//! Type inference: a pure per-body query.
//!
//! Other items are seen only through [`signature`]; builtins through a fixed
//! table. The [`InferCtx`] owns an ena unification table — today every
//! expectation is checked eagerly via `unify`, which is exactly the seam
//! where deferred constraints + fixpoint solving slot in later without
//! changing the query graph.

use base_db::Db;
use ena::unify::InPlaceUnificationTable;
use la_arena::ArenaMap;

use crate::body::{Body, BindingId, ExprData, ExprId, LiteralData, Stmt, body};
use crate::scopes::{Builtin, Resolution, duplicated_names, resolutions};
use crate::ty::{Ty, TyVar, TyVarValue, lower_type_ref, signature};
use crate::{ItemId, item_loc};
use crate::item_tree::item_tree;

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
}

#[salsa::tracked(returns(ref))]
pub fn infer<'db>(db: &'db dyn Db, item: ItemId<'db>) -> InferenceResult {
    let body = body(db, item);
    let mut ctx = InferCtx {
        db,
        body,
        resolutions: resolutions(db, item),
        duplicated_names: duplicated_names(db, item.file(db)),
        table: InPlaceUnificationTable::new(),
        result: InferenceResult::default(),
    };

    if let Some(root) = body.root {
        // Check the body against the item's annotation, if any.
        let loc = item_loc(db, item);
        let expected = item_tree(db, item.file(db))
            .items
            .get(loc.index as usize)
            .and_then(|it| it.type_ref.as_ref())
            .map(lower_type_ref);
        ctx.infer_expr(root, expected.as_ref());
    }

    ctx.finish()
}

struct InferCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    duplicated_names: &'db rustc_hash::FxHashSet<String>,
    table: InPlaceUnificationTable<TyVar>,
    result: InferenceResult,
}

impl InferCtx<'_> {
    fn finish(mut self) -> InferenceResult {
        let mut result = std::mem::take(&mut self.result);
        for (_, ty) in result.type_of_expr.iter_mut() {
            *ty = resolve_fully(&mut self.table, ty);
        }
        for (_, ty) in result.type_of_binding.iter_mut() {
            *ty = resolve_fully(&mut self.table, ty);
        }
        for diag in result.diagnostics.iter_mut() {
            match diag {
                InferenceDiagnostic::TypeMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_fully(&mut self.table, expected);
                    *actual = resolve_fully(&mut self.table, actual);
                }
                InferenceDiagnostic::NotCallable { ty, .. } => {
                    *ty = resolve_fully(&mut self.table, ty);
                }
                InferenceDiagnostic::ArgCountMismatch { .. } => {}
            }
        }
        result
    }

    fn fresh_var(&mut self) -> Ty {
        Ty::Infer(self.table.new_key(TyVarValue::Unknown))
    }

    /// Infer `expr`; if `expected` is given, check against it (recording a
    /// diagnostic on mismatch and recovering with the expected type).
    fn infer_expr(&mut self, expr: ExprId, expected: Option<&Ty>) -> Ty {
        let ty = match &self.body.exprs[expr] {
            ExprData::Missing => Ty::Error,
            ExprData::Literal(LiteralData::Int(_)) => Ty::Int,
            ExprData::Literal(LiteralData::Str(_)) => Ty::Str,
            ExprData::NameRef(name) => match self.resolutions.get(expr) {
                Some(&Resolution::Local(binding)) => self
                    .result
                    .type_of_binding
                    .get(binding)
                    .cloned()
                    .unwrap_or(Ty::Error),
                // An ambiguously-defined name has no one signature a use
                // could take on; the silent error type keeps downstream
                // checks quiet (the duplicate definition carries the
                // diagnostic).
                Some(&Resolution::Item(_)) if self.duplicated_names.contains(name) => Ty::Error,
                Some(&Resolution::Item(loc)) => match loc.to_id(self.db) {
                    Some(item) => signature(self.db, item),
                    None => Ty::Error,
                },
                Some(&Resolution::Builtin(builtin)) => builtin_type(builtin),
                None => Ty::Error, // unresolved: already diagnosed by name resolution
            },
            ExprData::Call { callee, args } => {
                let callee_ty = self.infer_expr(*callee, None);
                match self.resolve_shallow(&callee_ty) {
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
                            let param = f.params.get(i).cloned();
                            self.infer_expr(arg, param.as_ref());
                        }
                        f.ret.clone()
                    }
                    Ty::Error | Ty::Never => {
                        for &arg in args {
                            self.infer_expr(arg, None);
                        }
                        Ty::Error
                    }
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NotCallable {
                                expr: *callee,
                                ty: other,
                            });
                        for &arg in args {
                            self.infer_expr(arg, None);
                        }
                        Ty::Error
                    }
                }
            }
            ExprData::Bin { lhs, rhs, .. } => {
                self.infer_expr(*lhs, Some(&Ty::Int));
                self.infer_expr(*rhs, Some(&Ty::Int));
                Ty::Int
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { binding, init } => {
                            let declared = self.body.bindings[*binding]
                                .type_ref
                                .as_ref()
                                .map(lower_type_ref);
                            let ty = match declared {
                                Some(declared) => {
                                    self.infer_expr(*init, Some(&declared));
                                    declared
                                }
                                None => self.infer_expr(*init, None),
                            };
                            self.result.type_of_binding.insert(*binding, ty);
                        }
                        Stmt::Expr(e) => {
                            self.infer_expr(*e, None);
                        }
                    }
                }
                match tail {
                    // Propagate the expectation so mismatches point at the
                    // tail expression, then skip re-checking at block level.
                    Some(tail) => {
                        let ty = self.infer_expr(*tail, expected);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        return ty;
                    }
                    None => Ty::Unit,
                }
            }
            ExprData::FnLiteral {
                params,
                ret_type,
                body: fn_body,
            } => {
                let param_tys: Vec<Ty> = params
                    .iter()
                    .map(|&param| {
                        let ty = match &self.body.bindings[param].type_ref {
                            Some(type_ref) => lower_type_ref(type_ref),
                            None => self.fresh_var(),
                        };
                        self.result.type_of_binding.insert(param, ty.clone());
                        ty
                    })
                    .collect();
                let ret = match ret_type {
                    Some(type_ref) => lower_type_ref(type_ref),
                    None => self.fresh_var(),
                };
                self.infer_expr(*fn_body, Some(&ret));
                Ty::fn_type(param_tys, ret)
            }
        };

        let ty = match expected {
            Some(expected) => self.check(expr, ty, expected),
            None => ty,
        };
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// Check `actual` against `expected`; on mismatch, report on `expr` and
    /// recover with the expected type (trust the annotation).
    fn check(&mut self, expr: ExprId, actual: Ty, expected: &Ty) -> Ty {
        // `!` coerces to anything — but only on the actual side.
        if matches!(self.resolve_shallow(&actual), Ty::Never)
            && !matches!(self.resolve_shallow(expected), Ty::Never)
        {
            return expected.clone();
        }
        if self.unify(&actual, expected) {
            actual
        } else {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::TypeMismatch {
                    expr,
                    expected: expected.clone(),
                    actual,
                });
            expected.clone()
        }
    }

    /// Eager constraint solving: this is the call site that becomes
    /// `push(Constraint::Eq(a, b))` once constraints get deferred.
    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.resolve_shallow(a);
        let b = self.resolve_shallow(b);
        match (a, b) {
            (Ty::Infer(v1), Ty::Infer(v2)) => {
                self.table.union(v1, v2);
                true
            }
            (Ty::Infer(var), ty) | (ty, Ty::Infer(var)) => {
                if occurs(&mut self.table, var, &ty) {
                    return false;
                }
                self.table.union_value(var, TyVarValue::Known(ty));
                true
            }
            // Errors are infectious and silent.
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Unit, Ty::Unit)
            | (Ty::Never, Ty::Never)
            | (Ty::Int, Ty::Int)
            | (Ty::Str, Ty::Str) => true,
            (Ty::Fn(f1), Ty::Fn(f2)) => {
                f1.params.len() == f2.params.len() && {
                    let params_ok = f1
                        .params
                        .iter()
                        .zip(&f2.params)
                        .all(|(p1, p2)| self.unify(p1, p2));
                    params_ok && self.unify(&f1.ret, &f2.ret)
                }
            }
            _ => false,
        }
    }

    fn resolve_shallow(&mut self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        while let Ty::Infer(var) = ty {
            match self.table.probe_value(var) {
                TyVarValue::Known(known) => ty = known,
                TyVarValue::Unknown => break,
            }
        }
        ty
    }
}

fn builtin_type(builtin: Builtin) -> Ty {
    match builtin {
        Builtin::Print => Ty::fn_type(vec![Ty::Str], Ty::Unit),
        Builtin::Panic => Ty::fn_type(vec![Ty::Str], Ty::Never),
    }
}

fn occurs(table: &mut InPlaceUnificationTable<TyVar>, var: TyVar, ty: &Ty) -> bool {
    match ty {
        Ty::Infer(other) => {
            if table.unioned(var, *other) {
                return true;
            }
            match table.probe_value(*other) {
                TyVarValue::Known(known) => occurs(table, var, &known),
                TyVarValue::Unknown => false,
            }
        }
        Ty::Fn(f) => {
            f.params.iter().any(|p| occurs(table, var, p)) || occurs(table, var, &f.ret)
        }
        _ => false,
    }
}

fn resolve_fully(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(var) => match table.probe_value(*var) {
            TyVarValue::Known(known) => resolve_fully(table, &known),
            // Canonicalize so equal results stay equal across runs.
            TyVarValue::Unknown => Ty::Infer(table.find(*var)),
        },
        Ty::Fn(f) => {
            let params = f.params.iter().map(|p| resolve_fully(table, p)).collect();
            let ret = resolve_fully(table, &f.ret);
            Ty::fn_type(params, ret)
        }
        other => other.clone(),
    }
}
