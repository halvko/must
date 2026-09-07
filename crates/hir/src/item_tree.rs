//! The item tree: a range-free summary of a file's top-level items.
//!
//! Because it carries no text positions, this value compares equal across
//! edits that only touch bodies or whitespace — salsa then backdates it and
//! nothing downstream of name-level information re-runs.

use base_db::{Db, SourceFile, parse};
use syntax::ast::{self, AstNode as _};

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
    /// `type S = struct { ... } without forget;` — this DECLARATION has no
    /// `forget` capability, whatever its fields say. A name-level fact on
    /// purpose: it gates every mention (instantiating a `forget`-bounded
    /// parameter with it is refused), so it belongs where arity and kinds
    /// already live rather than in [`type_decl`] — editing the
    /// declaration's fields must not churn it.
    ///
    /// Only meaningful on a `type` item; `validation` rejects the clause
    /// everywhere else, and the flag stays `false` there.
    pub without_forget: bool,
}

/// One generic parameter of an item's binder, in declaration order — the
/// position in this list is the parameter's identity (see
/// [`crate::ty::ParamTy::index`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParamData {
    /// Empty when the name is missing (broken code).
    pub name: String,
    pub kind: GenericParamKind,
    /// The `T: Display + Write` bounds, syntactic and in written order —
    /// trait names stay [`TypeRef`]s here (resolution is a per-file
    /// judgement, see `crate::traits`). Always empty for const and region
    /// params.
    pub bounds: Vec<TypeRef>,
    /// The `@b: @a + @c` OUTLIVES bounds of a region param, as written
    /// region names (sigil included). Always empty for type and const
    /// params — a region's bounds are regions, never traits, so they get
    /// their own field rather than sharing [`Self::bounds`]'s type domain.
    pub outlives: Vec<String>,
    /// `T without forget` — this parameter is not REQUIRED to have the
    /// `forget` capability, the opt-out from the default bound every type
    /// parameter otherwise carries. Not one of [`Self::bounds`] because it
    /// subtracts: bounds say what the parameter must have, this says what
    /// it need not.
    ///
    /// Inside the declaring body the param is then checked RIGIDLY as a
    /// value that must be consumed — the caller may hand it a linear, so
    /// the body may not assume otherwise.
    pub without_forget: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericParamKind {
    /// `@a` — a rigid REGION parameter, the third binder kind. Regions ride
    /// the same binder slot as types and consts (one list, three kinds) but
    /// are DISTINGUISHED: a region argument is erased, so it never
    /// participates in type identity, never reaches an instance key and
    /// never reaches MIR (the specialization law — lifetimes may reject
    /// programs, never select behaviors).
    Region,
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
    /// An inherent member of a type's `with`-chain. Not a `Value`: a member
    /// carries no item-level `static`/`const` keyword to report (a member's
    /// own `const fn` marker lives on its fn literal), and it is never a
    /// file-scope name, so saying `Value(Static)` here would answer a
    /// question the spelling never asked.
    Member,
    /// `trait Name = requires { ... };` — a trait declaration. Not a type
    /// (a trait classifies types, not values — bare trait names are
    /// rejected in type position) and not a value.
    Trait,
}

