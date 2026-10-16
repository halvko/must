//! Which locals of enclosing functions each `fn` literal reaches — its
//! *outer places* (#175).
//!
//! A closure holds nothing between calls: every time it runs, it is handed
//! a borrow of each outer place it mentions, `.&mut` when it writes the
//! place (or borrows it mutably) and `.&` otherwise. This module answers
//! the syntactic half of that — WHICH bindings, and whether any mention
//! writes — once per item, so inference (which gives a capturing literal
//! a region) and MIR lowering (which passes the borrows) agree on the set.
//!
//! A mention reaches through every literal between it and the binding's
//! own: if an inner literal mentions a place two levels out, the middle
//! literal mentions it too, because the inner one's borrow is taken from
//! the middle one's.
//!
//! A `const { ... }` block is a body of its own and bounds this: a mention
//! from inside one of a runtime local outside it is not a capture (it is
//! refused elsewhere), so it adds nothing here.

use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::body::{Body, ExprData, GenericArgData, Stmt};
use crate::scopes::{Resolution, resolutions};
use crate::{BindingId, ExprId, ItemId};
use base_db::Db;

/// One outer place a literal reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capture {
    pub binding: BindingId,
    /// Whether some mention writes the place or borrows it mutably — the
    /// borrow the closure is handed is then `.&mut`.
    pub mutable: bool,
}

/// Every capturing literal of one item, with its captures in the order
/// they are first mentioned. A literal that reaches nothing is absent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Captures {
    by_literal: FxHashMap<ExprId, Vec<Capture>>,
}

impl Captures {
    /// The outer places `literal` reaches; empty for a capture-free one.
    pub fn of(&self, literal: ExprId) -> &[Capture] {
        self.by_literal
            .get(&literal)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Every capturing literal with its captures, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (ExprId, &[Capture])> {
        self.by_literal
            .iter()
            .map(|(&literal, captures)| (literal, captures.as_slice()))
    }

    /// Whether `literal` reaches any outer place.
    pub fn captures(&self, literal: ExprId) -> bool {
        self.by_literal.contains_key(&literal)
    }
}

#[salsa::tracked(returns(ref))]
pub fn closure_captures<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Captures {
    compute(crate::body::body(db, item), resolutions(db, item))
}

/// [`closure_captures`] off a body and its name resolutions — for
/// inference, which holds both already.
pub(crate) fn compute(body: &Body, resolutions: &ArenaMap<ExprId, Resolution>) -> Captures {
    let mut walk = Walk {
        body,
        resolutions,
        owner: ArenaMap::default(),
        stack: Vec::new(),
        out: Captures::default(),
    };
    if let Some(root) = body.root {
        walk.expr(root, false);
    }
    walk.out
}

/// A body boundary a mention can sit inside.
#[derive(Clone, Copy)]
enum Frame {
    Literal(ExprId),
    Const,
}

struct Walk<'a> {
    body: &'a Body,
    resolutions: &'a ArenaMap<ExprId, Resolution>,
    /// Each binding's declaring body: the index of its frame in `stack`
    /// when it was declared, `None` at the item's top level.
    owner: ArenaMap<BindingId, Option<usize>>,
    stack: Vec<Frame>,
    out: Captures,
}

