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
    /// The generic binder of the item's initializer literal
    /// (`fn::<T, const N: usize>`), read syntactically and range-free —
    /// same firewall discipline as [`type_decl`]. Empty for non-generic
    /// items. For a generic item, [`Self::type_ref`] is *always* the
    /// signature synthesized from the literal's own annotations (TR06: the
    /// binder's fully-annotated signature IS the item's contract; an
    /// explicit item-level annotation cannot name the binder's params and
    /// is ignored here), or `None` when the fully-annotated rule is
    /// violated.
    pub generics: Vec<GenericParamData>,
}

/// One generic parameter of an item's binder, in declaration order — the
/// position in this list is the parameter's identity (see
/// [`crate::ty::ParamTy::index`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParamData {
    /// Empty when the name is missing (broken code).
    pub name: String,
    pub kind: GenericParamKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericParamKind {
    /// `T` — a rigid type parameter.
    Type,
    /// `const N: usize` — a const parameter; carries its declared type.
    /// Dependent declared types (`const N: T`) are rejected (TR06) — the
    /// `TypeRef` stays syntactic here and lowers without the binder's
    /// param scope, so a type-param mention lowers to `{error}` with the
    /// matching diagnostic in [`crate::file_diagnostics`].
    Const(TypeRef),
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
    /// `Pair::<usize, 8>` — a generic type mention with its turbofish. The
    /// name stays syntactic (like [`TypeRef::Path`]); arity, kinds and
    /// resolution are judged in lowering against the target's binder.
    Apply {
        name: String,
        args: Vec<GenericArgRef>,
    },
    /// `Shape::Circle` — a variant type, named through its enum. The names
    /// stay syntactic here (like [`TypeRef::Path`]); resolution happens in
    /// lowering, against the enum's [`type_decl`].
    Variant {
        enum_name: String,
        variant: String,
    },
    Hole,
    /// `{ x: T, y: U }` — a structural record type. Fields are sorted by
    /// name (field order is irrelevant to the type, so the canonical order
    /// makes equal types compare equal).
    Record(Vec<(String, TypeRef)>),
    Error,
}

/// One generic argument of a [`TypeRef::Apply`], syntactic and range-free.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArgRef {
    Type(TypeRef),
    Const(ConstArgRef),
}

/// A const argument in *annotation* position, read straight off the syntax
/// — the annotation firewall: const eval never runs on this path, so only
/// literal-shaped values and (const-param) names are representable. A
/// `const { ... }` block keeps its shape as [`ConstArgRef::Block`] so the
/// diagnostics pass can reject it cleanly; it never lowers to a value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstArgRef {
    Int(u128),
    Str(String),
    Bool(bool),
    /// `const N` — a name forced to a value reading; resolved against the
    /// enclosing binder's const params during lowering.
    Name(String),
    /// `const { ... }` — rejected in annotation position (the eval-free
    /// path); expression-position turbofish on *functions* still accepts
    /// blocks through the ordinary const-block machinery.
    Block,
    /// Broken source (a parse error covers it) or an overflowing literal
    /// (the diagnostics pass reports it).
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
            ast::Type::PathType(it) => match it.generic_arg_list() {
                Some(list) => match it.name_ref() {
                    Some(name) => TypeRef::Apply {
                        name: name.text(),
                        args: generic_args_from_ast(&list),
                    },
                    None => TypeRef::Error,
                },
                None => match (it.name_ref(), it.variant_name_ref()) {
                    (Some(enum_name), Some(variant)) => TypeRef::Variant {
                        enum_name: enum_name.text(),
                        variant: variant.text(),
                    },
                    (Some(name), None) => TypeRef::Path(name.text()),
                    (None, _) => TypeRef::Error,
                },
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
    pub fn is_fully_typed(&self) -> bool {
        match self {
            TypeRef::Hole => false,
            TypeRef::Unit
            | TypeRef::Never
            | TypeRef::Path(_)
            | TypeRef::Variant { .. }
            | TypeRef::Error => true,
            TypeRef::Fn { params, ret } => {
                params.iter().all(TypeRef::is_fully_typed)
                    && ret.as_ref().is_some_and(|r| r.is_fully_typed())
            }
            TypeRef::Ref(inner) => inner.is_fully_typed(),
            TypeRef::Apply { args, .. } => args.iter().all(|arg| match arg {
                GenericArgRef::Type(ty) => ty.is_fully_typed(),
                // Const args are never inferred (TR06) — a written one is
                // always "fully typed"; whether it is *legal* is a
                // different question (the diagnostics pass).
                GenericArgRef::Const(_) => true,
            }),
            TypeRef::Record(fields) => fields.iter().all(|(_, ty)| ty.is_fully_typed()),
        }
    }

    /// Whether an fn type is written anywhere inside this reference —
    /// syntactic, like everything else on `TypeRef`. Used to keep fn
    /// values out of the const-arg domain at the *declaration*
    /// (TR06: concrete data types only — a const param's type may not
    /// mention one — see
    /// [`crate::diag::FN_CONST_ARG`]). A `Path` naming a `type` item whose
    /// underlying record smuggles an fn field is not seen here; the
    /// mention-side belt in inference shares the same (nominal-opaque)
    /// blind spot — acceptable while nominal types are identity-only.
    pub fn mentions_fn(&self) -> bool {
        match self {
            TypeRef::Fn { .. } => true,
            TypeRef::Ref(inner) => inner.mentions_fn(),
            TypeRef::Record(fields) => fields.iter().any(|(_, ty)| ty.mentions_fn()),
            TypeRef::Apply { args, .. } => args.iter().any(|arg| match arg {
                GenericArgRef::Type(ty) => ty.mentions_fn(),
                GenericArgRef::Const(_) => false,
            }),
            TypeRef::Unit
            | TypeRef::Never
            | TypeRef::Path(_)
            | TypeRef::Variant { .. }
            | TypeRef::Hole
            | TypeRef::Error => false,
        }
    }
}

