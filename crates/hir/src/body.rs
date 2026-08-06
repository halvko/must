//! Body lowering: AST → arena-based HIR.
//!
//! [`Body`] is deliberately range-free so it compares equal when an edit only
//! moves things around — that equality is what stops salsa from re-running
//! inference of untouched items. All positions live in [`BodySourceMap`],
//! which only the final, range-producing layers (diagnostics, IDE) read.

use base_db::Db;
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;
use syntax::SyntaxNodePtr;
use syntax::ast::{self, AstNode as _};

use crate::ItemId;
use crate::item_tree::{TypeRef, item_source};

pub type ExprId = Idx<ExprData>;
pub type BindingId = Idx<BindingData>;
pub type PatId = Idx<PatData>;

pub use syntax::ast::BinOp;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body {
    pub exprs: Arena<ExprData>,
    pub bindings: Arena<BindingData>,
    /// Match-arm patterns. Its own arena (not `ExprData`): a pattern is not
    /// a value-producing expression, and the docs' future param
    /// destructuring will reuse it.
    pub pats: Arena<PatData>,
    /// The item's initializer expression (usually a `fn` literal).
    pub root: Option<ExprId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingData {
    pub name: String,
    pub type_ref: Option<TypeRef>,
    /// Whether the binding was introduced with `mut` (`let mut` / a `mut`
    /// parameter). A hole (`_`) is never mutable — there is no name to
    /// assign through — regardless of a written `mut` (validation flags
    /// that as pointless).
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprData {
    /// Source was broken; nothing to lower.
    Missing,
    Literal(LiteralData),
    NameRef(String),
    /// `Shape::Circle` — a two-segment variant path. The base is a real
    /// [`ExprData::NameRef`] allocated on the first segment's node (so
    /// scopes/resolution/goto treat `Shape` like any other name); the
    /// variant is resolved *type-directed* against the enum's declaration
    /// during inference, not here and not in scopes.
    VariantPath {
        /// The enum name (a `NameRef` expression).
        base: ExprId,
        /// The second segment's text. Empty when broken (`Shape::` — the
        /// parse error covers it).
        variant: String,
        /// The OWNER's written turbofish (`Option::<usize>::Some`), when
        /// present — matched against the enum's binder during inference,
        /// exactly like a [`ExprData::GenericApp`]'s.
        args: Option<Vec<GenericArgData>>,
        /// The SECOND segment's OWN written turbofish
        /// (`Measured::size::<usize>`), when present — kept separate from
        /// `args` all the way down, because it belongs to a binder the
        /// second segment does not have YET: a member's own generic
        /// arguments are reserved (inference states it), a variant's are a
        /// mistake for the owner's. Neither may ever be spent on the
        /// owner's binder, which is why the two lists never merge.
        member_args: Option<Vec<GenericArgData>>,
    },
    /// `::Circle` / `::Circle(3)` — the elided-sigil variant EXPRESSION,
    /// the mirror of the elided-sigil variant pattern. There is no base to
    /// allocate: the enum comes from the position's EXPECTED type, the way
    /// the pattern's comes from the scrutinee. Reject-only sugar — when no
    /// expected enum is in view, inference refuses and names the qualified
    /// spelling, which stays canonical.
    ElidedVariant {
        /// The variant's name. Empty when broken (`::` alone — the parse
        /// error covers it).
        variant: String,
    },
    /// `f::<usize, 42>` — a turbofish mention. The base is a real
    /// [`ExprData::NameRef`] allocated on the first segment's node (same
    /// scheme as [`ExprData::VariantPath`]: resolution, goto-def and hover
    /// on `f` work like any reference); the arguments are matched against
    /// the resolved item's generic binder during inference.
    GenericApp {
        /// The mentioned name (a `NameRef` expression).
        base: ExprId,
        /// The written arguments, in source order.
        args: Vec<GenericArgData>,
    },
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
        /// Whether the call was WRITTEN as a dot-call — the callee is
        /// DIRECTLY a field-access expression (`recv.name(args)`), with no
        /// parens in between. Syntax-directed member selection (G13)
        /// keys off this: `recv.name(...)` resolves the member
        /// first, while `(recv.name)(...)` is an ordinary value call of
        /// the field (parens lower transparently, so this bit is the only
        /// trace of them).
        dot_call: bool,
    },
    Bin {
        op: Option<BinOp>,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// `-x`: unary minus on a number. On a literal (`-5`) the negation
    /// participates in the literal's range check (`-128` fits `i8`); on
    /// unsigned operands it is legal syntax that traps at runtime unless
    /// the operand is zero (the overflow rule).
    Neg {
        operand: ExprId,
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
    /// `const { ... }`. Transparent for typing and evaluation — carries the
    /// same value as `body` — but keeps its own `ExprId` so source-map
    /// lookups (hover, go-to-def) land on the `const` wrapper too.
    ConstBlock {
        body: ExprId,
    },
    /// `struct { x = e, y }`: a record literal (equals-defines). Fields
    /// keep source order (squiggles and evaluation order follow the
    /// source); the *type* canonicalizes to name order in inference. A
    /// shorthand field `x` lowers as the name plus a
    /// [`ExprData::NameRef`] for `x`, so the reference resolves through
    /// scopes like any other. A field may additionally carry its `: Type`
    /// ascription (`x: usize = 10`) — checked against the value AND the
    /// position's expectation during inference.
    RecordLit {
        fields: Vec<RecordLitField>,
    },
    /// `receiver.field`. The field name is not an expression of its own —
    /// it names a projection, not a value in scope.
    Field {
        receiver: ExprId,
        /// Empty when the name is missing (broken source, e.g. `a.`); the
        /// parse error covers it, inference stays silent.
        name: String,
    },
    /// `[e1, e2, e3]`: an array literal — the length is the element count.
    ArrayLit {
        elements: Vec<ExprId>,
    },
    /// `[e; N]`: the repeat form. The count lowers as an ordinary
    /// expression (scoped, resolved); inference restricts it to the
    /// const-arg forms — an integer literal or a const-param read — so the
    /// array's TYPE can carry the length on the eval-free path.
    ArrayRepeat {
        element: ExprId,
        count: ExprId,
    },
    /// `base[index]`: an element read. As an assignment target
    /// (`a[i] = e;`) the same node names the written-into place.
    Index {
        base: ExprId,
        index: ExprId,
    },
    /// `place.&raw` / `place.&raw mut`: takes the address of a place,
    /// producing a raw pointer. The operand lowers as an ordinary
    /// expression (its reads resolve, hover works); inference restricts it
    /// to places — a variable, a chain of its fields and elements, a
    /// `static`/`const` item, or a chain rooted in a deref (`p.*.x`).
    AddrOf {
        mutable: bool,
        place: ExprId,
    },
    /// `place.&` / `place.&mut` (optionally `place.&mut::<@a>`): a SAFE
    /// borrow of a place. The operand lowers as an ordinary expression, so
    /// its reads resolve and hover works; inference restricts it to places,
    /// exactly as [`Self::AddrOf`] does.
    ///
    /// The region is optional here and REQUIRED in a type annotation, and
    /// that asymmetry is the no-elision rule read correctly: a signature's
    /// regions are parameters and must be named, a body's are existentials
    /// and are inferred. An omitted turbofish means the same thing `@_`
    /// does.
    Borrow {
        mutable: bool,
        place: ExprId,
        region: Option<crate::item_tree::RegionRef>,
    },
    /// `receiver.*`: reads through a raw pointer OR a safe borrow. Which
    /// one decides whether an `unsafe` block is needed (flavor determines
    /// safety) — a question only inference can answer, so lowering keeps
    /// one node and `unsafe_check` consults types.
    Deref {
        receiver: ExprId,
    },
    /// `unsafe { ... }`. A pure *checker region* — transparent for typing,
    /// scoping and evaluation exactly like a plain block; only the unsafe
    /// pass ([`crate::unsafe_check`]) reads the boundary.
    Unsafe {
        body: ExprId,
    },
    FnLiteral {
        /// Whether the literal was written `const fn`. Orthogonal to the
        /// enclosing item's own `static`/`const`; read by the separate
        /// `const_check` pass, not by typing.
        is_const: bool,
        params: Vec<Param>,
        ret_type: Option<TypeRef>,
        /// `None` for an `extern fn`, and only for an `extern fn` — a HOST
        /// IMPORT declaration rather than a definition, and the one fn
        /// literal in the language with nothing to check, lower or run. An
        /// extern literal is exactly a signature: it has no body, because
        /// the code it names is on the other side of the boundary, so
        /// `None` IS the fact and nothing else records it.
        body: Option<ExprId>,
    },
    /// `match scrutinee { arms }`. Typing-wise the arms are witnesses of
    /// one join (like `if`/`else` branches); dispatch-wise MIR decides
    /// between a tag switch (enum-typed scrutinee) and a direct
    /// destructure (variant-typed scrutinee — no dispatch at all).
    Match {
        scrutinee: ExprId,
        arms: Vec<MatchArm>,
    },
    /// `loop { body }`: an infinite loop. Its value is carried by `break`s
    /// — the break values are witnesses of one join whose result is the
    /// loop's type; the body's own tail value is discarded (running off the
    /// body's end continues the loop). No breaks at all: the loop types `!`.
    Loop {
        body: ExprId,
    },
    /// `break` / `break value`: exits the enclosing `loop`, carrying the
    /// value (`()` when absent) as the loop's. An expression of type `!` —
    /// it composes with the never machinery, so `if c { break; }` needs no
    /// special casing.
    Break {
        value: Option<ExprId>,
    },
    /// `continue`: restarts the enclosing `loop`'s body. Typed `!` like
    /// `break`.
    Continue,
    /// `return` / `return value`: exits the enclosing BODY — the nearest
    /// `fn` literal or `const` block — with the value (`()` when absent).
    /// `break`'s sibling one tier up: an expression of type `!`, so
    /// `let x = if c { 1 } else { return 0 };` composes through the never
    /// machinery with no special casing. The value is checked against that
    /// body's return type exactly as its tail expression is.
    Return {
        value: Option<ExprId>,
    },
}

/// One turbofish argument, disambiguated by *form* at parse time (a
/// literal or `const`-prefixed expression is a const arg; anything else is
/// a type — see the grammar): whether the position actually takes a type
/// or a const is checked against the declaration during inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericArgData {
    /// A REGION argument (`@a`, `@_`, `@a + @b`). Carries no expression and
    /// no value: regions are erased, so nothing here survives to MIR.
    Region(crate::item_tree::RegionRef),
    /// A type argument, including the `_` hole (which is an *error* in a
    /// const position — const args are never inferred, TR06).
    Type(TypeRef),
    /// A const argument's value expression. Lowered like any expression
    /// (scoped, resolved, type-checked); its *evaluation* is staged
    /// for instance identity — inference records these in
    /// [`crate::infer::InferenceResult::const_args_of_expr`].
    Const(ExprId),
    /// A NAMED argument — `Self = Point` (TR01). Position-irrelevant: the
    /// name selects the argument, so a named `Self` composes with a generic
    /// trait's positional arguments in any order. Only `Self` is nameable in
    /// v1 (general named args are gated on the binder-names-as-API ruling);
    /// anything else is diagnosed at the mention.
    Named { name: String, ty: TypeRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pat: PatId,
    pub body: ExprId,
}

/// One field of a [`ExprData::RecordLit`]: `name[: Type][= value]` — the
/// value is the shorthand's own [`ExprData::NameRef`] when neither `:` nor
/// `=` was written, and a [`ExprData::Missing`] for a value-less annotated
/// field (validation carries that error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLitField {
    pub name: String,
    pub value: ExprId,
    /// The written `: Type` ascription, when present.
    pub type_ref: Option<TypeRef>,
}

/// One parameter of a `fn` literal: a pattern (construction's mirror image)
/// plus its own optional `: Type` annotation. The annotation types the
/// *whole* destructured value — for a bare [`PatData::Bind`] it is the same
/// annotation [`BindingData::type_ref`] already carries (kept there too, so
/// the common case needs no special-casing); for [`PatData::Record`] and
/// [`PatData::Newtype`] this is the only place it lives, since there is no
/// single binding to hang it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub pat: PatId,
    pub type_ref: Option<TypeRef>,
}

/// A pattern. Match-arm patterns (`Wildcard`/`Bind`/`Variant`/`Char`) stay
/// deliberately flat (no nesting, or-patterns or guards — those land later
/// as one coherent pattern-language feature); `Record` and
/// `Newtype` are `let`/parameter patterns, construction's mirror image, and
/// only ever appear as a whole `let`/parameter pattern (or nested one level
/// inside a `Newtype`) — never inside a variant pattern's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatData {
    /// Source was broken (including the reserved `..` as a whole pattern —
    /// validation rejects it); matches nothing.
    Missing,
    /// `_`: matches anything, binds nothing. Only a match-arm pattern — a
    /// `let`/parameter hole lowers as an (unnamed) [`PatData::Bind`]
    /// instead, so it still gets a value slot at runtime, exactly as an
    /// ordinary hole binding always has.
    Wildcard,
    /// A bare name: binds the whole scrutinee — unless inference
    /// reinterprets it, type-directed, as a payload-less variant of the
    /// scrutinee's enum (`Point =>` on a `Shape` scrutinee). Also how a
    /// `let`/parameter hole (`_`) lowers (an unnamed binding), and how a
    /// bare name inside a [`PatData::Newtype`] binds the whole underlying
    /// value (`Foo(inner)`).
    Bind(BindingId),
    /// `'x'` — a character literal pattern: matches exactly this scalar
    /// value, binds nothing. The only literal pattern so far (`validation`
    /// rejects the others, which parse into the same node), so it carries a
    /// bare `char`; the day another literal kind becomes a pattern this
    /// generalizes to a [`LiteralData`].
    Char(char),
    /// `Circle(r)` / `Shape::Circle(r)`: a variant pattern, construction's
    /// mirror image. The variant resolves against the scrutinee's enum
    /// (bare) or the named enum (qualified) during inference.
    Variant {
        /// `Some` for the qualified spelling (`Shape::Circle`), `None` for
        /// the bare one (`Circle`).
        enum_name: Option<String>,
        /// Empty when broken (`Shape:: =>` — the parse error covers it).
        variant: String,
        /// Positional payload bindings, in source order; `_` holes lower
        /// as unnamed bindings (like hole params).
        bindings: Vec<BindingId>,
        /// A `..` rest marker was written. Reserved for record patterns
        /// (validation rejects it today); carried so the arena is already
        /// shaped for them.
        rest: bool,
    },
    /// `struct { x, y as z, mut w, .. }` — destructures a structural
    /// record, construction's mirror image ([`crate::body::ExprData::RecordLit`]).
    /// Field patterns are flat (no nested sub-patterns in v1: each field
    /// binds directly, with an optional rename).
    Record {
        fields: Vec<RecordPatField>,
        /// A `..` rest marker was written: fields not named here are simply
        /// not bound — exact-equality typing of the *scrutinee* still
        /// applies (every field of the type must exist; `..` only means
        /// "don't bind the rest", never width subtyping). Without it every
        /// field of the scrutinee's type must be named.
        rest: bool,
    },
    /// `Name(pattern)` — unwraps a newtype and destructures its underlying
    /// shape, construction's mirror image (`Name(struct { ... })`). Erased
    /// at runtime (a named type's value *is* its underlying value — see
    /// [`crate::ty::Ty::Named`]'s doc comment), so lowering this is a pure
    /// retype, no MIR operation.
    Newtype {
        /// The newtype's name, resolved (type-directed) during inference.
        type_name: String,
        inner: PatId,
    },
}

