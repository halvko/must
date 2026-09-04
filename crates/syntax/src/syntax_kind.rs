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
    L_BRACKET,
    R_BRACKET,
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
    RETURN_KW,
    TRUE_KW,
    FALSE_KW,
    AS_KW,
    PUB_KW,
    RAW_KW,
    UNSAFE_KW,
    WITH_KW,
    IMPL_KW,
    FOR_KW,
    TRAIT_KW,
    REQUIRES_KW,
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
    RETURN_EXPR,
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
    NAMED_ARG,
    RAW_PTR_TYPE,
    ADDR_OF_EXPR,
    DEREF_EXPR,
    BORROW_EXPR,
    BORROW_TYPE,
    UNSAFE_BLOCK_EXPR,
    ARRAY_TYPE,
    ARRAY_EXPR,
    INDEX_EXPR,
    NEG_EXPR,
    WITH_GROUP,
    WITH_CLAUSE,
    IMPL_ELEMENT,
    UNSAFE_ELEMENT,
    FOR_ELEMENT,
    MEMBER,
    TRAIT_ITEM,
    REQUIRES_DEF,
    REQUIRES_CLAUSE,
    TRAIT_ALIAS,
    ERROR,
}

use SyntaxKind::*;

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, WHITESPACE | COMMENT)
    }
}

/// Declares the language's keywords ONCE, and derives everything that has to
/// agree about them from that one list: [`KEYWORDS`] (the table itself),
/// [`SyntaxKind::from_keyword`] (what the lexer promotes an identifier to)
/// and [`SyntaxKind::is_keyword`] (what every consumer asks instead of
/// re-listing the kinds).
///
/// The point is that adding a keyword is a ONE-LINE change: the lexer starts
/// producing it, `is_keyword` starts answering `true` for it, and the ide
/// layer's highlighter — which classifies keywords by asking `is_keyword`,
/// never by enumerating kinds — colors it with no edit of its own. The
/// drift-guard tests in `crate::tests` pin exactly that: every `*_KW` kind
/// must appear here, every entry must round-trip, and the ide crate has a
/// matching test asserting the highlighter tags each of them as a keyword.
macro_rules! keywords {
    ($($text:literal => $kind:ident),* $(,)?) => {
        /// Every keyword of the language, paired with the token kind the
        /// lexer produces for it — the single source of truth (see the
        /// `keywords!` macro that generates this).
        pub const KEYWORDS: &[(&str, SyntaxKind)] = &[$(($text, $kind)),*];

        impl SyntaxKind {
            /// The keyword kind `ident` spells, if it spells one.
            pub fn from_keyword(ident: &str) -> Option<SyntaxKind> {
                match ident {
                    $($text => Some($kind),)*
                    _ => None,
                }
            }

            /// Whether this kind is one of the language's keyword tokens.
            /// Consumers (highlighting, completions, editor affordances) ask
            /// this rather than matching a list of their own, so a new
            /// keyword can never be silently forgotten by one of them.
            pub fn is_keyword(self) -> bool {
                matches!(self, $($kind)|*)
            }
        }
    };
}

keywords! {
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
    "return" => RETURN_KW,
    "true" => TRUE_KW,
    "false" => FALSE_KW,
    "as" => AS_KW,
    "pub" => PUB_KW,
    "raw" => RAW_KW,
    "unsafe" => UNSAFE_KW,
    "with" => WITH_KW,
    "impl" => IMPL_KW,
    "for" => FOR_KW,
    "trait" => TRAIT_KW,
    "requires" => REQUIRES_KW,
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