impl ItemKind {
    /// The `static`/`const` distinction, when the item has one.
    pub fn constness(self) -> Option<Constness> {
        match self {
            ItemKind::Value(constness) => Some(constness),
            ItemKind::Type | ItemKind::Member | ItemKind::Trait => None,
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
    /// `T.&raw` / `T.&raw mut` — a raw pointer type.
    RawPtr {
        mutable: bool,
        inner: Box<TypeRef>,
    },
    /// `T.&::<@a>` / `T.&mut::<@a>` — a SAFE borrow type. The region rides
    /// the borrow operator's own turbofish and is REQUIRED in signature
    /// position (no elision at launch); `None` records that none was
    /// written, so the mirror pass can say so instead of guessing.
    Borrow {
        mutable: bool,
        region: Option<RegionRef>,
        inner: Box<TypeRef>,
    },
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
    /// `[T; N]` — a fixed-size array type. The length is a const argument
    /// in annotation position, so it stays in the eval-free
    /// [`ConstArgRef`] domain (a `const { ... }` block keeps its
    /// [`ConstArgRef::Block`] shape and is rejected by the mirror pass).
    Array {
        elem: Box<TypeRef>,
        len: ConstArgRef,
    },
    Error,
}

/// One generic argument of a [`TypeRef::Apply`], syntactic and range-free.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArgRef {
    Region(RegionRef),
    Type(TypeRef),
    Const(ConstArgRef),
}

/// A REGION argument, read straight off the syntax and range-free. Regions
/// never carry values and never evaluate, so unlike [`ConstArgRef`] there is
/// no firewall to keep — the whole domain is names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegionRef {
    /// `@a` — a named region, resolved against the enclosing binder.
    Named(String),
    /// `@_` — "there is a region here, infer it". A written token resolved
    /// locally, NOT elision: it says a region exists and declines to name
    /// it, which is exactly what a body-local annotation needs and what a
    /// signature may never say.
    Wildcard,
    /// `@a + @b` — the join. Reads as conjunction: the argument is outlived
    /// by every member, i.e. it names their greatest lower bound.
    Join(Vec<RegionRef>),
    /// A written argument that is not a region at all (`T.&::<usize>`), or
    /// broken syntax. The mirror pass carries the diagnostic.
    Error,
}

