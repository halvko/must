//! Body lowering: AST → arena-based HIR.
//!
//! [`Body`] is deliberately range-free so it compares equal when an edit only
//! moves things around — that equality is what stops salsa from re-running
//! inference of untouched items. All positions live in [`BodySourceMap`],
//! which only the final, range-producing layers (diagnostics, IDE) read.

use base_db::{Db, SourceFile};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;
use syntax::SyntaxNodePtr;
use syntax::ast::{self, AstNode as _};

use crate::ItemId;
use crate::item_tree::{TypeRef, item_source};

pub type ExprId = Idx<ExprData>;
pub type BindingId = Idx<BindingData>;

pub use syntax::ast::BinOp;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body {
    pub exprs: Arena<ExprData>,
    pub bindings: Arena<BindingData>,
    /// The item's initializer expression (usually a `fn` literal).
    pub root: Option<ExprId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingData {
    pub name: String,
    pub type_ref: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprData {
    /// Source was broken; nothing to lower.
    Missing,
    Literal(LiteralData),
    NameRef(String),
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    Bin {
        op: Option<BinOp>,
        lhs: ExprId,
        rhs: ExprId,
    },
    If {
        condition: ExprId,
        then_branch: ExprId,
        /// `None` for `if` without `else`; `else if` chains nest here.
        else_branch: Option<ExprId>,
    },
    Block {
        stmts: Vec<Stmt>,
        tail: Option<ExprId>,
    },
    FnLiteral {
        params: Vec<BindingId>,
        ret_type: Option<TypeRef>,
        body: ExprId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralData {
    /// `None` if the literal doesn't fit in u128.
    Int(Option<u128>),
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let { binding: BindingId, init: ExprId },
    Expr(ExprId),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodySourceMap {
    expr_map: FxHashMap<SyntaxNodePtr, ExprId>,
    expr_map_back: ArenaMap<ExprId, SyntaxNodePtr>,
    binding_map: FxHashMap<SyntaxNodePtr, BindingId>,
    binding_map_back: ArenaMap<BindingId, SyntaxNodePtr>,
}

impl BodySourceMap {
    pub fn expr_for_node(&self, ptr: SyntaxNodePtr) -> Option<ExprId> {
        self.expr_map.get(&ptr).copied()
    }
    pub fn node_for_expr(&self, expr: ExprId) -> Option<SyntaxNodePtr> {
        self.expr_map_back.get(expr).copied()
    }
    pub fn binding_for_node(&self, ptr: SyntaxNodePtr) -> Option<BindingId> {
        self.binding_map.get(&ptr).copied()
    }
    pub fn node_for_binding(&self, binding: BindingId) -> Option<SyntaxNodePtr> {
        self.binding_map_back.get(binding).copied()
    }
}

#[salsa::tracked(returns(ref))]
pub fn body_with_source_map<'db>(db: &'db dyn Db, item: ItemId<'db>) -> (Body, BodySourceMap) {
    let mut ctx = LowerCtx::default();
    let root = item_source(db, item)
        .and_then(|it| it.body())
        .map(|expr| ctx.lower_expr(expr));
    (
        Body {
            exprs: ctx.exprs,
            bindings: ctx.bindings,
            root,
        },
        ctx.source_map,
    )
}

/// Range-free projection of [`body_with_source_map`]. Edits that only move
/// code produce an equal `Body`, so consumers of this query backdate.
#[salsa::tracked(returns(ref))]
pub fn body<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Body {
    body_with_source_map(db, item).0.clone()
}

/// The file a body's positions live in (trivially the item's file, but going
/// through this keeps signatures honest once multiple files exist).
pub fn body_file(db: &dyn Db, item: ItemId<'_>) -> SourceFile {
    item.file(db)
}

#[derive(Default)]
struct LowerCtx {
    exprs: Arena<ExprData>,
    bindings: Arena<BindingData>,
    source_map: BodySourceMap,
}

impl LowerCtx {
    fn alloc_expr(&mut self, data: ExprData, node: &syntax::SyntaxNode) -> ExprId {
        let id = self.exprs.alloc(data);
        let ptr = SyntaxNodePtr::new(node);
        self.source_map.expr_map.insert(ptr, id);
        self.source_map.expr_map_back.insert(id, ptr);
        id
    }

    fn missing_expr(&mut self) -> ExprId {
        self.exprs.alloc(ExprData::Missing)
    }

    fn lower_opt_expr(&mut self, expr: Option<ast::Expr>) -> ExprId {
        match expr {
            Some(expr) => self.lower_expr(expr),
            None => self.missing_expr(),
        }
    }

    fn lower_expr(&mut self, expr: ast::Expr) -> ExprId {
        match expr {
            ast::Expr::Literal(it) => {
                let data = match it.kind() {
                    Some(ast::LiteralKind::Int(token)) => {
                        let text = token.text().replace('_', "");
                        LiteralData::Int(text.parse().ok())
                    }
                    Some(ast::LiteralKind::Str(token)) => {
                        LiteralData::Str(unescape(token.text()))
                    }
                    Some(ast::LiteralKind::Bool(value)) => LiteralData::Bool(value),
                    None => return self.missing_expr(),
                };
                self.alloc_expr(ExprData::Literal(data), it.syntax())
            }
            ast::Expr::PathExpr(it) => {
                let Some(name_ref) = it.name_ref() else {
                    return self.missing_expr();
                };
                self.alloc_expr(ExprData::NameRef(name_ref.text()), it.syntax())
            }
            ast::Expr::CallExpr(it) => {
                let callee = self.lower_opt_expr(it.callee());
                let args = it
                    .arg_list()
                    .map(|args| args.args().map(|a| self.lower_expr(a)).collect())
                    .unwrap_or_default();
                self.alloc_expr(ExprData::Call { callee, args }, it.syntax())
            }
            ast::Expr::BinExpr(it) => {
                let lhs = self.lower_opt_expr(it.lhs());
                let rhs = self.lower_opt_expr(it.rhs());
                let op = it.op();
                self.alloc_expr(ExprData::Bin { op, lhs, rhs }, it.syntax())
            }
            ast::Expr::ParenExpr(it) => {
                // No HIR node for parens, but let position lookups through
                // the paren node land on the inner expression.
                let inner = self.lower_opt_expr(it.expr());
                self.source_map
                    .expr_map
                    .insert(SyntaxNodePtr::new(it.syntax()), inner);
                inner
            }
            ast::Expr::IfExpr(it) => {
                let condition = self.lower_opt_expr(it.condition());
                let then_branch = self.lower_opt_expr(it.then_branch());
                let else_branch = it.else_branch().map(|e| self.lower_expr(e));
                self.alloc_expr(
                    ExprData::If {
                        condition,
                        then_branch,
                        else_branch,
                    },
                    it.syntax(),
                )
            }
            ast::Expr::BlockExpr(it) => self.lower_block(it),
            ast::Expr::FnLiteral(it) => {
                let params = it
                    .param_list()
                    .map(|list| {
                        list.params()
                            .map(|param| {
                                let type_ref = TypeRef::from_opt_ast(param.ty());
                                self.alloc_binding(param.name(), type_ref, param.syntax())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let ret_type = it
                    .ret_type()
                    .map(|rt| rt.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error));
                let body = self.lower_opt_expr(it.body());
                self.alloc_expr(
                    ExprData::FnLiteral {
                        params,
                        ret_type,
                        body,
                    },
                    it.syntax(),
                )
            }
        }
    }

    fn lower_block(&mut self, block: ast::BlockExpr) -> ExprId {
        let stmts = block
            .statements()
            .map(|stmt| match stmt {
                ast::Stmt::LetStmt(it) => {
                    let init = self.lower_opt_expr(it.initializer());
                    let type_ref = TypeRef::from_opt_ast(it.ty());
                    let binding = self.alloc_binding(it.name(), type_ref, it.syntax());
                    Stmt::Let { binding, init }
                }
                ast::Stmt::ExprStmt(it) => Stmt::Expr(self.lower_opt_expr(it.expr())),
            })
            .collect();
        let tail = block.tail_expr().map(|e| self.lower_expr(e));
        self.alloc_expr(ExprData::Block { stmts, tail }, block.syntax())
    }

    fn alloc_binding(
        &mut self,
        name: Option<ast::Name>,
        type_ref: Option<TypeRef>,
        fallback_node: &syntax::SyntaxNode,
    ) -> BindingId {
        let id = self.bindings.alloc(BindingData {
            name: name.as_ref().map(|n| n.text()).unwrap_or_default(),
            type_ref,
        });
        let ptr = match &name {
            Some(name) => SyntaxNodePtr::new(name.syntax()),
            None => SyntaxNodePtr::new(fallback_node),
        };
        self.source_map.binding_map.insert(ptr, id);
        self.source_map.binding_map_back.insert(id, ptr);
        id
    }
}

fn unescape(raw: &str) -> String {
    // Tolerates a missing closing quote (unterminated string literals).
    let inner = raw.strip_prefix('"').unwrap_or(raw);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