/// Read a turbofish in *annotation* position, range-free. Const arguments
/// keep only their literal shape ([`ConstArgRef`]) — the firewall: nothing
/// here can require evaluation.
pub(crate) fn generic_args_from_ast(list: &ast::GenericArgList) -> Vec<GenericArgRef> {
    list.args()
        .map(|arg| match arg {
            ast::GenericArg::TypeArg(ty_arg) => {
                GenericArgRef::Type(ty_arg.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error))
            }
            ast::GenericArg::ConstArg(const_arg) => {
                GenericArgRef::Const(const_arg_ref_from_ast(&const_arg))
            }
        })
        .collect()
}

pub(crate) fn const_arg_ref_from_ast(arg: &ast::ConstArg) -> ConstArgRef {
    match arg.expr() {
        Some(ast::Expr::Literal(lit)) => match lit.kind() {
            Some(ast::LiteralKind::Int(token)) => {
                match token.text().replace('_', "").parse() {
                    Ok(value) => ConstArgRef::Int(value),
                    // Overflow: the diagnostics pass reports it (same text
                    // as a body literal's overflow).
                    Err(_) => ConstArgRef::Error,
                }
            }
            Some(ast::LiteralKind::Str(token)) => {
                ConstArgRef::Str(crate::body::unescape(token.text()))
            }
            Some(ast::LiteralKind::Bool(value)) => ConstArgRef::Bool(value),
            None => ConstArgRef::Error,
        },
        Some(ast::Expr::PathExpr(path)) => match path.name_ref() {
            Some(name) => ConstArgRef::Name(name.text()),
            None => ConstArgRef::Error,
        },
        Some(ast::Expr::ConstBlockExpr(_)) => ConstArgRef::Block,
        _ => ConstArgRef::Error,
    }
}

#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    let tree = parse(db, file).tree();
    let items = tree
        .items()
        .map(|item| match &item {
            ast::Item::StaticItem(it) => {
                let generics = generics_from_fn_literal(it.body());
                let type_ref = if generics.is_empty() {
                    TypeRef::from_opt_ast(it.ty()).or_else(|| type_ref_from_fn_literal(it.body()))
                } else {
                    // A generic item's contract is the binder signature —
                    // its literal's own (mandatory, TR06) annotations, which
                    // are the only place the type params are in scope. An
                    // explicit item-level `: Type` annotation cannot
                    // mention them and is not consulted.
                    type_ref_from_fn_literal(it.body())
                };
                ItemData {
                    name: item.name().map(|n| n.text()).unwrap_or_default(),
                    kind: ItemKind::Value(if it.is_const() {
                        Constness::Const
                    } else {
                        Constness::Static
                    }),
                    type_ref,
                    generics,
                }
            }
            ast::Item::TypeItem(it) => ItemData {
                name: item.name().map(|n| n.text()).unwrap_or_default(),
                // The declared shape lives in [`type_decl`], not here: the
                // item tree only answers name-level questions, and keeping
                // the shape out means editing a declaration's fields never
                // churns the whole file's name-level facts. The generic
                // BINDER is a name-level fact though (arity and kinds gate
                // every mention), so it lives here like a fn item's.
                kind: ItemKind::Type,
                type_ref: None,
                generics: generics_from_type_literal(it.body()),
            },
        })
        .collect();
    ItemTree { items }
}

