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

ast_enum!(
    Expr: FnLiteral,
    CallExpr,
    PathExpr,
    Literal,
    BlockExpr,
    ConstBlockExpr,
    ParenExpr,
    BinExpr,
    IfExpr,
    RecordExpr,
    FieldExpr
);
ast_enum!(
    Type: FnType,
    UnitType,
    NeverType,
    PathType,
    RefType,
    HoleType,
    RecordType
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
    pub fn name(&self) -> Option<Name> {
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
    pub fn name(&self) -> Option<Name> {
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
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
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
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.syntax)
    }
}

impl RefType {
    pub fn ty(&self) -> Option<Type> {
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
}

impl RecordExpr {
    pub fn fields(&self) -> impl Iterator<Item = RecordExprField> + use<> {
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
