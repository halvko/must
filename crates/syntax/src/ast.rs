//! Typed views over the untyped syntax tree. Most accessors returns `Option`:
//! a node produced from broken code may be missing ~any child.

use crate::SyntaxKind::{self, *};
use crate::{MustLanguage, SyntaxNode, SyntaxToken};

pub use rowan::ast::{AstNode, AstPtr};

fn child<N: AstNode<Language = MustLanguage>>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

fn children<N: AstNode<Language = MustLanguage>>(
    parent: &SyntaxNode,
) -> impl Iterator<Item = N> + use<N> {
    parent.children().filter_map(N::cast)
}

fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == kind)
}

macro_rules! ast_node {
    ($(#[$attr:meta])* $name:ident: $kind:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: SyntaxNode,
        }
        impl AstNode for $name {
            type Language = MustLanguage;
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(syntax: SyntaxNode) -> Option<Self> {
                Self::can_cast(syntax.kind()).then(|| Self { syntax })
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

macro_rules! ast_enum {
    ($(#[$attr:meta])* $name:ident: $($variant:ident),+ $(,)?) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($variant),)+
        }
        impl AstNode for $name {
            type Language = MustLanguage;
            fn can_cast(kind: SyntaxKind) -> bool {
                $($variant::can_cast(kind))||+
            }
            fn cast(syntax: SyntaxNode) -> Option<Self> {
                $(if $variant::can_cast(syntax.kind()) {
                    return Some($name::$variant($variant { syntax }));
                })+
                None
            }
            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $($name::$variant(it) => &it.syntax,)+
                }
            }
        }
    };
}

ast_node!(SourceFile: SOURCE_FILE);
ast_node!(
    /// `static name: Type = expr;` or `const name: Type = expr;`
    StaticItem: STATIC_ITEM
);
ast_node!(
    /// `type Name = struct { ... };` — a newtype declaration. The RHS is an
    /// ordinary expression grammar-wise; hir restricts it to a `struct`
    /// literal with a diagnostic (keeping the parse resilient).
    TypeItem: TYPE_ITEM
);
ast_node!(Name: NAME);
ast_node!(NameRef: NAME_REF);
ast_node!(FnLiteral: FN_LITERAL);
ast_node!(ParamList: PARAM_LIST);
ast_node!(Param: PARAM);
ast_node!(RetType: RET_TYPE);
ast_node!(BlockExpr: BLOCK_EXPR);
ast_node!(
    /// `const { ... }`, a const block in expression position.
    ConstBlockExpr: CONST_BLOCK_EXPR
);
ast_node!(LetStmt: LET_STMT);
ast_node!(
    /// `lhs = rhs;`. Superset-parsed: `lhs` may be any expression (validation
    /// rejects anything but a plain variable).
    AssignStmt: ASSIGN_STMT
);
ast_node!(ExprStmt: EXPR_STMT);
ast_node!(CallExpr: CALL_EXPR);
ast_node!(ArgList: ARG_LIST);
ast_node!(ParenExpr: PAREN_EXPR);
ast_node!(BinExpr: BIN_EXPR);
ast_node!(IfExpr: IF_EXPR);
ast_node!(Literal: LITERAL);
ast_node!(PathExpr: PATH_EXPR);
ast_node!(FnType: FN_TYPE);
ast_node!(UnitType: UNIT_TYPE);
ast_node!(NeverType: NEVER_TYPE);
ast_node!(PathType: PATH_TYPE);
ast_node!(RefType: REF_TYPE);
ast_node!(HoleType: HOLE_TYPE);
ast_node!(
    /// `{ x: T, y: U }`, a structural record type.
    RecordType: RECORD_TYPE
);
ast_node!(RecordTypeField: RECORD_TYPE_FIELD);
ast_node!(
    /// `{ x: e, y }`, a record literal.
    RecordExpr: RECORD_EXPR
);
ast_node!(RecordExprField: RECORD_EXPR_FIELD);
ast_node!(
    /// `receiver.field`, a field access.
    FieldExpr: FIELD_EXPR
);
ast_node!(
    /// `enum { Circle(usize), Point }`, an enum literal. Grammar-wise an
    /// expression; validation restricts it to a `type` declaration's RHS.
    EnumExpr: ENUM_EXPR
);
ast_node!(
    /// One variant of an enum literal: a name plus zero or more positional
    /// payload types.
    EnumVariant: ENUM_VARIANT
);
ast_node!(
    /// `match scrutinee { arms }`.
    MatchExpr: MATCH_EXPR
);
ast_node!(
    /// One arm: `pattern => expr` with an optional trailing `,`.
    MatchArm: MATCH_ARM
);
ast_node!(
    /// `Circle(r)` / `Shape::Circle(r)` / bare `Shape::Circle` — a variant
    /// pattern. The variant names are references (they resolve against an
    /// enum declaration); the payload bindings are declarations.
    VariantPat: VARIANT_PAT
);
ast_node!(
    /// `_` as a whole pattern.
    WildcardPat: WILDCARD_PAT
);
ast_node!(
    /// A bare name as a whole pattern: a binding — unless hir reinterprets
    /// it as a payload-less variant of the scrutinee's enum.
    BindPat: BIND_PAT
);
ast_node!(
    /// `..` in pattern position — legal inside a [`RecordPat`] ("don't bind
    /// the rest"); reserved (validation rejects it) everywhere else a
    /// pattern can appear.
    RestPat: REST_PAT
);
ast_node!(
    /// `struct { x, y as z, mut w, .. }` — a record-destructuring pattern,
    /// construction's mirror image. Only legal as a whole `let`/parameter
    /// pattern (or nested one level inside a [`NewtypePat`]) — not inside a
    /// variant pattern's payload.
    RecordPat: RECORD_PAT
);
ast_node!(
    /// One field of a [`RecordPat`]: `mut? name (as name)?`.
    RecordPatField: RECORD_PAT_FIELD
);
ast_node!(
    /// `Name(pattern)` — unwraps a newtype and destructures its underlying
    /// shape, construction's mirror image (`Name(struct { ... })`).
    NewtypePat: NEWTYPE_PAT
);
ast_node!(
    /// `loop { ... }` — an infinite loop; its value is carried by `break`.
    LoopExpr: LOOP_EXPR
);
ast_node!(
    /// `break` with an optional value, exiting the enclosing `loop`.
    BreakExpr: BREAK_EXPR
);
ast_node!(
    /// `continue`, restarting the enclosing `loop`'s body.
    ContinueExpr: CONTINUE_EXPR
);
ast_node!(
    /// `return` with an optional value, exiting the enclosing body — the
    /// nearest `fn` literal or `const` block.
    ReturnExpr: RETURN_EXPR
);
ast_node!(
    /// `::<T, const V: usize>` — a generic fn literal's binder list.
    GenericParamList: GENERIC_PARAM_LIST
);
ast_node!(
    /// A bare-name generic parameter (`T`).
    TypeParam: TYPE_PARAM
);
ast_node!(
    /// `const name: Type` — a const generic parameter.
    ConstParam: CONST_PARAM
);
ast_node!(
    /// `::<usize, 42>` — a turbofish argument list at a call site or type
    /// mention.
    GenericArgList: GENERIC_ARG_LIST
);
ast_node!(
    /// A type argument in a [`GenericArgList`] — any type, including the
    /// `_` hole.
    TypeArg: TYPE_ARG
);
ast_node!(
    /// A const argument in a [`GenericArgList`]: a bare literal (`42`,
    /// `"x"`, `true`, `false`) or a `const`-prefixed expression.
    ConstArg: CONST_ARG
);
ast_node!(
    /// A NAMED argument in a [`GenericArgList`] — `Self = Type` (TR01's one
    /// nameable argument). The grammar accepts any name; which names are
    /// nameable is a semantic question.
    NamedArg: NAMED_ARG
);
ast_node!(
    /// `::<usize>` on a path's SECOND segment (`Pair::first::<usize>`,
    /// `Shape::Circle::<usize>`) — that segment's OWN generic arguments,
    /// wrapping the `::` and the [`GenericArgList`].
    ///
    /// The node exists so ownership is structural: the OWNER's turbofish is
    /// a direct `GENERIC_ARG_LIST` child of the [`PathExpr`], a segment's
    /// own list hangs in here, and no consumer can read one as the other.
    /// Reaching it goes through [`PathExpr::member_generic_arg_list`].
    MemberGenericArgs: MEMBER_GENERIC_ARGS
);

ast_node!(
    /// `@a` (with optional outlives bounds `@b: @a + @c`) in a binder list —
    /// the THIRD generic parameter kind. Regions ride the same binder slot
    /// as types and consts but are a DISTINGUISHED kind downstream: erased,
    /// never reaching instance keys or MIR identity.
    RegionParam: REGION_PARAM
);
ast_node!(
    /// `@a`, the wildcard `@_`, or the join `@a + @b` in a turbofish — one
    /// region argument. A join names several regions at once and reads as
    /// conjunction ("outlived by all of them"), exactly as `+` does in bound
    /// composition.
    RegionArg: REGION_ARG
);

ast_enum!(
    /// One generic parameter: a region, a bare type name, or a `const`
    /// value binder.
    GenericParam: RegionParam, TypeParam, ConstParam
);
ast_enum!(
    /// One turbofish argument: a region, a type (including `_`), a const
    /// value, or a named argument (`Self = Type`).
    GenericArg: RegionArg, TypeArg, ConstArg, NamedArg
);

ast_node!(
    /// `x.&raw` / `x.&raw mut` — postfix address-of, the dual of `.*`:
    /// takes the address of a place, producing a raw pointer, and chains
    /// like field access. hir restricts the receiver to places (a variable,
    /// a chain of its fields and elements, a `static`/`const` item, or a
    /// chain rooted in a deref). The retired prefix spelling `&raw x`
    /// superset-parses into this node too (with a migration diagnostic).
    AddrOfExpr: ADDR_OF_EXPR
);
ast_node!(
    /// `p.*` — postfix deref of a raw pointer; chains like field access.
    DerefExpr: DEREF_EXPR
);
ast_node!(
    /// `x.&` / `x.&mut` — a postfix safe borrow, the dual of `.*`.
    BorrowExpr: BORROW_EXPR
);
ast_node!(
    /// `T.&` / `T.&mut` — a postfix safe reference type.
    BorrowType: BORROW_TYPE
);
ast_node!(
    /// `unsafe { ... }` — a checker region: raw-pointer derefs are legal
    /// inside. `unsafe fn` superset-parses into this node too (the child is
    /// then a [`FnLiteral`]); validation rejects it as reserved.
    UnsafeBlockExpr: UNSAFE_BLOCK_EXPR
);
ast_node!(
    /// `T.&raw` / `T.&raw mut` — a postfix raw pointer type, mirroring the
    /// expression-side postfix address-of. The retired prefix spelling
    /// `&raw T` superset-parses into this node too (with a migration
    /// diagnostic).
    RawPtrType: RAW_PTR_TYPE
);
ast_node!(
    /// `[T; N]` — a fixed-size array type. The length is a [`ConstArg`],
    /// the same node a turbofish's const argument uses.
    ArrayType: ARRAY_TYPE
);
ast_node!(
    /// `[e1, e2]` (length = element count) or `[e; N]` (the repeat form —
    /// a `;` separates the element from the count).
    ArrayExpr: ARRAY_EXPR
);
ast_node!(
    /// `base[index]` — an index read (or, as an assignment target, an
    /// element write). Chains like field access.
    IndexExpr: INDEX_EXPR
);
ast_node!(
    /// `-x` — unary minus on a number.
    NegExpr: NEG_EXPR
);
ast_node!(
    /// `with ::<binders>? clause,* { element* }` — one attachment group
    /// trailing a `type` declaration (sealed trait-syntax grammar, TR01).
    /// Only the plain form (`with { ... }`) is semantically supported;
    /// binder/clause groups parse and are reserved.
    WithGroup: WITH_GROUP
);
ast_node!(
    /// One group clause: `T: Bound + Bound` (constrain) or `T = usize`
    /// (pin). Reserved.
    WithClause: WITH_CLAUSE
);
ast_node!(
    /// `impl ⟨head⟩ { member* }` or the body-elided `impl ⟨head⟩;`.
    /// `impl Self { ... }` (inherent members) and a bare trait/type name
    /// (a trait impl, at the trait's or the self-type's head) are
    /// supported; modifier heads, markers and generic-owner impls are
    /// parse-and-reserve.
    ImplElement: IMPL_ELEMENT
);
ast_node!(
    /// `unsafe ⟨element⟩` / `unsafe { element* }` — a modifier head
    /// (TR01). Reserved.
    UnsafeElement: UNSAFE_ELEMENT
);
ast_node!(
    /// `for ⟨Type⟩ ⟨element⟩` / `for ⟨Type⟩ { element* }` — the covered
    /// impl head (TR01). Reserved.
    ForElement: FOR_ELEMENT
);
ast_node!(
    /// One member of an impl body: `name = fn(...) -> R { ... };`
    /// (equals-defines), `name: fn(...);` (colon-declares — a trait's
    /// requirement form; rejected in impl bodies), and the reserved
    /// `type Item = T;` / `const N: usize;` spellings (distinguished by
    /// their leading keyword token).
    Member: MEMBER
);
ast_node!(
    /// `trait Name = requires { ... };` — a trait declaration (sealed
    /// grammar, TR01). The RHS is a [`RequiresDef`] or a reserved
    /// [`TraitAlias`].
    TraitItem: TRAIT_ITEM
);
ast_node!(
    /// `unsafe? requires ::<binders>? clause,* { member* }` — the trait
    /// constructor. Only the plain non-generic form is supported; binders,
    /// clauses and `unsafe` are parse-and-reserve.
    RequiresDef: REQUIRES_DEF
);
ast_node!(
    /// One supertrait clause: `Self: Bound + Bound` (reserved).
    RequiresClause: REQUIRES_CLAUSE
);
ast_node!(
    /// A trait-alias RHS: `Eq + PartialOrd` (reserved).
    TraitAlias: TRAIT_ALIAS
);

ast_enum!(
    Expr: FnLiteral,
    CallExpr,
    PathExpr,
    Literal,
    BlockExpr,
    ConstBlockExpr,
    UnsafeBlockExpr,
    ParenExpr,
    BinExpr,
    IfExpr,
    RecordExpr,
    FieldExpr,
    AddrOfExpr,
    DerefExpr,
    BorrowExpr,
    EnumExpr,
    MatchExpr,
    LoopExpr,
    BreakExpr,
    ContinueExpr,
    ReturnExpr,
    ArrayExpr,
    IndexExpr,
    NegExpr
);
ast_enum!(
    /// A pattern: a match-arm pattern (`VariantPat`/`WildcardPat`/`RestPat`)
    /// or a `let`/parameter binding pattern (`BindPat`/`RecordPat`/
    /// `NewtypePat`) — the grammar keeps the two vocabularies mostly
    /// disjoint (see `crate::grammar`'s `match_pattern` vs
    /// `binding_pattern`), but both lower through the same `Pat` arena.
    Pat: VariantPat,
    WildcardPat,
    BindPat,
    RestPat,
    RecordPat,
    NewtypePat
);
ast_enum!(
    Type: FnType,
    UnitType,
    NeverType,
    PathType,
    RefType,
    RawPtrType,
    BorrowType,
    HoleType,
    RecordType,
    ArrayType
);
ast_enum!(Stmt: LetStmt, AssignStmt, ExprStmt);
ast_enum!(
    /// Any top-level item.
    Item: StaticItem,
    TypeItem,
    TraitItem
);

impl SourceFile {
    pub fn items(&self) -> impl Iterator<Item = Item> + use<> {
        children(&self.syntax)
    }
}

impl Item {
    pub fn name(&self) -> Option<Name> {
        child(self.syntax())
    }
    pub fn body(&self) -> Option<Expr> {
        child(self.syntax())
    }
    /// The item's type annotation. Only `static`/`const` items have one; a
    /// `type`/`trait` item's annotation is superset-parsed junk (validation
    /// rejects it), so it is never surfaced here.
    pub fn ty(&self) -> Option<Type> {
        match self {
            Item::StaticItem(it) => it.ty(),
            Item::TypeItem(_) | Item::TraitItem(_) => None,
        }
    }

    /// The attachment `with`-chain trailing the item, in source order —
    /// meaningful on `type` and `trait` declarations (superset-parsed and
    /// rejected on `static`/`const`).
    pub fn with_groups(&self) -> impl Iterator<Item = WithGroup> + use<> {
        children(self.syntax())
    }
}

impl StaticItem {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    /// Whether the item is introduced by `const` (as opposed to `static`).
    /// Only the item's own leading keyword counts — a `const` starting the
    /// initializer (`static f = const fn ...`, `static x = const { ... }`)
    /// belongs to the fn literal / const block, not to the item.
    pub fn is_const(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| !it.kind().is_trivia())
            .is_some_and(|it| it.kind() == CONST_KW)
    }
}

ast_enum!(
    /// One element of a [`WithGroup`]: an `impl` element or a modifier
    /// head (`unsafe`/`for`) wrapping further elements.
    Element: ImplElement,
    UnsafeElement,
    ForElement
);

impl WithGroup {
    pub fn with_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, WITH_KW)
    }
    /// The `::<...>` binder list, when the group declares fresh binders
    /// (reserved).
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
    /// The constrain/pin clauses (reserved).
    pub fn clauses(&self) -> impl Iterator<Item = WithClause> + use<> {
        children(&self.syntax)
    }
    /// Whether this is the plain, supported form: no binders, no
    /// clauses.
    pub fn is_plain(&self) -> bool {
        self.generic_param_list().is_none() && self.clauses().next().is_none()
    }
    pub fn elements(&self) -> impl Iterator<Item = Element> + use<> {
        children(&self.syntax)
    }
}

