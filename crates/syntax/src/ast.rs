//! Typed views over the untyped syntax tree. Every accessor returns `Option`:
//! a node produced from broken code may be missing any child.

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
ast_node!(Name: NAME);
ast_node!(NameRef: NAME_REF);
ast_node!(FnLiteral: FN_LITERAL);
ast_node!(ParamList: PARAM_LIST);
ast_node!(Param: PARAM);
ast_node!(RetType: RET_TYPE);
ast_node!(BlockExpr: BLOCK_EXPR);
ast_node!(LetStmt: LET_STMT);
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

ast_enum!(Expr: FnLiteral, CallExpr, PathExpr, Literal, BlockExpr, ParenExpr, BinExpr, IfExpr);
ast_enum!(Type: FnType, UnitType, NeverType, PathType, RefType, HoleType);
ast_enum!(Stmt: LetStmt, ExprStmt);

impl SourceFile {
    pub fn items(&self) -> impl Iterator<Item = StaticItem> + use<> {
        children(&self.syntax)
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
    pub fn is_const(&self) -> bool {
        token(&self.syntax, CONST_KW).is_some()
    }
}

impl Name {
    pub fn text(&self) -> String {
        token(&self.syntax, IDENT)
            .map(|it| it.text().to_owned())
            .unwrap_or_default()
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

impl LetStmt {
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
    pub fn op(&self) -> Option<BinOp> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|it| {
                let op = match it.kind() {
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
            })
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