/// The declared shape of a `type` item, read *syntactically* — `TypeRef`s,
/// range-free. This is the incrementality firewall for named types: users of
/// a `type Foo` depend on this value (and never on const eval), so an edit
/// elsewhere in the declaring file backdates here and stops.
///
/// `None` when the RHS is neither a `struct` nor an `enum` literal
/// (diagnosed in [`crate::file_diagnostics`]) or the item is not a `type`
/// item at all.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeDeclData {
    /// `type Foo = struct { ... };` — a newtype over a record shape.
    Struct {
        /// Sorted by field name (same canonicalization as
        /// [`TypeRef::Record`]).
        fields: Vec<(String, TypeRef)>,
    },
    /// `type Shape = enum { ... };` — an enum with positional-payload
    /// variants.
    Enum {
        /// In *source* order — a variant's index is its identity at the
        /// type and value level, so no canonicalization happens here.
        /// Duplicate names stay (validation errors on them); lookups by
        /// name find the first.
        variants: Vec<(String, Vec<TypeRef>)>,
    },
}

#[salsa::tracked(returns(ref))]
pub fn type_decl<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Option<TypeDeclData> {
    let ast::Item::TypeItem(decl) = item_source(db, item)? else {
        return None;
    };
    match decl.body()? {
        ast::Expr::RecordExpr(record) => Some(TypeDeclData::Struct {
            fields: record_expr_fields_as_types(&record),
        }),
        ast::Expr::EnumExpr(en) => Some(TypeDeclData::Enum {
            variants: en
                .variants()
                .filter_map(|variant| {
                    // A variant without a name is broken source (the parse
                    // error covers it); nothing meaningful to keep.
                    let name = variant.name()?.text();
                    let payload = variant.payload_types().map(TypeRef::from_ast).collect();
                    Some((name, payload))
                })
                .collect(),
        }),
        _ => None,
    }
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
pub(crate) fn expr_as_type_ref(expr: ast::Expr) -> TypeRef {
    match expr {
        ast::Expr::PathExpr(it) => match it.generic_arg_list() {
            Some(list) => match (it.name_ref(), it.variant_name_ref()) {
                // `List::<T>` as a field's type — a generic type mention.
                (Some(name), None) => TypeRef::Apply {
                    name: name.text(),
                    args: generic_args_from_ast(&list),
                },
                // `Option::<T>::Some` — variant types of generic enums have
                // no annotation spelling yet.
                _ => TypeRef::Error,
            },
            None => match (it.name_ref(), it.variant_name_ref()) {
                (Some(enum_name), Some(variant)) => TypeRef::Variant {
                    enum_name: enum_name.text(),
                    variant: variant.text(),
                },
                (Some(name), None) => TypeRef::Path(name.text()),
                (None, _) => TypeRef::Error,
            },
        },
        ast::Expr::RecordExpr(it) => TypeRef::Record(record_expr_fields_as_types(&it)),
        _ => TypeRef::Error,
    }
}

/// The generic binder of `body` when it is a fn literal with one
/// (`fn::<T, const N: usize>`), range-free. Params without a name are
/// kept with an empty name (broken source; the parse error covers it) so
/// indices — a param's identity — stay aligned with what was written.
fn generics_from_fn_literal(body: Option<ast::Expr>) -> Vec<GenericParamData> {
    let Some(ast::Expr::FnLiteral(fn_lit)) = body else {
        return Vec::new();
    };
    generics_from_param_list(fn_lit.generic_param_list())
}

/// The generic binder of a `type` declaration's `struct`/`enum` literal
/// (`type Pair = struct::<T> { ... }`) — the type-declaration sibling of
/// [`generics_from_fn_literal`], same range-free discipline.
fn generics_from_type_literal(body: Option<ast::Expr>) -> Vec<GenericParamData> {
    let list = match body {
        Some(ast::Expr::RecordExpr(record)) => record.generic_param_list(),
        Some(ast::Expr::EnumExpr(en)) => en.generic_param_list(),
        _ => None,
    };
    generics_from_param_list(list)
}

fn generics_from_param_list(list: Option<ast::GenericParamList>) -> Vec<GenericParamData> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.params()
        .map(|param| match param {
            ast::GenericParam::TypeParam(it) => GenericParamData {
                name: it.name().map(|n| n.text()).unwrap_or_default(),
                kind: GenericParamKind::Type,
            },
            ast::GenericParam::ConstParam(it) => GenericParamData {
                name: it.name().map(|n| n.text()).unwrap_or_default(),
                kind: GenericParamKind::Const(
                    it.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error),
                ),
            },
        })
        .collect()
}

/// If `body` is a fn literal with all params typed and an explicit return type,
/// synthesize a `TypeRef::Fn` so the item acts as a hard inference firewall
/// without requiring a redundant item-level annotation. For a *generic*
/// literal this is the scheme's spelling (TR06 makes it mandatory): the
/// type-param names inside stay syntactic (`TypeRef::Path("T")`) and are
/// bound to rigid [`crate::ty::Ty::Param`]s only during lowering, against
/// [`ItemData::generics`].
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
    type_ref.is_fully_typed().then_some(type_ref)
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
