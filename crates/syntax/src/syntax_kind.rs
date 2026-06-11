//! The flat set of token and node kinds for the Must syntax tree.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // Parser-internal; never appears in a finished tree.
    TOMBSTONE,
    EOF,

    // Tokens
    WHITESPACE,
    COMMENT,
    IDENT,
    INT_NUMBER,
    STRING,
    LIFETIME_IDENT,
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    COLON,
    SEMICOLON,
    COMMA,
    EQ,
    THIN_ARROW,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    AMP,
    BANG,
    FN_KW,
    STATIC_KW,
    CONST_KW,
    LET_KW,
    ERROR_TOKEN,

    // Nodes
    SOURCE_FILE,
    STATIC_ITEM,
    NAME,
    NAME_REF,
    FN_LITERAL,
    PARAM_LIST,
    PARAM,
    RET_TYPE,
    BLOCK_EXPR,
    LET_STMT,
    EXPR_STMT,
    CALL_EXPR,
    ARG_LIST,
    PAREN_EXPR,
    BIN_EXPR,
    LITERAL,
    PATH_EXPR,
    FN_TYPE,
    UNIT_TYPE,
    NEVER_TYPE,
    PATH_TYPE,
    REF_TYPE,
    ERROR,
}

use SyntaxKind::*;

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, WHITESPACE | COMMENT)
    }

    pub fn from_keyword(ident: &str) -> Option<SyntaxKind> {
        let kw = match ident {
            "fn" => FN_KW,
            "static" => STATIC_KW,
            "const" => CONST_KW,
            "let" => LET_KW,
            _ => return None,
        };
        Some(kw)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

impl From<rowan::SyntaxKind> for SyntaxKind {
    fn from(raw: rowan::SyntaxKind) -> Self {
        assert!(raw.0 <= ERROR as u16);
        // SAFETY: SyntaxKind is repr(u16), fieldless, and the value is in range.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }
}
