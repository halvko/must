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

ast_enum!(
    /// One generic parameter: a bare type name or a `const` value binder.
    GenericParam: TypeParam, ConstParam
);
ast_enum!(
    /// One turbofish argument: a type (including `_`) or a const value.
    GenericArg: TypeArg, ConstArg
);

ast_node!(
    /// `&raw x` / `&raw mut x` — takes the address of a place, producing a
    /// raw pointer. The operand superset-parses as any postfix chain; hir
    /// restricts it to places (a variable, its fields, a `static`).
    AddrOfExpr: ADDR_OF_EXPR
);
ast_node!(
    /// `p.*` — postfix deref of a raw pointer; chains like field access.
    DerefExpr: DEREF_EXPR
);
ast_node!(
    /// `unsafe { ... }` — a checker region: raw-pointer derefs are legal
    /// inside. `unsafe fn` superset-parses into this node too (the child is
    /// then a [`FnLiteral`]); validation rejects it as reserved.
    UnsafeBlockExpr: UNSAFE_BLOCK_EXPR
);
ast_node!(
    /// `&raw T` / `&raw mut T` — a raw pointer type.
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
    EnumExpr,
    MatchExpr,
    LoopExpr,
    BreakExpr,
    ContinueExpr,
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
    HoleType,
    RecordType,
    ArrayType
);
ast_enum!(Stmt: LetStmt, AssignStmt, ExprStmt);
ast_enum!(
    /// Any top-level item.
    Item: StaticItem,
    TypeItem
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
    /// `type` item's annotation is superset-parsed junk (validation rejects
    /// it), so it is never surfaced here.
    pub fn ty(&self) -> Option<Type> {
        match self {
            Item::StaticItem(it) => it.ty(),
            Item::TypeItem(_) => None,
        }
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

impl TypeItem {
    pub fn name(&self) -> Option<Name> {
        child(&self.syntax)
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
    /// The turbofish argument list (`f::<usize, 42>`), when present; hir
    /// lowers it and reports arity and position errors.
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
    pub fn param_types(&self) -> impl Iterator<Item = Type> + use<> {
        children(&self.syntax)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.syntax)
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
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }
    /// Shorthand fields (`x` meaning `x: x`) have no colon.
    pub fn is_shorthand(&self) -> bool {
        self.colon_token().is_none()
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