/// One field of a [`PatData::Record`] pattern: which field of the record is
/// matched, and the binding it's bound to (the same one when there is no
/// `as` rename — see [`crate::body::LowerCtx::lower_binding_pattern`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPatField {
    pub field: String,
    pub binding: BindingId,
}

impl Body {
    /// Every binding one pattern introduces, flattened in source order —
    /// the `let`/parameter counterpart of a scope's entries: a `Bind` binds
    /// itself, a `Record` its (possibly renamed) fields, a `Newtype`
    /// whatever its inner pattern binds, and `Wildcard`/`Missing`/`Char`/`Variant`
    /// (match-only in binding position; never produced there) bind nothing
    /// beyond what's already covered by their own call sites.
    pub fn pat_bindings(&self, pat: PatId) -> Vec<(String, BindingId)> {
        let mut out = Vec::new();
        self.collect_pat_bindings(pat, &mut out);
        out
    }

    fn collect_pat_bindings(&self, pat: PatId, out: &mut Vec<(String, BindingId)>) {
        match &self.pats[pat] {
            PatData::Missing | PatData::Wildcard | PatData::Char(_) => {}
            PatData::Bind(binding) => out.push((self.bindings[*binding].name.clone(), *binding)),
            PatData::Variant { bindings, .. } => {
                out.extend(bindings.iter().map(|&b| (self.bindings[b].name.clone(), b)));
            }
            PatData::Record { fields, .. } => {
                out.extend(
                    fields
                        .iter()
                        .map(|f| (self.bindings[f.binding].name.clone(), f.binding)),
                );
            }
            PatData::Newtype { inner, .. } => self.collect_pat_bindings(*inner, out),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralData {
    /// `None` if the literal doesn't fit in u128.
    Int(Option<u128>),
    Str(String),
    Bool(bool),
    /// `'x'` — one Unicode scalar value, already cooked (escapes decoded).
    /// Unlike [`LiteralData::Int`] there is no `Option` here: a literal the
    /// lexer errored on lowers as [`ExprData::Missing`] instead, because a
    /// character literal has no partial reading — an empty or two-character
    /// literal names no value at all, whereas an over-long integer still
    /// names a number the checker can talk about.
    Char(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        pat: PatId,
        /// The `let`'s own `: Type` annotation, when written. Mirrors
        /// [`Param::type_ref`]'s reasoning: kept here (in addition to
        /// [`BindingData::type_ref`] for the common bare-name case) because
        /// a destructuring pattern has no single binding to hang it on.
        type_ref: Option<TypeRef>,
        init: ExprId,
    },
    /// `target = value;`. `target` lowers as a normal expression (so its
    /// `NameRef` gets the usual source-map + resolution entries — goto-def
    /// and hover on the LHS work for free), even though the only target
    /// shape validation allows is a plain variable.
    Assign {
        target: ExprId,
        value: ExprId,
    },
    Expr(ExprId),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodySourceMap {
    expr_map: FxHashMap<SyntaxNodePtr, ExprId>,
    expr_map_back: ArenaMap<ExprId, SyntaxNodePtr>,
    binding_map: FxHashMap<SyntaxNodePtr, BindingId>,
    binding_map_back: ArenaMap<BindingId, SyntaxNodePtr>,
    /// Maps a binding to the syntax node of its type annotation, when present.
    binding_annotation_back: ArenaMap<BindingId, SyntaxNodePtr>,
    pat_map: FxHashMap<SyntaxNodePtr, PatId>,
    pat_map_back: ArenaMap<PatId, SyntaxNodePtr>,
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
    pub fn annotation_for_binding(&self, binding: BindingId) -> Option<SyntaxNodePtr> {
        self.binding_annotation_back.get(binding).copied()
    }
    pub fn pat_for_node(&self, ptr: SyntaxNodePtr) -> Option<PatId> {
        self.pat_map.get(&ptr).copied()
    }
    pub fn node_for_pat(&self, pat: PatId) -> Option<SyntaxNodePtr> {
        self.pat_map_back.get(pat).copied()
    }
}

#[salsa::tracked(returns(ref))]
pub fn body_with_source_map<'db>(db: &'db dyn Db, item: ItemId<'db>) -> (Body, BodySourceMap) {
    let mut ctx = LowerCtx::default();
    // A `type` item's RHS is a *type declaration*, not a value: it is read
    // syntactically by `type_decl` and never lowered, inferred, const-checked
    // or evaluated — so its body here is empty (`root: None`). A MEMBER id's
    // body is its defining fn literal (the BODY side of the name-keyed
    // signature/body split: editing it dirties only this member's own
    // checks).
    let root = if item.member(db).is_some() {
        crate::item_tree::member_source(db, item)
            .and_then(|member| member.value())
            .map(|expr| ctx.lower_expr(expr))
    } else {
        match item_source(db, item) {
            Some(syntax::ast::Item::StaticItem(it)) => it.body().map(|expr| ctx.lower_expr(expr)),
            // A `trait` item's RHS is a declaration too — read by
            // `trait_requirements`, never lowered as a value.
            Some(syntax::ast::Item::TypeItem(_) | syntax::ast::Item::TraitItem(_)) | None => None,
        }
    };
    (
        Body {
            exprs: ctx.exprs,
            bindings: ctx.bindings,
            pats: ctx.pats,
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

#[derive(Default)]
struct LowerCtx {
    exprs: Arena<ExprData>,
    bindings: Arena<BindingData>,
    pats: Arena<PatData>,
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

    fn alloc_pat(&mut self, data: PatData, node: &syntax::SyntaxNode) -> PatId {
        let id = self.pats.alloc(data);
        let ptr = SyntaxNodePtr::new(node);
        self.source_map.pat_map.insert(ptr, id);
        self.source_map.pat_map_back.insert(id, ptr);
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

    /// One written turbofish list, in source order. Type arguments stay
    /// syntactic (`TypeRef`, range-free); const arguments are ordinary
    /// expressions, allocated here so they resolve, type and evaluate like
    /// any other expression; a named argument (`Self = Type`) keeps its
    /// name for the semantic layer.
    ///
    /// Shared by every list a path can carry — the owner's and a second
    /// segment's own — so the two differ only in WHERE they are stored,
    /// never in how they are read.
    fn lower_generic_args(&mut self, list: ast::GenericArgList) -> Vec<GenericArgData> {
        list.args()
            .map(|arg| match arg {
                ast::GenericArg::RegionArg(region) => {
                    GenericArgData::Region(crate::item_tree::RegionRef::from_ast(&region))
                }
                ast::GenericArg::TypeArg(ty_arg) => GenericArgData::Type(
                    ty_arg.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error),
                ),
                ast::GenericArg::ConstArg(const_arg) => {
                    GenericArgData::Const(self.lower_opt_expr(const_arg.expr()))
                }
                ast::GenericArg::NamedArg(named) => GenericArgData::Named {
                    name: named.name_ref().map(|n| n.text()).unwrap_or_default(),
                    ty: named.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error),
                },
            })
            .collect()
    }

    fn lower_expr(&mut self, expr: ast::Expr) -> ExprId {
        match expr {
            ast::Expr::Literal(it) => {
                let data = match it.kind() {
                    Some(ast::LiteralKind::Int(token)) => {
                        let text = token.text().replace('_', "");
                        LiteralData::Int(text.parse().ok())
                    }
                    Some(ast::LiteralKind::Str(token)) => LiteralData::Str(unescape(token.text())),
                    Some(ast::LiteralKind::Bool(value)) => LiteralData::Bool(value),
                    // A malformed character literal names no value (see
                    // `LiteralData::Char`): the lexer's diagnostic is the
                    // whole story, so this lowers as broken source.
                    Some(ast::LiteralKind::Char(token)) => {
                        match syntax::char_literal_value(token.text()) {
                            Some(value) => LiteralData::Char(value),
                            None => return self.missing_expr(),
                        }
                    }
                    None => return self.missing_expr(),
                };
                self.alloc_expr(ExprData::Literal(data), it.syntax())
            }
            ast::Expr::PathExpr(it) => {
                let Some(name_ref) = it.name_ref() else {
                    return self.missing_expr();
                };
                // `f::<usize, 42>`: like a variant path, the base lowers as
                // a normal `NameRef` on its own node; the whole path is the
                // turbofish mention. Type args stay syntactic (`TypeRef`,
                // range-free); const args are ordinary expressions. With a
                // trailing variant segment (`Option::<usize>::Some`) the
                // whole path is a variant-path mention carrying the args.
                if let Some(arg_list) = it.generic_arg_list() {
                    let base =
                        self.alloc_expr(ExprData::NameRef(name_ref.text()), name_ref.syntax());
                    let args = self.lower_generic_args(arg_list);
                    if let Some(variant) = it.variant_name_ref() {
                        // `Pair::<usize>::first::<T>` — arguments on BOTH
                        // segments. They stay two lists (see
                        // `ExprData::VariantPath::member_args`).
                        let member_args = it
                            .member_generic_arg_list()
                            .map(|list| self.lower_generic_args(list));
                        return self.alloc_expr(
                            ExprData::VariantPath {
                                base,
                                variant: variant.text(),
                                args: Some(args),
                                member_args,
                            },
                            it.syntax(),
                        );
                    }
                    return self.alloc_expr(ExprData::GenericApp { base, args }, it.syntax());
                }
                // `Shape::Circle`: the base lowers as a normal `NameRef` on
                // its own node (resolution, goto-def and hover on `Shape`
                // work like any reference); the whole path is the
                // variant-path expression.
                if it.colon2_token().is_some() {
                    let base =
                        self.alloc_expr(ExprData::NameRef(name_ref.text()), name_ref.syntax());
                    let variant = it.variant_name_ref().map(|n| n.text()).unwrap_or_default();
                    // `Measured::size::<usize>` — the second segment's OWN
                    // turbofish. Lowered for real (its const arguments are
                    // ordinary expressions that must resolve and type like
                    // any other), kept out of `args`, and reserved in
                    // inference, which is the layer that knows whether the
                    // segment names a member or a variant.
                    let member_args = it
                        .member_generic_arg_list()
                        .map(|list| self.lower_generic_args(list));
                    return self.alloc_expr(
                        ExprData::VariantPath {
                            base,
                            variant,
                            args: None,
                            member_args,
                        },
                        it.syntax(),
                    );
                }
                self.alloc_expr(ExprData::NameRef(name_ref.text()), it.syntax())
            }
            ast::Expr::ElidedVariantExpr(it) => {
                let variant = it.variant_name_ref().map(|n| n.text()).unwrap_or_default();
                self.alloc_expr(ExprData::ElidedVariant { variant }, it.syntax())
            }
            ast::Expr::CallExpr(it) => {
                let dot_call = matches!(it.callee(), Some(ast::Expr::FieldExpr(_)));
                let callee = self.lower_opt_expr(it.callee());
                let args = it
                    .arg_list()
                    .map(|args| args.args().map(|a| self.lower_expr(a)).collect())
                    .unwrap_or_default();
                self.alloc_expr(
                    ExprData::Call {
                        callee,
                        args,
                        dot_call,
                    },
                    it.syntax(),
                )
            }
            ast::Expr::BinExpr(it) => {
                let lhs = self.lower_opt_expr(it.lhs());
                let rhs = self.lower_opt_expr(it.rhs());
                let op = it.op();
                self.alloc_expr(ExprData::Bin { op, lhs, rhs }, it.syntax())
            }
            ast::Expr::NegExpr(it) => {
                let operand = self.lower_opt_expr(it.expr());
                self.alloc_expr(ExprData::Neg { operand }, it.syntax())
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
            ast::Expr::ConstBlockExpr(it) => {
                let body = match it.block() {
                    Some(block) => self.lower_block(block),
                    None => self.missing_expr(),
                };
                self.alloc_expr(ExprData::ConstBlock { body }, it.syntax())
            }
            ast::Expr::RecordExpr(it) => {
                let fields = it
                    .fields()
                    .filter_map(|field| {
                        // No name: broken source, the parse error covers it.
                        let name_ref = field.name_ref()?;
                        let name = name_ref.text();
                        let value = if field.is_shorthand() {
                            // `x` is sugar for `x = x`: the value is a normal
                            // `NameRef` allocated on the name's own node, so
                            // resolution, hover and the source map treat it
                            // like any other reference to `x`.
                            self.alloc_expr(ExprData::NameRef(name.clone()), name_ref.syntax())
                        } else {
                            // `= value` — or the retired `: value`
                            // spelling's superset-parsed expression (its
                            // parse error carries the story; keeping the
                            // value keeps inference and hover working).
                            // An annotated field with NO value lowers
                            // `Missing` (validation carries that error).
                            self.lower_opt_expr(field.expr())
                        };
                        Some(RecordLitField {
                            name,
                            value,
                            type_ref: field.ty().map(TypeRef::from_ast),
                        })
                    })
                    .collect();
                self.alloc_expr(ExprData::RecordLit { fields }, it.syntax())
            }
            ast::Expr::FieldExpr(it) => {
                let receiver = self.lower_opt_expr(it.receiver());
                let name = it.name_ref().map(|n| n.text()).unwrap_or_default();
                self.alloc_expr(ExprData::Field { receiver, name }, it.syntax())
            }
            ast::Expr::ArrayExpr(it) => {
                if it.is_repeat() {
                    let (element, count) = match it.repeat_parts() {
                        Some((element, count)) => {
                            (self.lower_expr(element), self.lower_opt_expr(count))
                        }
                        // `[; n]` and friends: broken source, the parse
                        // error covers it.
                        None => (self.missing_expr(), self.missing_expr()),
                    };
                    self.alloc_expr(ExprData::ArrayRepeat { element, count }, it.syntax())
                } else {
                    let elements = it.elements().map(|e| self.lower_expr(e)).collect();
                    self.alloc_expr(ExprData::ArrayLit { elements }, it.syntax())
                }
            }
            ast::Expr::IndexExpr(it) => {
                let base = self.lower_opt_expr(it.base());
                let index = self.lower_opt_expr(it.index());
                self.alloc_expr(ExprData::Index { base, index }, it.syntax())
            }
            ast::Expr::AddrOfExpr(it) => {
                let place = self.lower_opt_expr(it.expr());
                self.alloc_expr(
                    ExprData::AddrOf {
                        mutable: it.is_mut(),
                        place,
                    },
                    it.syntax(),
                )
            }
            ast::Expr::DerefExpr(it) => {
                let receiver = self.lower_opt_expr(it.receiver());
                self.alloc_expr(ExprData::Deref { receiver }, it.syntax())
            }
            ast::Expr::BorrowExpr(it) => {
                let place = self.lower_opt_expr(it.receiver());
                let region = it
                    .generic_arg_list()
                    .and_then(|list| list.args().next())
                    .map(|arg| match arg {
                        ast::GenericArg::RegionArg(region) => {
                            crate::item_tree::RegionRef::from_ast(&region)
                        }
                        _ => crate::item_tree::RegionRef::Error,
                    });
                self.alloc_expr(
                    ExprData::Borrow {
                        mutable: it.is_mut(),
                        place,
                        region,
                    },
                    it.syntax(),
                )
            }
            ast::Expr::UnsafeBlockExpr(it) => {
                let body = self.lower_opt_expr(it.expr());
                self.alloc_expr(ExprData::Unsafe { body }, it.syntax())
            }
            // An `enum` literal is type-declaration syntax; in a value body
            // it is broken source (validation rejects it), so there is
            // nothing to lower.
            ast::Expr::EnumExpr(_) => self.missing_expr(),
            ast::Expr::MatchExpr(it) => {
                let scrutinee = self.lower_opt_expr(it.scrutinee());
                let arms = it
                    .arms()
                    .map(|arm| {
                        let pat = match arm.pat() {
                            Some(pat) => self.lower_pat(pat),
                            // No pattern at all: broken source, the parse
                            // error covers it.
                            None => self.pats.alloc(PatData::Missing),
                        };
                        let body = self.lower_opt_expr(arm.body());
                        MatchArm { pat, body }
                    })
                    .collect();
                self.alloc_expr(ExprData::Match { scrutinee, arms }, it.syntax())
            }
            ast::Expr::LoopExpr(it) => {
                let body = self.lower_opt_expr(it.body());
                self.alloc_expr(ExprData::Loop { body }, it.syntax())
            }
            ast::Expr::BreakExpr(it) => {
                let value = it.expr().map(|e| self.lower_expr(e));
                self.alloc_expr(ExprData::Break { value }, it.syntax())
            }
            ast::Expr::ContinueExpr(it) => self.alloc_expr(ExprData::Continue, it.syntax()),
            ast::Expr::ReturnExpr(it) => {
                let value = it.expr().map(|e| self.lower_expr(e));
                self.alloc_expr(ExprData::Return { value }, it.syntax())
            }
            ast::Expr::FnLiteral(it) => {
                let is_const = it.is_const();
                let params = it
                    .param_list()
                    .map(|list| {
                        list.params()
                            .map(|param| {
                                let type_ref = TypeRef::from_opt_ast(param.ty());
                                let pat = match param.pat() {
                                    Some(pat) => self.lower_binding_pattern(
                                        pat,
                                        type_ref.clone(),
                                        param.is_mut(),
                                    ),
                                    None => self.pats.alloc(PatData::Missing),
                                };
                                if let Some(ty) = param.ty()
                                    && let PatData::Bind(binding) = &self.pats[pat]
                                {
                                    let binding = *binding;
                                    self.source_map
                                        .binding_annotation_back
                                        .insert(binding, SyntaxNodePtr::new(ty.syntax()));
                                }
                                Param { pat, type_ref }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let ret_type = it
                    .ret_type()
                    .map(|rt| rt.ty().map(TypeRef::from_ast).unwrap_or(TypeRef::Error));
                // `None` IS the import, so an IMPORT only ever exists where
                // the declaration is well formed — see
                // `ast::FnLiteral::declares_host_import`. A written body, or
                // a placement that gives the import no name of its own, is
                // already a syntax error; lowering it as an import anyway
                // would launder that error into a host refusal at run time
                // ("no host implementation for `bad`"), which blames the
                // wrong side of a boundary the program never crossed. The
                // superset rule applies instead: keep what the user wrote,
                // and a missing body lowers to `Missing` like any other.
                let body = (!it.declares_host_import()).then(|| self.lower_opt_expr(it.body()));
                self.alloc_expr(
                    ExprData::FnLiteral {
                        is_const,
                        params,
                        ret_type,
                        body,
                    },
                    it.syntax(),
                )
            }
        }
    }

    fn lower_pat(&mut self, pat: ast::Pat) -> PatId {
        match pat {
            ast::Pat::WildcardPat(it) => self.alloc_pat(PatData::Wildcard, it.syntax()),
            // Reserved syntax (validation rejects it): nothing to match.
            ast::Pat::RestPat(it) => self.alloc_pat(PatData::Missing, it.syntax()),
            // Only the character literal has pattern semantics; the other
            // literal kinds parse into this node so `validation` can name
            // them, and lower as broken (their diagnostic is already
            // written, and there is nothing to match on).
            ast::Pat::LiteralPat(it) => {
                let value = match it.literal().and_then(|lit| lit.kind()) {
                    Some(ast::LiteralKind::Char(token)) => syntax::char_literal_value(token.text()),
                    _ => None,
                };
                let data = value.map_or(PatData::Missing, PatData::Char);
                self.alloc_pat(data, it.syntax())
            }
            ast::Pat::BindPat(it) => {
                let binding = self.alloc_binding(it.name(), None, false, it.syntax());
                self.alloc_pat(PatData::Bind(binding), it.syntax())
            }
            ast::Pat::VariantPat(it) => {
                // The three spellings — qualified `Shape::Circle`, elided
                // `::Circle`, retired bare `Circle(...)` — all funnel
                // through the same helpers: `enum_name_ref` is present only
                // for the qualified spelling, and a missing variant segment
                // stays empty (the parse error covers it), mirroring
                // `ExprData::VariantPath`.
                let enum_name = it.enum_name_ref().map(|n| n.text());
                let variant = it.variant_name_ref().map(|n| n.text()).unwrap_or_default();
                let bindings = it
                    .bindings()
                    .map(|name| {
                        let node = name.syntax().clone();
                        self.alloc_binding(Some(name), None, false, &node)
                    })
                    .collect();
                let rest = it.rest_pat().is_some();
                self.alloc_pat(
                    PatData::Variant {
                        enum_name,
                        variant,
                        bindings,
                        rest,
                    },
                    it.syntax(),
                )
            }
            // `let`/parameter-only shapes; `match_pattern`'s grammar never
            // produces them, but a defensive fallback keeps this function
            // total if that ever changes.
            ast::Pat::RecordPat(_) | ast::Pat::NewtypePat(_) => {
                self.lower_binding_pattern(pat, None, false)
            }
        }
    }

    /// A `let`/parameter pattern — construction's mirror image. Unlike
    /// [`Self::lower_pat`] (match arms), a bare name/hole always lowers as
    /// [`PatData::Bind`] (never [`PatData::Wildcard`]): a `let`/parameter
    /// hole has always allocated an (unnamed) value slot, and this keeps
    /// doing exactly that. `type_ref`/`mutable` apply only when `pat` is
    /// directly a bare name — nested patterns (a `Newtype`'s inner, a
    /// `Record`'s fields) carry no type ascription in v1, and a field's own
    /// `mut` is read straight off its syntax instead.
    fn lower_binding_pattern(
        &mut self,
        pat: ast::Pat,
        type_ref: Option<TypeRef>,
        mutable: bool,
    ) -> PatId {
        match pat {
            ast::Pat::BindPat(it) => {
                let binding = self.alloc_binding(it.name(), type_ref, mutable, it.syntax());
                self.alloc_pat(PatData::Bind(binding), it.syntax())
            }
            ast::Pat::RecordPat(it) => {
                let fields = it
                    .fields()
                    .map(|field| {
                        let name = field.field_name().map(|n| n.text()).unwrap_or_default();
                        let binding = self.alloc_binding(
                            field.bound_name(),
                            None,
                            field.is_mut(),
                            field.syntax(),
                        );
                        RecordPatField {
                            field: name,
                            binding,
                        }
                    })
                    .collect();
                let rest = it.rest_pat().is_some();
                self.alloc_pat(PatData::Record { fields, rest }, it.syntax())
            }
            ast::Pat::NewtypePat(it) => {
                let type_name = it.name_ref().map(|n| n.text()).unwrap_or_default();
                let inner = match it.pat() {
                    Some(inner) => self.lower_binding_pattern(inner, None, false),
                    None => self.pats.alloc(PatData::Missing),
                };
                self.alloc_pat(PatData::Newtype { type_name, inner }, it.syntax())
            }
            // Match-only shapes; `binding_pattern`'s grammar never produces
            // them. Broken/defensive fallback: nothing to bind.
            ast::Pat::WildcardPat(it) => self.alloc_pat(PatData::Missing, it.syntax()),
            ast::Pat::VariantPat(it) => self.alloc_pat(PatData::Missing, it.syntax()),
            ast::Pat::RestPat(it) => self.alloc_pat(PatData::Missing, it.syntax()),
            ast::Pat::LiteralPat(it) => self.alloc_pat(PatData::Missing, it.syntax()),
        }
    }

    fn lower_block(&mut self, block: ast::BlockExpr) -> ExprId {
        let stmts = block
            .statements()
            .map(|stmt| match stmt {
                ast::Stmt::LetStmt(it) => {
                    let init = self.lower_opt_expr(it.initializer());
                    let type_ref = TypeRef::from_opt_ast(it.ty());
                    let pat = match it.pat() {
                        Some(pat) => self.lower_binding_pattern(pat, type_ref.clone(), it.is_mut()),
                        None => self.pats.alloc(PatData::Missing),
                    };
                    if let Some(ty) = it.ty()
                        && let PatData::Bind(binding) = &self.pats[pat]
                    {
                        let binding = *binding;
                        self.source_map
                            .binding_annotation_back
                            .insert(binding, SyntaxNodePtr::new(ty.syntax()));
                    }
                    Stmt::Let {
                        pat,
                        type_ref,
                        init,
                    }
                }
                ast::Stmt::AssignStmt(it) => {
                    let target = self.lower_opt_expr(it.lhs());
                    let value = self.lower_opt_expr(it.rhs());
                    Stmt::Assign { target, value }
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
        mutable: bool,
        fallback_node: &syntax::SyntaxNode,
    ) -> BindingId {
        // A hole binds no name, so there is nothing `mut` could ever make
        // assignable — `mutable` stays `false` regardless of the written
        // keyword (validation flags a written `mut` there separately).
        let is_hole = name.as_ref().is_some_and(|n| n.is_hole());
        let id = self.bindings.alloc(BindingData {
            name: name.as_ref().map(|n| n.text()).unwrap_or_default(),
            type_ref,
            mutable: mutable && !is_hole,
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

/// The one place a string literal's token text becomes its value: every
/// consumer (inference, MIR consts, const args, the machine) sees the cooked
/// string. Escape spelling comes from [`syntax::unescape_char`], so the
/// lexer's `unknown escape sequence` diagnostic and this decoder can never
/// disagree about what is an escape.
///
/// Cooking changes the string's *length*, never anyone's source ranges:
/// diagnostics are anchored on syntax nodes, and the raw token text stays
/// in the green tree untouched.
pub(crate) fn unescape(raw: &str) -> String {
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
        // An unknown escape already carries a syntax error; keeping the
        // escaped character is recovery, so the rest of the literal still
        // reads sensibly in the editor. A trailing lone backslash yields
        // `None` here — also already reported, and it contributes nothing.
        if let Some(other) = chars.next() {
            out.push(syntax::unescape_char(other).unwrap_or(other));
        }
    }
    out
}
