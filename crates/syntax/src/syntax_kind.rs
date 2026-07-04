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
    HOLE,
    INT_NUMBER,
    STRING,
    LIFETIME_IDENT,
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    COLON,
    COLON2,
    SEMICOLON,
    COMMA,
    DOT,
    DOT2,
    DOT3,
    EQ,
    EQ2,
    NEQ,
    L_ANGLE,
    R_ANGLE,
    LTEQ,
    GTEQ,
    THIN_ARROW,
    FAT_ARROW,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    AMP,
    BANG,
    FN_KW,
    STATIC_KW,
    CONST_KW,
    TYPE_KW,
    STRUCT_KW,
    ENUM_KW,
    LET_KW,
    MUT_KW,
    IF_KW,
    ELSE_KW,
    MATCH_KW,
    LOOP_KW,
    BREAK_KW,
    CONTINUE_KW,
    TRUE_KW,
    FALSE_KW,
    AS_KW,
    PUB_KW,
    ERROR_TOKEN,

    // Nodes
    SOURCE_FILE,
    STATIC_ITEM,
    TYPE_ITEM,
    NAME,
    NAME_REF,
    FN_LITERAL,
    PARAM_LIST,
    PARAM,
    RET_TYPE,
    BLOCK_EXPR,
    CONST_BLOCK_EXPR,
    LET_STMT,
    ASSIGN_STMT,
    EXPR_STMT,
    CALL_EXPR,
    ARG_LIST,
    PAREN_EXPR,
    BIN_EXPR,
    IF_EXPR,
    LITERAL,
    PATH_EXPR,
    FN_TYPE,
    UNIT_TYPE,
    NEVER_TYPE,
    PATH_TYPE,
    REF_TYPE,
    HOLE_TYPE,
    RECORD_TYPE,
    RECORD_TYPE_FIELD,
    RECORD_EXPR,
    RECORD_EXPR_FIELD,
    FIELD_EXPR,
    ENUM_EXPR,
    ENUM_VARIANT,
    MATCH_EXPR,
    MATCH_ARM,
    LOOP_EXPR,
    BREAK_EXPR,
    CONTINUE_EXPR,
    VARIANT_PAT,
    WILDCARD_PAT,
    BIND_PAT,
    REST_PAT,
    RECORD_PAT,
    RECORD_PAT_FIELD,
    NEWTYPE_PAT,
    GENERIC_PARAM_LIST,
    TYPE_PARAM,
    CONST_PARAM,
    GENERIC_ARG_LIST,
    TYPE_ARG,
    CONST_ARG,
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
            "type" => TYPE_KW,
            "struct" => STRUCT_KW,
            "enum" => ENUM_KW,
            "let" => LET_KW,
            "mut" => MUT_KW,
            "if" => IF_KW,
            "else" => ELSE_KW,
            "match" => MATCH_KW,
            "loop" => LOOP_KW,
            "break" => BREAK_KW,
            "continue" => CONTINUE_KW,
            "true" => TRUE_KW,
            "false" => FALSE_KW,
            "as" => AS_KW,
            "pub" => PUB_KW,
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