impl WithClause {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    /// The pin's `=` token (`T = usize`); a constrain clause has none.
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EQ)
    }
    /// The region head's sigil (`@a` in `@a: @b + @c`); a type-headed
    /// clause (`T: ...` / `T = ...`) has none.
    pub fn region_ident_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, REGION_IDENT)
    }
}

impl ImplElement {
    pub fn impl_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, IMPL_KW)
    }
    /// The head type: `Self`, a trait name, a marker, an implementer.
    pub fn head(&self) -> Option<Type> {
        child(&self.syntax)
    }
    /// Whether the head is the bare `Self` — the inherent form.
    pub fn is_self_head(&self) -> bool {
        matches!(
            &self.head(),
            Some(Type::PathType(path))
                if path.generic_arg_list().is_none()
                    && path.variant_name_ref().is_none()
                    && path.name_ref().is_some_and(|n| n.text() == "Self")
        )
    }
    pub fn members(&self) -> impl Iterator<Item = Member> + use<> {
        children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, L_BRACE)
    }
}

impl UnsafeElement {
    pub fn unsafe_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, UNSAFE_KW)
    }
}

impl ForElement {
    pub fn for_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, FOR_KW)
    }
}

impl Member {
    /// The leading keyword of the reserved associated-type spelling
    /// (`type Item = T;`).
    pub fn type_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, TYPE_KW)
    }
    /// The leading keyword of the reserved associated-const spelling
    /// (`const N: usize;`). Only a *leading* `const` counts — a `const fn`
    /// value's keyword belongs to the fn literal.
    pub fn const_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| !it.kind().is_trivia())
            .filter(|it| it.kind() == CONST_KW)
    }
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    /// The colon-declared type (`name: fn(...);`), when written.
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EQ)
    }
    /// The defining value (`name = fn(...) { ... };`), when written.
    pub fn value(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl TypeItem {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    /// The attachment `with`-chain trailing the declaration, in source
    /// order.
    pub fn with_groups(&self) -> impl Iterator<Item = WithGroup> + use<> {
        children(&self.syntax)
    }
    /// The declaration's RHS — restricted to a `struct` literal by hir, but
    /// any expression parses (resilience).
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    /// A superset-parsed `: Type` annotation (validation rejects it).
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl Name {
    pub fn text(&self) -> String {
        token(&self.syntax, IDENT)
            .map(|it| it.text().to_owned())
            .unwrap_or_default()
    }

    /// Whether this name is the hole `_` rather than an identifier. A
    /// `Name` node always wraps exactly one of `IDENT` or `HOLE` (see
    /// `grammar::pattern`), so this is the complement of having text.
    pub fn is_hole(&self) -> bool {
        token(&self.syntax, HOLE).is_some()
    }
}

impl NameRef {
    pub fn text(&self) -> String {
        token(&self.syntax, IDENT)
            .map(|it| it.text().to_owned())
            .unwrap_or_default()
    }
}

impl FnLiteral {
    /// The `const` marker of a `const fn` literal, if present.
    pub fn const_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, CONST_KW)
    }
    pub fn is_const(&self) -> bool {
        self.const_token().is_some()
    }
    /// The generic binder (`fn::<T, const N: usize>`), when present; hir
    /// lowers it.
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.syntax)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.syntax)
    }
    /// The language requires a block, but the parser accepts any expression
    /// for resilience — validation flags non-block bodies.
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl GenericParamList {
    pub fn params(&self) -> impl Iterator<Item = GenericParam> + use<> {
        children(&self.syntax)
    }
}

