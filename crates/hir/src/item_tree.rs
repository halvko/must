//! The item tree: a range-free summary of a file's top-level items.
//!
//! Because it carries no text positions, this value compares equal across
//! edits that only touch bodies or whitespace — salsa then backdates it and
//! nothing downstream of name-level information re-runs.

use base_db::{Db, SourceFile, parse};
use syntax::ast;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ItemTree {
    pub items: Vec<ItemData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemData {
    /// Empty string when the name is missing (broken code); the
    /// disambiguator in [`crate::ItemId`] still keeps identity stable.
    pub name: String,
    pub kind: ItemKind,
    /// The item's type contract: a written annotation, or one synthesized
    /// from a self-sufficient fn-literal body. Nothing records which kind
    /// it was — rules that care (e.g. "exported items require *written*
    /// contracts") must not read the synthesized kind as written.
    pub type_ref: Option<TypeRef>,
}

/// What sort of item this is. Not an extension of [`Constness`]: a `type`
/// item is not a third flavor of value item — it declares no runtime value
/// at all — so the value-only fact (`static` vs `const`) stays its own enum
/// and everything that asks "which constness?" is forced to first decide
/// what a *type* item means for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// `static`/`const` — an item with a value.
    Value(Constness),
    /// `type Name = struct { ... };` — a newtype declaration. Types have no
    /// runtime location (not `static`) and are not copied values (`const`
    /// is reserved for future type *aliases*).
    Type,
}

impl ItemKind {
    /// The `static`/`const` distinction, when the item has one.
    pub fn constness(self) -> Option<Constness> {
        match self {
            ItemKind::Value(constness) => Some(constness),
            ItemKind::Type => None,
        }
    }
}

/// Whether an item is a `static` or a `const` — a direct mapping of the
/// item's keyword. Const-ness of a *function* is a property of the fn
/// literal (the upcoming explicit `const fn` marker) and will be a separate
/// item-tree fact, not encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constness {
    Static,
    Const,
}

/// Syntax-free representation of a type annotation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Unit,
    Never,
    Fn {
        params: Vec<TypeRef>,
        ret: Option<Box<TypeRef>>,
    },
    Ref(Box<TypeRef>),
    Path(String),
    Hole,
    /// `{ x: T, y: U }` — a structural record type. Fields are sorted by
    /// name (field order is irrelevant to the type, so the canonical order
    /// makes equal types compare equal).
    Record(Vec<(String, TypeRef)>),
    Error,
}

impl TypeRef {
    pub fn from_ast(ty: ast::Type) -> TypeRef {
        match ty {
            ast::Type::UnitType(_) => TypeRef::Unit,
            ast::Type::NeverType(_) => TypeRef::Never,
            ast::Type::FnType(it) => TypeRef::Fn {
                params: it.param_types().map(TypeRef::from_ast).collect(),
                ret: it
                    .ret_type()
                    .and_then(|rt| rt.ty())
                    .map(|t| Box::new(TypeRef::from_ast(t))),
            },
            ast::Type::RefType(it) => match it.ty() {
                Some(inner) => TypeRef::Ref(Box::new(TypeRef::from_ast(inner))),
                None => TypeRef::Error,
            },
            ast::Type::PathType(it) => match it.name_ref() {
                Some(name) => TypeRef::Path(name.text()),
                None => TypeRef::Error,
            },
            ast::Type::HoleType(_) => TypeRef::Hole,
            ast::Type::RecordType(it) => {
                let mut fields: Vec<(String, TypeRef)> = it
                    .fields()
                    .filter_map(|field| {
                        // A field without a name is broken source (the parse
                        // error covers it); nothing meaningful to keep.
                        let name = field.name()?.text();
                        let ty = field.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error);
                        Some((name, ty))
                    })
                    .collect();
                // Canonicalize: sorted by name, so field order in source is
                // irrelevant to the type. Duplicate fields keep the FIRST
                // occurrence (the stable sort preserves source order among
                // equals): validation already errors on the duplicate, this
                // is recovery, not semantics.
                fields.sort_by(|(a, _), (b, _)| a.cmp(b));
                fields.dedup_by(|second, first| second.0 == first.0);
                TypeRef::Record(fields)
            }
        }
    }

    pub fn from_opt_ast(ty: Option<ast::Type>) -> Option<TypeRef> {
        ty.map(TypeRef::from_ast)
    }
}