impl Walk<'_> {
    fn declare(&mut self, pat: crate::PatId) {
        let depth = self.stack.len().checked_sub(1);
        for binding in self.body.pat_bindings(pat) {
            self.owner.insert(binding, depth);
        }
    }

    fn mention(&mut self, binding: BindingId, mutable: bool) {
        let Some(&owner) = self.owner.get(binding) else {
            return;
        };
        let inner = owner.map_or(0, |depth| depth + 1);
        let crossed = &self.stack[inner.min(self.stack.len())..];
        if crossed.iter().any(|frame| matches!(frame, Frame::Const)) {
            return;
        }
        for frame in crossed {
            let Frame::Literal(literal) = *frame else {
                continue;
            };
            let captures = self.out.by_literal.entry(literal).or_default();
            match captures.iter_mut().find(|c| c.binding == binding) {
                Some(capture) => capture.mutable |= mutable,
                None => captures.push(Capture { binding, mutable }),
            }
        }
    }

    /// Walk `expr`. `mutating` is set while walking the root of a place
    /// that is written or borrowed mutably: the name at its root is a
    /// mutating mention.
    fn expr(&mut self, expr: ExprId, mutating: bool) {
        let body = self.body;
        match &body.exprs[expr] {
            ExprData::NameRef(_) => {
                if let Some(Resolution::Local(binding)) = self.resolutions.get(expr) {
                    self.mention(*binding, mutating);
                }
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { pat, init, .. } => {
                            if let Some(init) = init {
                                self.expr(*init, false);
                            }
                            self.declare(*pat);
                        }
                        Stmt::Assign { target, value } => {
                            self.expr(*target, true);
                            self.expr(*value, false);
                        }
                        Stmt::Expr(e) => self.expr(*e, false),
                    }
                }
                if let Some(tail) = tail {
                    self.expr(*tail, false);
                }
            }
            ExprData::ConstBlock { body: inner } => {
                self.stack.push(Frame::Const);
                self.expr(*inner, false);
                self.stack.pop();
            }
            ExprData::FnLiteral {
                params,
                body: inner,
                ..
            } => {
                self.stack.push(Frame::Literal(expr));
                for param in params {
                    self.declare(param.pat);
                }
                self.expr(*inner, false);
                self.stack.pop();
            }
            ExprData::Call { callee, args, .. } => {
                self.expr(*callee, false);
                for &arg in args {
                    self.expr(arg, false);
                }
            }
            ExprData::Bin { lhs, rhs, .. } => {
                self.expr(*lhs, false);
                self.expr(*rhs, false);
            }
            ExprData::Neg { operand } => self.expr(*operand, false),
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(*condition, false);
                self.expr(*then_branch, false);
                if let Some(else_branch) = else_branch {
                    self.expr(*else_branch, false);
                }
            }
            ExprData::RecordLit { fields } => {
                for field in fields {
                    self.expr(field.value, false);
                }
            }
            // A step of a place: writing `p.f` writes through `p`.
            ExprData::Field { receiver, .. } => self.expr(*receiver, mutating),
            ExprData::Index { base, index } => {
                self.expr(*base, mutating);
                self.expr(*index, false);
            }
            ExprData::Deref { receiver } => self.expr(*receiver, mutating),
            ExprData::ArrayLit { elements } => {
                for &element in elements {
                    self.expr(element, false);
                }
            }
            ExprData::ArrayRepeat { element, count } => {
                self.expr(*element, false);
                self.expr(*count, false);
            }
            ExprData::AddrOf { mutable, place } | ExprData::Borrow { mutable, place } => {
                self.expr(*place, *mutable);
            }
            ExprData::Unsafe { body: inner } | ExprData::Loop { body: inner } => {
                self.expr(*inner, false);
            }
            ExprData::VariantPath {
                base,
                args,
                member_args,
                ..
            } => {
                self.expr(*base, false);
                for arg in args.iter().flatten().chain(member_args.iter().flatten()) {
                    if let GenericArgData::Const(value) = arg {
                        self.expr(*value, false);
                    }
                }
            }
            ExprData::GenericApp { base, args } => {
                self.expr(*base, false);
                for arg in args {
                    if let GenericArgData::Const(value) = arg {
                        self.expr(*value, false);
                    }
                }
            }
            ExprData::Match { scrutinee, arms } => {
                self.expr(*scrutinee, false);
                for arm in arms {
                    self.declare(arm.pat);
                    self.expr(arm.body, false);
                }
            }
            ExprData::Break { value } | ExprData::Return { value } => {
                if let Some(value) = value {
                    self.expr(*value, false);
                }
            }
            ExprData::Missing
            | ExprData::Literal(_)
            | ExprData::ExternItem
            | ExprData::ElidedVariant { .. }
            | ExprData::Continue => {}
        }
    }
}