impl TypeParam {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    /// The `: Bound + Bound` bounds, in source order (empty when unbounded).
    pub fn bounds(&self) -> impl Iterator<Item = Type> + use<> {
        children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON)
    }
}

impl TraitItem {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    /// The `requires` constructor, when the RHS is one.
    pub fn requires_def(&self) -> Option<RequiresDef> {
        child(&self.syntax)
    }
    /// The reserved alias RHS (`Eq + PartialOrd`), when the RHS is one.
    pub fn trait_alias(&self) -> Option<TraitAlias> {
        child(&self.syntax)
    }
    /// The attachment `with`-chain trailing the declaration, in source
    /// order — the trait-side impl home (TR01).
    pub fn with_groups(&self) -> impl Iterator<Item = WithGroup> + use<> {
        children(&self.syntax)
    }
    /// A superset-parsed `: Type` annotation (validation rejects it).
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl RequiresDef {
    pub fn requires_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, REQUIRES_KW)
    }
    /// The reserved `unsafe` head (`unsafe requires ...`).
    pub fn unsafe_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, UNSAFE_KW)
    }
    /// The `::<...>` binder list (reserved: generic traits).
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
    /// The reserved supertrait clauses.
    pub fn clauses(&self) -> impl Iterator<Item = RequiresClause> + use<> {
        children(&self.syntax)
    }
    /// The requirement members, in source order.
    pub fn members(&self) -> impl Iterator<Item = Member> + use<> {
        children(&self.syntax)
    }
}