#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    let tree = parse(db, file).tree();
    let items = tree
        .items()
        .map(|item| match &item {
            ast::Item::StaticItem(it) => ItemData {
                name: item.name().map(|n| n.text()).unwrap_or_default(),
                kind: ItemKind::Value(if it.is_const() {
                    Constness::Const
                } else {
                    Constness::Static
                }),
                type_ref: TypeRef::from_opt_ast(it.ty())
                    .or_else(|| type_ref_from_fn_literal(it.body())),
            },
            ast::Item::TypeItem(_) => ItemData {
                name: item.name().map(|n| n.text()).unwrap_or_default(),
                // The declared shape lives in [`type_decl`], not here: the
                // item tree only answers name-level questions, and keeping
                // the shape out means editing a declaration's fields never
                // churns the whole file's name-level facts.
                kind: ItemKind::Type,
                type_ref: None,
            },
        })
        .collect();
    ItemTree { items }
}

/// The declared shape of a `type` item: its `struct` literal's fields read
/// *syntactically* as types — one `TypeRef` per field, sorted by name,
/// range-free. This is the incrementality firewall for named types: users of
/// a `type Foo` depend on this value (and never on const eval), so an edit
/// elsewhere in the declaring file backdates here and stops.
///
/// `None` when the RHS is not a `struct` literal (diagnosed in
/// [`crate::file_diagnostics`]) or the item is not a `type` item at all.
/// When `enum` literals land, they become the second accepted RHS here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDeclData {
    /// Sorted by field name (same canonicalization as [`TypeRef::Record`]).
    pub fields: Vec<(String, TypeRef)>,
}

#[salsa::tracked(returns(ref))]
pub fn type_decl<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Option<TypeDeclData> {
    let ast::Item::TypeItem(decl) = item_source(db, item)? else {
        return None;
    };
    let ast::Expr::RecordExpr(record) = decl.body()? else {
        return None;
    };
    Some(TypeDeclData {
        fields: record_expr_fields_as_types(&record),
    })
}

/// The fields of a `struct` literal used as a type declaration, each value
/// expression reinterpreted as a type. Sorted + deduplicated like
/// [`TypeRef::from_ast`] does for record *types*.
fn record_expr_fields_as_types(record: &ast::RecordExpr) -> Vec<(String, TypeRef)> {
    let mut fields: Vec<(String, TypeRef)> = record
        .fields()
        .filter_map(|field| {
            // A field without a name is broken source (the parse error
            // covers it); nothing meaningful to keep.
            let name = field.name_ref()?.text();
            let ty = field
                .expr()
                .map(expr_as_type_ref)
                // Shorthand (`x` without `: type`): no type to read.
                .unwrap_or(TypeRef::Error);
            Some((name, ty))
        })
        .collect();
    fields.sort_by(|(a, _), (b, _)| a.cmp(b));
    fields.dedup_by(|second, first| second.0 == first.0);
    fields
}

/// A type-declaration field's value expression, read as a type: a name is a
/// type name, a nested `struct` literal is a nested record. Anything else
/// (a computed expression) is not a type — [`TypeRef::Error`];
/// [`crate::file_diagnostics`] reports it.
fn expr_as_type_ref(expr: ast::Expr) -> TypeRef {
    match expr {
        ast::Expr::PathExpr(it) => match it.name_ref() {
            Some(name) => TypeRef::Path(name.text()),
            None => TypeRef::Error,
        },
        ast::Expr::RecordExpr(it) => TypeRef::Record(record_expr_fields_as_types(&it)),
        _ => TypeRef::Error,
    }
}

/// If `body` is a fn literal with all params typed and an explicit return type,
/// synthesize a `TypeRef::Fn` so the item acts as a hard inference firewall
/// without requiring a redundant item-level annotation.
fn type_ref_from_fn_literal(body: Option<ast::Expr>) -> Option<TypeRef> {
    let ast::Expr::FnLiteral(fn_lit) = body? else {
        return None;
    };
    let params: Vec<TypeRef> = fn_lit
        .param_list()?
        .params()
        .map(|p| Some(TypeRef::from_ast(p.ty()?)))
        .collect::<Option<_>>()?;
    let ret = fn_lit
        .ret_type()
        .and_then(|rt| rt.ty())
        .map(|t| Box::new(TypeRef::from_ast(t)));
    let type_ref = TypeRef::Fn { params, ret };
    crate::ty::is_fully_typed(&type_ref).then_some(type_ref)
}

/// The syntax node for `item`, looked up by (name, disambiguator). Range
/// information enters here and only here; callers that want to stay in the
/// firewall must not feed this back into range-free queries.
pub fn item_source(db: &dyn Db, item: crate::ItemId<'_>) -> Option<ast::Item> {
    let file = item.file(db);
    let tree = parse(db, file).tree();
    let name = item.name(db);
    let mut seen = 0;
    for ast_item in tree.items() {
        let item_name = ast_item.name().map(|n| n.text()).unwrap_or_default();
        if item_name == *name {
            if seen == item.disambiguator(db) {
                return Some(ast_item);
            }
            seen += 1;
        }
    }
    None
}