impl RegionRef {
    /// Read one region argument off its syntax node. A single token is a
    /// name or the wildcard; several are a join.
    pub fn from_ast(arg: &ast::RegionArg) -> RegionRef {
        let mut regions = arg.regions().map(|token| {
            if token.text() == "@_" {
                RegionRef::Wildcard
            } else {
                RegionRef::Named(token.text().to_owned())
            }
        });
        let Some(first) = regions.next() else {
            return RegionRef::Error;
        };
        let rest: Vec<RegionRef> = regions.collect();
        if rest.is_empty() {
            return first;
        }
        let mut all = vec![first];
        all.extend(rest);
        RegionRef::Join(all)
    }
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
    Char(char),
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
            ast::Type::RawPtrType(it) => match it.ty() {
                Some(inner) => TypeRef::RawPtr {
                    mutable: it.is_mut(),
                    inner: Box::new(TypeRef::from_ast(inner)),
                },
                None => TypeRef::Error,
            },
            // `T.&::<@a>` / `T.&mut::<@a>` — a safe borrow type. Exactly
            // one region argument is meaningful here; the mirror pass
            // reports a wrong count or a wrong KIND (`T.&::<usize>`), which
            // arrives as `RegionRef::Error` so checking still sees a borrow
            // of the right referent.
            ast::Type::BorrowType(it) => match it.ty() {
                Some(inner) => TypeRef::Borrow {
                    mutable: it.is_mut(),
                    region: borrow_region_from_ast(&it),
                    inner: Box::new(TypeRef::from_ast(inner)),
                },
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
            ast::Type::ArrayType(it) => {
                let elem = it.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error);
                let len = it
                    .len()
                    .map(|arg| const_arg_ref_from_ast(&arg))
                    .unwrap_or(ConstArgRef::Error);
                TypeRef::Array {
                    elem: Box::new(elem),
                    len,
                }
            }
        }
    }

    pub fn from_opt_ast(ty: Option<ast::Type>) -> Option<TypeRef> {
        ty.map(TypeRef::from_ast)
    }

    /// Whether a `_` is written anywhere inside this reference. Narrower
    /// than `!is_fully_typed()`, which also counts a return-type-less `fn`
    /// type — this asks only about holes the user actually wrote, for
    /// positions that must name a type OUTRIGHT (a named `Self` argument:
    /// it selects the impl, and v1's impls are all ground).
    pub fn contains_hole(&self) -> bool {
        match self {
            TypeRef::Hole => true,
            TypeRef::Unit
            | TypeRef::Never
            | TypeRef::Path(_)
            | TypeRef::Variant { .. }
            | TypeRef::Error => false,
            TypeRef::Fn { params, ret } => {
                params.iter().any(TypeRef::contains_hole)
                    || ret.as_ref().is_some_and(|r| r.contains_hole())
            }
            TypeRef::RawPtr { inner, .. } | TypeRef::Borrow { inner, .. } => inner.contains_hole(),
            TypeRef::Apply { args, .. } => args.iter().any(|arg| match arg {
                GenericArgRef::Type(ty) => ty.contains_hole(),
                // Neither a const arg (never inferred, TR06) nor a region can
                // be a `_` hole in this sense — a region's `@_` is a region
                // question, answered by the outlives module.
                GenericArgRef::Region(_) | GenericArgRef::Const(_) => false,
            }),
            TypeRef::Record(fields) => fields.iter().any(|(_, ty)| ty.contains_hole()),
            TypeRef::Array { elem, .. } => elem.contains_hole(),
        }
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
            // A borrow's REGION is not part of "fully typed": a missing or
            // wildcard region is a region question, reported as one.
            TypeRef::RawPtr { inner, .. } | TypeRef::Borrow { inner, .. } => inner.is_fully_typed(),
            TypeRef::Apply { args, .. } => args.iter().all(|arg| match arg {
                GenericArgRef::Type(ty) => ty.is_fully_typed(),
                // Const args are never inferred (TR06) — a written one is
                // always "fully typed"; whether it is *legal* is a
                // different question (the diagnostics pass). Regions are
                // outside the type question entirely.
                GenericArgRef::Region(_) | GenericArgRef::Const(_) => true,
            }),
            TypeRef::Record(fields) => fields.iter().all(|(_, ty)| ty.is_fully_typed()),
            // The length is a written const value, never inferred (same
            // reasoning as `Apply`'s const args).
            TypeRef::Array { elem, .. } => elem.is_fully_typed(),
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
            TypeRef::RawPtr { inner, .. } | TypeRef::Borrow { inner, .. } => inner.mentions_fn(),
            TypeRef::Record(fields) => fields.iter().any(|(_, ty)| ty.mentions_fn()),
            TypeRef::Array { elem, .. } => elem.mentions_fn(),
            TypeRef::Apply { args, .. } => args.iter().any(|arg| match arg {
                GenericArgRef::Type(ty) => ty.mentions_fn(),
                GenericArgRef::Region(_) | GenericArgRef::Const(_) => false,
            }),
            TypeRef::Unit
            | TypeRef::Never
            | TypeRef::Path(_)
            | TypeRef::Variant { .. }
            | TypeRef::Hole
            | TypeRef::Error => false,
        }
    }

    /// Whether an array type is written anywhere inside this reference —
    /// the array twin of [`TypeRef::mentions_fn`], with the same use: array
    /// VALUES stay outside the const-arg domain for now (the ruled domain
    /// is builtins + records + variants), so a const param whose declared
    /// type mentions one is rejected at the declaration (and, as a belt, at
    /// every mention — both render [`crate::diag::ARRAY_CONST_ARG`]). Same
    /// nominal-opaque blind spot as `mentions_fn`.
    pub fn mentions_array(&self) -> bool {
        match self {
            TypeRef::Array { .. } => true,
            TypeRef::RawPtr { inner, .. } | TypeRef::Borrow { inner, .. } => inner.mentions_array(),
            TypeRef::Record(fields) => fields.iter().any(|(_, ty)| ty.mentions_array()),
            TypeRef::Fn { params, ret } => {
                params.iter().any(TypeRef::mentions_array)
                    || ret.as_ref().is_some_and(|ret| ret.mentions_array())
            }
            TypeRef::Apply { args, .. } => args.iter().any(|arg| match arg {
                GenericArgRef::Type(ty) => ty.mentions_array(),
                GenericArgRef::Region(_) | GenericArgRef::Const(_) => false,
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

/// The region written on a borrow operator's own turbofish
/// (`T.&mut::<@a>`). `None` when no turbofish was written at all — the
/// no-elision rule makes that an error in signature position, and the
/// mirror pass says so; `Some(RegionRef::Error)` when something was written
/// that is not a region.
fn borrow_region_from_ast(borrow: &ast::BorrowType) -> Option<RegionRef> {
    let list = borrow.generic_arg_list()?;
    let mut args = list.args();
    let first = args.next()?;
    // More than one argument is a wrong-arity mention; keep the first so
    // the referent still checks, and let the mirror pass report the count.
    Some(match first {
        ast::GenericArg::RegionArg(region) => RegionRef::from_ast(&region),
        _ => RegionRef::Error,
    })
}

/// Read a turbofish in *annotation* position, range-free. Const arguments
/// keep only their literal shape ([`ConstArgRef`]) — the firewall: nothing
/// here can require evaluation.
pub(crate) fn generic_args_from_ast(list: &ast::GenericArgList) -> Vec<GenericArgRef> {
    list.args()
        .map(|arg| match arg {
            ast::GenericArg::RegionArg(region) => {
                GenericArgRef::Region(RegionRef::from_ast(&region))
            }
            ast::GenericArg::TypeArg(ty_arg) => {
                GenericArgRef::Type(ty_arg.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error))
            }
            ast::GenericArg::ConstArg(const_arg) => {
                GenericArgRef::Const(const_arg_ref_from_ast(&const_arg))
            }
            // TR01's named arguments name a TRAIT's `Self`; no annotation
            // position takes one (the diagnostics pass says so). Range-free
            // lowering keeps the arity, so the argument counts as an
            // unusable type.
            ast::GenericArg::NamedArg(_) => GenericArgRef::Type(TypeRef::Error),
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
            // A malformed literal (`''`, `'ab'`) has no value; the lexer
            // reported it, so this is the silent broken case.
            Some(ast::LiteralKind::Char(token)) => syntax::char_literal_value(token.text())
                .map_or(ConstArgRef::Error, ConstArgRef::Char),
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
                    // Superset-parsed here (validation rejects it); a
                    // value item's capabilities are its type's.
                    without_forget: false,
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
                // The declaration-site opt-out. Read off the item, not the
                // type literal: the clause trails the whole declaration
                // (`= struct { ... } without forget;`), in the same slot
                // `with` groups use.
                without_forget: item.without_clauses().any(|c| names_forget(&c)),
            },
            ast::Item::TraitItem(it) => ItemData {
                name: item.name().map(|n| n.text()).unwrap_or_default(),
                // The requirement list lives in [`trait_requirements`],
                // mirroring `type_decl`'s split. The reserved
                // `requires::<...>` binder (generic traits) is recorded
                // for arity honesty at mentions.
                kind: ItemKind::Trait,
                type_ref: None,
                generics: generics_from_param_list(
                    it.requires_def().and_then(|def| def.generic_param_list()),
                ),
                // Superset-parsed here too: a trait classifies types, so
                // it has no capabilities of its own to shed.
                without_forget: false,
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
    // A compiler-provided declaration (`scopes::synthetic_decls`): its
    // variants are syntax-shaped data, so everything downstream
    // (`enum_variants`, patterns, widening, match lowering) runs the
    // completely ordinary nominal-enum machinery on it.
    if let Some(decl) = crate::synthetic_decl(db, item) {
        return Some(TypeDeclData::Enum {
            variants: decl.variants.clone(),
        });
    }
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

/// The fields of a `struct` literal used as a type declaration: each
/// field's `: Type` annotation, read as the real type syntax it now is
/// (the equals-defines respell — the colon annotates a TYPE everywhere).
/// A field without an annotation (shorthand, or the retired `name: value`
/// spelling's error recovery) keeps a [`TypeRef::Error`]; the diagnostics
/// pass carries the story. Sorted + deduplicated like
/// [`TypeRef::from_ast`] does for record *types*.
fn record_expr_fields_as_types(record: &ast::RecordExpr) -> Vec<(String, TypeRef)> {
    let mut fields: Vec<(String, TypeRef)> = record
        .fields()
        .filter_map(|field| {
            // A field without a name is broken source (the parse error
            // covers it); nothing meaningful to keep.
            let name = field.name_ref()?.text();
            let ty = field.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error);
            Some((name, ty))
        })
        .collect();
    fields.sort_by(|(a, _), (b, _)| a.cmp(b));
    fields.dedup_by(|second, first| second.0 == first.0);
    fields
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

/// Whether a `without ...` clause names `forget`. Unknown capability names
/// are `validation`'s error, not this function's: an opt-out that names
/// nothing the language knows about opts out of nothing.
fn names_forget(clause: &ast::WithoutClause) -> bool {
    clause.capabilities().any(|name| name.text() == "forget")
}

fn generics_from_param_list(list: Option<ast::GenericParamList>) -> Vec<GenericParamData> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.params()
        .map(|param| match param {
            // A region param's NAME carries its sigil (`@a`), so region and
            // type names live in disjoint spelling spaces and one binder
            // list can hold both without a shadowing question.
            ast::GenericParam::RegionParam(it) => GenericParamData {
                name: it.name().unwrap_or_default(),
                kind: GenericParamKind::Region,
                bounds: Vec::new(),
                outlives: it.bounds().map(|token| token.text().to_owned()).collect(),
                // A region names a duration; validation rejects the clause
                // here, so the flag can never be set.
                without_forget: false,
            },
            ast::GenericParam::TypeParam(it) => GenericParamData {
                name: it.name().map(|n| n.text()).unwrap_or_default(),
                kind: GenericParamKind::Type,
                bounds: it.bounds().map(TypeRef::from_ast).collect(),
                outlives: Vec::new(),
                without_forget: it
                    .without_clause()
                    .is_some_and(|clause| names_forget(&clause)),
            },
            ast::GenericParam::ConstParam(it) => GenericParamData {
                name: it.name().map(|n| n.text()).unwrap_or_default(),
                kind: GenericParamKind::Const(
                    it.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error),
                ),
                bounds: Vec::new(),
                outlives: Vec::new(),
                // Const values are plain data; validation rejects the
                // clause here too.
                without_forget: false,
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
///
/// `None` for MEMBER ids — a member is not an [`ast::Item`]; its syntax is
/// [`member_source`]'s business.
pub fn item_source(db: &dyn Db, item: crate::ItemId<'_>) -> Option<ast::Item> {
    if item.member(db).is_some() {
        return None;
    }
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

// ---- members ------------------------------------------------------------

/// Which semantic home a minted member belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemberHome {
    /// An `impl Self { ... }` member of a type's chain.
    Inherent,
    /// A member of a trait-naming/implementer-naming impl element
    /// (`impl Display { ... }` on a type, `impl usize { ... }` on a
    /// trait). `head` is the element's written head name — the member's
    /// item name is qualified `head::name` so members of sibling impls
    /// never collide (name-keyed identity, one extra name segment).
    TraitImpl { head: String },
}

/// One semantically-supported member of a `type`/`trait` item's
/// `with`-chain, range-free. Identity is `(name, disambiguator)` —
/// NAME-KEYED, never positional: inserting a sibling
/// member doesn't change any other member's identity. For a trait-impl
/// member the name is the QUALIFIED `head::member` spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberData {
    pub name: String,
    /// Which occurrence of `name` among the item's members (0-based) —
    /// duplicates are diagnosed, the first wins lookups.
    pub disambiguator: u32,
    /// The member's signature, synthesized from its defining fn literal's
    /// own annotations exactly like a generic item's scheme
    /// ([`type_ref_from_fn_literal`]); `None` when the member isn't fully
    /// annotated (the definition site carries the diagnostic — member
    /// signatures are always annotation-derived, so dot-call resolution
    /// never needs inference to read a head).
    pub type_ref: Option<TypeRef>,
    pub home: MemberHome,
    /// The member's OWN generic binder (`fmt = fn::<W: Write>(...)`) —
    /// live in full for trait-impl members (a requirement may be a generic
    /// fn); for an INHERENT member only its REGION params are live, and
    /// they are APPENDED to the owner's binder by [`crate::item_data`].
    ///
    /// Why regions and only regions: an inherent member already sees the
    /// owner's type and const params ("the type's own binders are already
    /// in scope"), so a member-own *type* binder is redundant sugar and
    /// stays reserved. A REGION has no such source — regions on type
    /// declarations are themselves reserved — so a borrow-taking member
    /// (`get = fn::<@b>(k: K, m: Self.&mut::<@b>)`) has nowhere else to
    /// bind the per-call region it needs, and with no elision it cannot
    /// decline to name one. The type/const halves are rejected in
    /// `syntax::validation::reject_nested_generic_binder` and dropped here.
    pub generics: Vec<GenericParamData>,
}

impl MemberData {
    /// The member's bare (unqualified) name — `fmt` for `Display::fmt`.
    pub fn bare_name(&self) -> &str {
        match &self.home {
            MemberHome::Inherent => &self.name,
            MemberHome::TraitImpl { head } => self
                .name
                .strip_prefix(head.as_str())
                .and_then(|rest| rest.strip_prefix("::"))
                .unwrap_or(&self.name),
        }
    }
}

/// The SIGNATURE side of the member query split: range-free member
/// facts, one query per owning `type`/`trait` item — a member BODY edit
/// leaves this value unchanged, so sibling signatures (and every dot-call
/// resolution) backdate behind it. Empty for value items.
#[salsa::tracked(returns(ref))]
pub fn type_members<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Vec<MemberData> {
    let Some(decl) = item_source(db, item) else {
        return Vec::new();
    };
    semantic_member_sources(&decl)
        .into_iter()
        .map(|source| {
            let own = match source.member.value() {
                Some(ast::Expr::FnLiteral(fn_lit)) => {
                    generics_from_param_list(fn_lit.generic_param_list())
                }
                _ => Vec::new(),
            };
            let generics = match &source.home {
                // Regions only — see [`MemberData::generics`]. A type or
                // const param here is a reserved spelling with its own
                // diagnostic; dropping it leaves its mentions resolving to
                // the owner's binder (or nothing), exactly as before.
                MemberHome::Inherent => own
                    .into_iter()
                    .filter(|param| matches!(param.kind, GenericParamKind::Region))
                    .collect(),
                MemberHome::TraitImpl { .. } => own,
            };
            MemberData {
                name: source.name,
                disambiguator: source.disambiguator,
                type_ref: type_ref_from_fn_literal(source.member.value()),
                home: source.home,
                generics,
            }
        })
        .collect()
}

/// One minted member's syntax with its identity — what
/// [`semantic_member_sources`] yields.
pub struct MemberSource {
    /// The member's item name (qualified for trait-impl members).
    pub name: String,
    pub disambiguator: u32,
    pub member: ast::Member,
    pub home: MemberHome,
}

/// Every semantically-live impl element of `decl`'s plain `with`-groups,
/// in source order, with its home — the ONE with-chain element traversal
/// shared by member minting ([`semantic_member_sources`]) and the impl
/// index (`crate::traits::trait_impls` and its syntax lookups), so
/// element identity and order can never disagree between the range-free
/// and the syntax side.
pub(crate) fn semantic_impl_elements(decl: &ast::Item) -> Vec<(ast::ImplElement, MemberHome)> {
    let mut out = Vec::new();
    for group in decl.with_groups() {
        // A group with binders or clauses is reserved wholesale: an
        // `impl Self { ... }` sitting directly in it would otherwise mint
        // an unconditional member even though validation flags the group
        // as unsupported.
        if !group.is_plain() {
            continue;
        }
        for element in group.elements() {
            // `unsafe`/`for` heads wrap their `impl` element in their own
            // node, so this direct-child enumeration already skips them —
            // they are reserved and mint nothing.
            let ast::Element::ImplElement(impl_element) = element else {
                continue;
            };
            let home = match syntax::semantic_member_context(impl_element.syntax()) {
                Some(syntax::MemberContext::Inherent) => MemberHome::Inherent,
                Some(syntax::MemberContext::TraitImpl) => {
                    let Some(head) = syntax::impl_element_bare_head(&impl_element) else {
                        continue;
                    };
                    MemberHome::TraitImpl { head: head.text() }
                }
                None => continue,
            };
            out.push((impl_element, home));
        }
    }
    out
}

/// Every member the current semantics MINT an item for, in source
/// order, each with its name-keyed disambiguator: the `=`-defined
/// `fn`-literal members of semantically-live impl elements sitting
/// DIRECTLY in plain `with { ... }` groups — `impl Self` on a type
/// (inherent), `impl Trait` on a NON-generic type, `impl Type` on a
/// non-generic trait (no binders, no clauses, no `unsafe`/`for` heads —
/// everything else is parse-and-reserve and mints nothing).
///
/// The enumeration [`type_members`] and [`member_source`] go through —
/// built on the same element traversal ([`semantic_impl_elements`]) as
/// the impl index, so identity always agrees between the range-free and
/// the syntax side.
pub fn semantic_member_sources(decl: &ast::Item) -> Vec<MemberSource> {
    let mut seen: rustc_hash::FxHashMap<String, u32> = rustc_hash::FxHashMap::default();
    let mut out = Vec::new();
    {
        for (impl_element, home) in semantic_impl_elements(decl) {
            for member in impl_element.members() {
                // Reserved spellings (`type`/`const` members) parse but
                // mint nothing — validation flags them as not supported
                // yet, and minting an item anyway would make that
                // reservation a lie: the member would still dot-call.
                if member.type_token().is_some() || member.const_token().is_some() {
                    continue;
                }
                // Colon-declares and non-fn values mint nothing — they are
                // rejected at the definition site (validation), and a
                // minted item for them would have nothing to check.
                if !matches!(member.value(), Some(ast::Expr::FnLiteral(_))) {
                    continue;
                }
                let Some(name) = member.name() else {
                    continue;
                };
                let name = name.text();
                if name.is_empty() {
                    continue;
                }
                let name = match &home {
                    MemberHome::Inherent => name,
                    MemberHome::TraitImpl { head } => format!("{head}::{name}"),
                };
                let disambiguator = seen.entry(name.clone()).or_insert(0);
                out.push(MemberSource {
                    name: name.clone(),
                    disambiguator: *disambiguator,
                    member,
                    home: home.clone(),
                });
                *disambiguator += 1;
            }
        }
    }
    out
}

/// The syntax node of a MEMBER id — the member-side sibling of
/// [`item_source`]. `None` for top-level ids and vanished members.
pub fn member_source(db: &dyn Db, item: crate::ItemId<'_>) -> Option<ast::Member> {
    let (member_name, member_dis) = item.member(db)?;
    let owner = crate::member_owner(db, item)?;
    let decl = item_source(db, owner)?;
    semantic_member_sources(&decl)
        .into_iter()
        .find(|source| source.name == member_name && source.disambiguator == member_dis)
        .map(|source| source.member)
}

// ---- trait declarations -------------------------------------------------

/// One requirement of a trait declaration (`fmt: fn::<W: Write>(w: W,
/// x: Self) -> W;`), range-free. Identity is the position in
/// [`trait_requirements`]'s list AND the name (first occurrence of a
/// duplicated name wins lookups; the duplicate is diagnosed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraitRequirement {
    pub name: String,
    /// The requirement's signature, synthesized from the colon-declared
    /// fn signature's annotations ([`type_ref_from_decl_signature`]);
    /// `None` when it isn't fully written (the declaration site carries
    /// the diagnostic).
    pub sig: Option<TypeRef>,
    /// The requirement's own generic binder (`fn::<W: Write>`), bounds
    /// included.
    pub generics: Vec<GenericParamData>,
}

/// The requirements a `trait` item declares, in source order — the
/// trait-side counterpart of [`type_decl`], and the same incrementality
/// firewall. Only live colon-declared fn-signature members enter;
/// reserved spellings (defaults, associated types/consts) are skipped
/// (validation carries their story). Empty for non-trait items.
#[salsa::tracked(returns(ref))]
pub fn trait_requirements<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Vec<TraitRequirement> {
    let Some(ast::Item::TraitItem(decl)) = item_source(db, item) else {
        return Vec::new();
    };
    let Some(requires) = decl.requires_def() else {
        return Vec::new();
    };
    let mut out: Vec<TraitRequirement> = Vec::new();
    for member in requires.members() {
        if member.type_token().is_some()
            || member.const_token().is_some()
            || member.eq_token().is_some()
            || member.colon_token().is_none()
        {
            continue;
        }
        let Some(name) = member.name() else {
            continue;
        };
        let name = name.text();
        if name.is_empty() || out.iter().any(|req| req.name == name) {
            continue;
        }
        let (sig, generics) = match member.ty() {
            Some(ast::Type::FnType(fn_type)) => (
                type_ref_from_decl_signature(&fn_type),
                generics_from_param_list(fn_type.generic_param_list()),
            ),
            _ => (None, Vec::new()),
        };
        out.push(TraitRequirement {
            name,
            sig,
            generics,
        });
    }
    out
}

/// The RESERVED associated-type names a trait declares (`type Item;` in
/// its `requires` block), in source order. Associated types are reserved —
/// nothing lowers them — but a qualified path naming one must say so
/// (`Trait::<Self = T>::Item`) instead of claiming the trait has no such
/// requirement.
#[salsa::tracked(returns(ref))]
pub fn trait_assoc_types<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Vec<String> {
    let Some(ast::Item::TraitItem(decl)) = item_source(db, item) else {
        return Vec::new();
    };
    let Some(requires) = decl.requires_def() else {
        return Vec::new();
    };
    requires
        .members()
        .filter(|member| member.type_token().is_some())
        .filter_map(|member| member.name())
        .map(|name| name.text())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Synthesize a [`TypeRef::Fn`] from a colon-declared member signature
/// (`fn::<W: Write>(w: W, x: Self) -> W` — NAMED params, so the types
/// come from the param list, not from bare child types). `None` unless
/// every param is annotated and the return type is written (declarations
/// have nothing to infer from), or when the signature carries the
/// reserved `unsafe` marker.
fn type_ref_from_decl_signature(fn_type: &ast::FnType) -> Option<TypeRef> {
    if fn_type.unsafe_token().is_some() {
        return None;
    }
    let params: Vec<TypeRef> = fn_type
        .param_list()?
        .params()
        .map(|p| Some(TypeRef::from_ast(p.ty()?)))
        .collect::<Option<_>>()?;
    let ret = fn_type
        .ret_type()
        .and_then(|rt| rt.ty())
        .map(|t| Box::new(TypeRef::from_ast(t)));
    let type_ref = TypeRef::Fn { params, ret };
    type_ref.is_fully_typed().then_some(type_ref)
}