impl RequiresClause {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
}

impl TraitAlias {
    pub fn types(&self) -> impl Iterator<Item = Type> + use<> {
        children(&self.syntax)
    }
}

impl ConstParam {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = Param> + use<> {
        children(&self.syntax)
    }
}

impl Param {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    pub fn pat(&self) -> Option<Pat> {
        child(&self.syntax)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl RetType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl BlockExpr {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + use<> {
        children(&self.syntax)
    }
    /// The trailing expression, if the block ends without a `;`.
    pub fn tail_expr(&self) -> Option<Expr> {
        children::<Expr>(&self.syntax).last()
    }
}

impl ConstBlockExpr {
    pub fn const_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, CONST_KW)
    }
    pub fn block(&self) -> Option<BlockExpr> {
        child(&self.syntax)
    }
}

impl LetStmt {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    pub fn pat(&self) -> Option<Pat> {
        child(&self.syntax)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    pub fn initializer(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl AssignStmt {
    pub fn lhs(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }
    pub fn rhs(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.syntax)
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = Expr> + use<> {
        children(&self.syntax)
    }
}

impl ParenExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl IfExpr {
    pub fn if_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, IF_KW)
    }
    pub fn condition(&self) -> Option<Expr> {
        self.branches(false).next()
    }
    /// The language requires a block, but the parser accepts any expression
    /// for resilience — validation flags non-block branches.
    pub fn then_branch(&self) -> Option<Expr> {
        self.branches(false).nth(1)
    }
    /// A block, or another `IfExpr` for `else if` chains.
    pub fn else_branch(&self) -> Option<Expr> {
        self.branches(true).next()
    }

    /// Direct child expressions before (`false`) or after (`true`) the
    /// `else` keyword.
    fn branches(&self, after_else: bool) -> impl Iterator<Item = Expr> + use<> {
        let mut seen_else = false;
        self.syntax
            .children_with_tokens()
            .filter_map(move |element| match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == ELSE_KW {
                        seen_else = true;
                    }
                    None
                }
                rowan::NodeOrToken::Node(node) if seen_else == after_else => Expr::cast(node),
                rowan::NodeOrToken::Node(_) => None,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }
    pub fn rhs(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    PLUS | MINUS | STAR | SLASH | EQ2 | NEQ | L_ANGLE | LTEQ | R_ANGLE | GTEQ
                )
            })
    }
    pub fn op(&self) -> Option<BinOp> {
        let op = match self.op_token()?.kind() {
            PLUS => BinOp::Add,
            MINUS => BinOp::Sub,
            STAR => BinOp::Mul,
            SLASH => BinOp::Div,
            EQ2 => BinOp::Eq,
            NEQ => BinOp::Ne,
            L_ANGLE => BinOp::Lt,
            LTEQ => BinOp::Le,
            R_ANGLE => BinOp::Gt,
            GTEQ => BinOp::Ge,
            _ => return None,
        };
        Some(op)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralKind {
    Int(SyntaxToken),
    Str(SyntaxToken),
    Bool(bool),
}

impl Literal {
    pub fn kind(&self) -> Option<LiteralKind> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|it| match it.kind() {
                INT_NUMBER => Some(LiteralKind::Int(it)),
                STRING => Some(LiteralKind::Str(it)),
                TRUE_KW => Some(LiteralKind::Bool(true)),
                FALSE_KW => Some(LiteralKind::Bool(false)),
                _ => None,
            })
    }
}

impl PathExpr {
    /// The first (or only) segment.
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    /// The second segment of a `::` path (`Circle` in `Shape::Circle`), when
    /// present.
    pub fn variant_name_ref(&self) -> Option<NameRef> {
        children::<NameRef>(&self.syntax).nth(1)
    }
    pub fn colon2_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON2)
    }
    /// The OWNER's turbofish argument list — `f::<usize, 42>`, and the
    /// leading list of a qualified path (`Pair::<usize>::first`,
    /// `Display::<Self = Foo>::fmt`). Always the list applied to the FIRST
    /// segment: a second segment's own arguments live one level down,
    /// inside [`MemberGenericArgs`], so this direct-child lookup cannot
    /// reach them.
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.syntax)
    }
    /// The SECOND segment's own turbofish (`Pair::first::<usize>`), when
    /// present — the arguments applied to the member/variant itself rather
    /// than to its owner. Semantically reserved (hir states the
    /// reservation), which is exactly why it must not be confused with
    /// [`Self::generic_arg_list`].
    pub fn member_generic_arg_list(&self) -> Option<GenericArgList> {
        child::<MemberGenericArgs>(&self.syntax)?.generic_arg_list()
    }
}

impl MemberGenericArgs {
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.syntax)
    }
}

impl MatchExpr {
    pub fn match_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MATCH_KW)
    }
    /// The scrutinee. Arm bodies live inside `MATCH_ARM` nodes, so the only
    /// direct child expression is the scrutinee.
    pub fn scrutinee(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    pub fn arms(&self) -> impl Iterator<Item = MatchArm> + use<> {
        children(&self.syntax)
    }
}

impl MatchArm {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.syntax)
    }
    pub fn fat_arrow_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, FAT_ARROW)
    }
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl LoopExpr {
    pub fn loop_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, LOOP_KW)
    }
    /// The language requires a block, but the parser accepts any expression
    /// for resilience — validation flags non-block bodies.
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl BreakExpr {
    pub fn break_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, BREAK_KW)
    }
    /// The carried value; `None` for a bare `break` (which carries `()`).
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl ContinueExpr {
    pub fn continue_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, CONTINUE_KW)
    }
}

impl ReturnExpr {
    pub fn return_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, RETURN_KW)
    }
    /// The returned value; `None` for a bare `return` (which returns `()`).
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl VariantPat {
    /// The qualifying enum segment (`Shape` in `Shape::Circle(r)`) — only
    /// the fully-qualified spelling has one. `None` for the elided sigil
    /// spelling (`::Circle(r)`) and for the retired unqualified `Circle(r)`
    /// shape (still parses — validation.rs rejects it).
    ///
    /// Decides by token order, not fixed position: the sigil spelling has
    /// its single `NameRef` *after* the `COLON2`, the qualified spelling
    /// has one *before* it.
    pub fn enum_name_ref(&self) -> Option<NameRef> {
        let colon2 = self.colon2_token()?;
        let first = children::<NameRef>(&self.syntax).next()?;
        (first.syntax().text_range().end() <= colon2.text_range().start()).then_some(first)
    }
    /// The variant name segment: the second segment when qualified, the
    /// only one otherwise (elided `::Variant`, or the retired bare
    /// `Variant(...)` shape).
    pub fn variant_name_ref(&self) -> Option<NameRef> {
        if self.enum_name_ref().is_some() {
            children::<NameRef>(&self.syntax).nth(1)
        } else {
            children::<NameRef>(&self.syntax).next()
        }
    }
    pub fn colon2_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON2)
    }
    /// The positional payload bindings (holes included), in source order.
    pub fn bindings(&self) -> impl Iterator<Item = Name> + use<> {
        children(&self.syntax)
    }
    /// The reserved `..` rest marker, if written.
    pub fn rest_pat(&self) -> Option<RestPat> {
        child(&self.syntax)
    }
}

impl BindPat {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
}

impl RecordPat {
    /// The leading `struct` keyword.
    pub fn struct_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, STRUCT_KW)
    }
    pub fn fields(&self) -> impl Iterator<Item = RecordPatField> + use<> {
        children(&self.syntax)
    }
    /// The `..` rest marker, if written — "don't bind the remaining
    /// fields" (the scrutinee must still name every field structurally;
    /// `..` only means the pattern doesn't bind them).
    pub fn rest_pat(&self) -> Option<RestPat> {
        child(&self.syntax)
    }
}

impl RecordPatField {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    /// The field being destructured — the first `NAME` child. Doubles as
    /// the binding's own declaration site in the shorthand spelling (no
    /// `as`): the same token both selects the field and names the local.
    pub fn field_name(&self) -> Option<Name> {
        children::<Name>(&self.syntax).next()
    }
    pub fn as_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, AS_KW)
    }
    /// The renamed binding (`z` in `y as z`), when written.
    pub fn rename(&self) -> Option<Name> {
        children::<Name>(&self.syntax).nth(1)
    }
    /// The name actually bound: the rename if present, else the field name
    /// itself.
    pub fn bound_name(&self) -> Option<Name> {
        self.rename().or_else(|| self.field_name())
    }
}

impl NewtypePat {
    /// The newtype's name.
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    /// The pattern destructuring the newtype's underlying shape.
    pub fn pat(&self) -> Option<Pat> {
        child(&self.syntax)
    }
}

impl EnumExpr {
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> + use<> {
        children(&self.syntax)
    }
    /// The leading `enum` keyword.
    pub fn enum_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, ENUM_KW)
    }
    /// The `::<T, const N: usize>` binder list of a generic type
    /// declaration (`type Option = enum::<T> { ... }`), when present.
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
}

impl EnumVariant {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    /// The positional payload types, in source order. Empty for a
    /// payload-less variant (`Point` or `Point()` alike).
    pub fn payload_types(&self) -> impl Iterator<Item = Type> + use<> {
        children(&self.syntax)
    }
}

impl FnType {
    /// The parameter types. A written fn TYPE spells them bare
    /// (`fn(usize) -> R`) and they are direct children; the member
    /// declaration spelling names them (`fn(n: usize) -> R`), which puts
    /// them one level down under a `PARAM_LIST`. Reading through it keeps
    /// this accessor honest for both shapes instead of silently answering
    /// "no parameters" for the second.
    pub fn param_types(&self) -> impl Iterator<Item = Type> + use<> {
        let params: Vec<Type> = match child::<ParamList>(&self.syntax) {
            Some(list) => list.params().filter_map(|p| p.ty()).collect(),
            None => children(&self.syntax).collect(),
        };
        params.into_iter()
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.syntax)
    }
    /// The NAMED parameter list of a colon-declared member signature
    /// (`alloc: fn(n: usize, v: Self) -> R;`) — only that grammar path
    /// produces one; a plain fn TYPE has bare [`Self::param_types`].
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.syntax)
    }
    /// The `::<...>` binder of a colon-declared member signature.
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
    /// The reserved `unsafe` marker of a colon-declared member signature.
    pub fn unsafe_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, UNSAFE_KW)
    }
}

impl PathType {
    /// The first (or only) segment.
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    /// The second segment of a `::` path (`Circle` in `Shape::Circle`), when
    /// present.
    pub fn variant_name_ref(&self) -> Option<NameRef> {
        children::<NameRef>(&self.syntax).nth(1)
    }
    /// The turbofish argument list (`Pair::<usize>`), when present; hir
    /// lowers it and reports arity and position errors.
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.syntax)
    }
}

impl GenericArgList {
    pub fn args(&self) -> impl Iterator<Item = GenericArg> + use<> {
        children(&self.syntax)
    }
}

impl TypeArg {
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

/// Every `REGION_IDENT` token directly under `node`, in written order — the
/// one place region tokens are read off a tree. A [`RegionParam`]'s first is
/// the declared name and the rest are its outlives bounds; a [`RegionArg`]'s
/// are the members of its join.
fn region_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| it.kind() == REGION_IDENT)
}

impl RegionParam {
    /// The declared region's token (`@a`).
    pub fn region_token(&self) -> Option<SyntaxToken> {
        region_tokens(&self.syntax).next()
    }
    /// The declared region's name INCLUDING its sigil (`@a`) — the spelling
    /// users see in diagnostics and hovers.
    pub fn name(&self) -> Option<String> {
        self.region_token().map(|it| it.text().to_owned())
    }
    /// The regions this one must outlive (`@b: @a + @c` yields `@a`, `@c`).
    pub fn bounds(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        region_tokens(&self.syntax).skip(1)
    }
}

impl RegionArg {
    /// The regions this argument names — one token for `@a`/`@_`, several
    /// for the join `@a + @b`.
    pub fn regions(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        region_tokens(&self.syntax)
    }
}

impl NamedArg {
    /// The argument's written name (`Self`).
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EQ)
    }
    /// The named argument's value — a type (`Self = Point`).
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl ConstArg {
    /// The `const` keyword of the `const <expr>` spelling; absent for the
    /// bare-literal spelling (`42`, `"x"`, `true`, `false`).
    pub fn const_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, CONST_KW)
    }
    /// The const value: a `Literal` for the bare-literal spelling, any
    /// expression for the `const`-prefixed spelling (superset — semantics
    /// restrict this later).
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl RefType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    /// The leading `&` — the reservation diagnostic's anchor.
    pub fn amp_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, AMP)
    }
}

impl RawPtrType {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    /// The pointee type.
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
}

impl AddrOfExpr {
    pub fn raw_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, RAW_KW)
    }
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    /// The place whose address is taken.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl DerefExpr {
    /// The pointer expression being dereferenced.
    pub fn receiver(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, STAR)
    }
}

impl BorrowExpr {
    /// The place being borrowed.
    pub fn receiver(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    /// The borrow operator's OWN turbofish (`x.&mut::<@a>`), when written.
    /// A borrow node has no other generic-argument child, so this direct
    /// lookup is unambiguous.
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.syntax)
    }
}

impl BorrowType {
    /// The referent type.
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MUT_KW)
    }
    pub fn is_mut(&self) -> bool {
        self.mut_token().is_some()
    }
    /// The region turbofish (`T.&::<@a>`) — REQUIRED in signatures (no
    /// elision at launch); body-local annotations may write `@_`.
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.syntax)
    }
    /// The `.&` operator's `&` token — the anchor for "this borrow needs a
    /// region" diagnostics.
    pub fn amp_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, AMP)
    }
}

impl UnsafeBlockExpr {
    pub fn unsafe_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, UNSAFE_KW)
    }
    /// The body — a block when well-formed; a superset-parsed `fn` literal
    /// for the reserved `unsafe fn` spelling (validation rejects it).
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

impl RecordType {
    pub fn fields(&self) -> impl Iterator<Item = RecordTypeField> + use<> {
        children(&self.syntax)
    }
    /// The leading `struct` keyword.
    pub fn struct_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, STRUCT_KW)
    }
    /// The trailing open-record marker `...`, if present.
    pub fn dot3_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, DOT3)
    }
}

impl RecordTypeField {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    /// The reserved `pub` marker, if written — field visibility is not
    /// supported yet; validation rejects it.
    pub fn pub_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, PUB_KW)
    }
}

impl RecordExpr {
    pub fn fields(&self) -> impl Iterator<Item = RecordExprField> + use<> {
        children(&self.syntax)
    }
    /// The leading `struct` keyword.
    pub fn struct_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, STRUCT_KW)
    }
    /// The `::<T, const N: usize>` binder list of a generic type
    /// declaration (`type Pair = struct::<T> { ... }`), when present.
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child(&self.syntax)
    }
    /// The trailing open-record marker `...`, if present.
    pub fn dot3_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, DOT3)
    }
}

impl RecordExprField {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, COLON)
    }
    /// The `: Type` annotation, when written — a TYPE, uniformly (the
    /// equals-defines respell): the declared type in a `type` item's RHS,
    /// an ascription on a construction field.
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, EQ)
    }
    /// The defining value (`name = expr`), when written.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    /// Shorthand fields (`x` meaning `x = x`) have neither a colon nor an
    /// equals.
    pub fn is_shorthand(&self) -> bool {
        self.colon_token().is_none() && self.eq_token().is_none()
    }
    /// The reserved `pub` marker, if written — field visibility is not
    /// supported yet; validation rejects it.
    pub fn pub_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, PUB_KW)
    }
}

impl ArrayType {
    /// The element type.
    pub fn ty(&self) -> Option<Type> {
        child(&self.syntax)
    }
    /// The length const argument.
    pub fn len(&self) -> Option<ConstArg> {
        child(&self.syntax)
    }
}

impl ArrayExpr {
    /// The `;` of the repeat form, if this is one.
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SEMICOLON)
    }
    pub fn is_repeat(&self) -> bool {
        self.semicolon_token().is_some()
    }
    /// The list form's elements (all direct child expressions). For the
    /// repeat form use [`Self::repeat_parts`] instead.
    pub fn elements(&self) -> impl Iterator<Item = Expr> + use<> {
        children(&self.syntax)
    }
    /// The repeat form's `(element, count)` — `None` when this is the list
    /// form or the source is broken.
    pub fn repeat_parts(&self) -> Option<(Expr, Option<Expr>)> {
        if !self.is_repeat() {
            return None;
        }
        let mut exprs = children::<Expr>(&self.syntax);
        let element = exprs.next()?;
        Some((element, exprs.next()))
    }
}

impl IndexExpr {
    /// The expression being indexed.
    pub fn base(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }
    /// The index expression (between the brackets).
    pub fn index(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }
}

impl NegExpr {
    /// The negated operand.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    pub fn minus_token(&self) -> Option<SyntaxToken> {
        token(&self.syntax, MINUS)
    }
}

impl FieldExpr {
    /// The pre-dot expression whose field is being accessed.
    pub fn receiver(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    /// The field being accessed.
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
}
