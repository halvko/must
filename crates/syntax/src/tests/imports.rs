//! `use` items and their recovery.

use super::*;

// ---- `use` — imports from another file -----------------------------------

#[test]
fn use_imports_one_name() {
    check(
        "use sub::helpers::double;",
        expect![[r#"
        SOURCE_FILE@0..25
          USE_ITEM@0..25
            USE_KW@0..3 "use"
            WHITESPACE@3..4 " "
            NAME_REF@4..7
              IDENT@4..7 "sub"
            COLON2@7..9 "::"
            NAME_REF@9..16
              IDENT@9..16 "helpers"
            COLON2@16..18 "::"
            USE_NAME@18..24
              NAME_REF@18..24
                IDENT@18..24 "double"
            SEMICOLON@24..25 ";"
    "#]],
    );
}

#[test]
fn use_imports_a_braced_list_with_a_rename() {
    check(
        "use b::{double, Pair as P,};",
        expect![[r#"
            SOURCE_FILE@0..28
              USE_ITEM@0..28
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..14
                  NAME_REF@8..14
                    IDENT@8..14 "double"
                COMMA@14..15 ","
                WHITESPACE@15..16 " "
                USE_NAME@16..25
                  NAME_REF@16..20
                    IDENT@16..20 "Pair"
                  WHITESPACE@20..21 " "
                  AS_KW@21..23 "as"
                  WHITESPACE@23..24 " "
                  NAME@24..25
                    IDENT@24..25 "P"
                COMMA@25..26 ","
                R_BRACE@26..27 "}"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn use_without_a_file() {
    check(
        "use double;",
        expect![[r#"
            SOURCE_FILE@0..11
              USE_ITEM@0..11
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                USE_NAME@4..10
                  NAME_REF@4..10
                    IDENT@4..10 "double"
                SEMICOLON@10..11 ";"
            error 4..10: expected a file to import from, followed by `::`
        "#]],
    );
}

#[test]
fn use_without_a_name() {
    check(
        "use b::;",
        expect![[r#"
            SOURCE_FILE@0..8
              USE_ITEM@0..8
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                SEMICOLON@7..8 ";"
            error 7..8: expected a name to import, or `{` and a list of them
        "#]],
    );
}

#[test]
fn use_with_an_unclosed_list_before_an_item() {
    check(
        r#"
use b::{double static f = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..30
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..15
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                L_BRACE@8..9 "{"
                USE_NAME@9..15
                  NAME_REF@9..15
                    IDENT@9..15 "double"
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..29
                ITEM_HEAD@16..22
                  STATIC_KW@16..22 "static"
                WHITESPACE@22..23 " "
                NAME@23..24
                  IDENT@23..24 "f"
                WHITESPACE@24..25 " "
                EQ@25..26 "="
                WHITESPACE@26..27 " "
                LITERAL@27..28
                  INT_NUMBER@27..28 "1"
                SEMICOLON@28..29 ";"
              WHITESPACE@29..30 "\n"
            error 14..15: expected `}`
        "#]],
    );
}

#[test]
fn use_with_nothing_after_as() {
    check(
        "use b::{a as};",
        expect![[r#"
            SOURCE_FILE@0..14
              USE_ITEM@0..14
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..12
                  NAME_REF@8..9
                    IDENT@8..9 "a"
                  WHITESPACE@9..10 " "
                  AS_KW@10..12 "as"
                R_BRACE@12..13 "}"
                SEMICOLON@13..14 ";"
            error 12..13: expected a name after `as`
        "#]],
    );
}

#[test]
fn use_with_a_number_in_its_list() {
    check(
        "use b::{1};",
        expect![[r#"
            SOURCE_FILE@0..11
              USE_ITEM@0..11
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                ERROR@8..9
                  INT_NUMBER@8..9 "1"
                R_BRACE@9..10 "}"
                SEMICOLON@10..11 ";"
            error 8..9: expected a name to import
        "#]],
    );
}

#[test]
fn use_with_a_missing_comma() {
    check(
        "use b::{a c};",
        expect![[r#"
            SOURCE_FILE@0..13
              USE_ITEM@0..13
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..9
                  NAME_REF@8..9
                    IDENT@8..9 "a"
                WHITESPACE@9..10 " "
                USE_NAME@10..11
                  NAME_REF@10..11
                    IDENT@10..11 "c"
                R_BRACE@11..12 "}"
                SEMICOLON@12..13 ";"
            error 10..11: expected `,` or `}`
        "#]],
    );
}

#[test]
fn use_without_a_semicolon() {
    check(
        r#"
use b::a
static f = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..24
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..9
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                USE_NAME@8..9
                  NAME_REF@8..9
                    IDENT@8..9 "a"
              WHITESPACE@9..10 "\n"
              STATIC_ITEM@10..23
                ITEM_HEAD@10..16
                  STATIC_KW@10..16 "static"
                WHITESPACE@16..17 " "
                NAME@17..18
                  IDENT@17..18 "f"
                WHITESPACE@18..19 " "
                EQ@19..20 "="
                WHITESPACE@20..21 " "
                LITERAL@21..22
                  INT_NUMBER@21..22 "1"
                SEMICOLON@22..23 ";"
              WHITESPACE@23..24 "\n"
            error 8..9: expected `;`
        "#]],
    );
}

#[test]
fn use_after_a_broken_expression() {
    check(
        r#"
static f = fn { 1 +
use b::a;
"#,
        expect![[r#"
            SOURCE_FILE@0..31
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..20
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..20
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  BLOCK_EXPR@15..20
                    L_BRACE@15..16 "{"
                    WHITESPACE@16..17 " "
                    EXPR_STMT@17..20
                      BIN_EXPR@17..20
                        LITERAL@17..18
                          INT_NUMBER@17..18 "1"
                        WHITESPACE@18..19 " "
                        PLUS@19..20 "+"
              WHITESPACE@20..21 "\n"
              USE_ITEM@21..30
                USE_KW@21..24 "use"
                WHITESPACE@24..25 " "
                NAME_REF@25..26
                  IDENT@25..26 "b"
                COLON2@26..28 "::"
                USE_NAME@28..29
                  NAME_REF@28..29
                    IDENT@28..29 "a"
                SEMICOLON@29..30 ";"
              WHITESPACE@30..31 "\n"
            error 19..20: expected `;`
            error 21..24: expected an expression
        "#]],
    );
}

#[test]
fn use_after_an_unclosed_match() {
    check(
        r#"
static f = fn(s: usize) { match s {
use b::a;
"#,
        expect![[r#"
            SOURCE_FILE@0..47
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..36
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..36
                  FN_KW@12..14 "fn"
                  PARAM_LIST@14..24
                    L_PAREN@14..15 "("
                    PARAM@15..23
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "s"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      PATH_TYPE@18..23
                        NAME_REF@18..23
                          IDENT@18..23 "usize"
                    R_PAREN@23..24 ")"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..36
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..36
                      MATCH_EXPR@27..36
                        MATCH_KW@27..32 "match"
                        WHITESPACE@32..33 " "
                        PATH_EXPR@33..34
                          NAME_REF@33..34
                            IDENT@33..34 "s"
                        WHITESPACE@34..35 " "
                        L_BRACE@35..36 "{"
              WHITESPACE@36..37 "\n"
              USE_ITEM@37..46
                USE_KW@37..40 "use"
                WHITESPACE@40..41 " "
                NAME_REF@41..42
                  IDENT@41..42 "b"
                COLON2@42..44 "::"
                USE_NAME@44..45
                  NAME_REF@44..45
                    IDENT@44..45 "a"
                SEMICOLON@45..46 ";"
              WHITESPACE@46..47 "\n"
            error 35..36: expected `}`
        "#]],
    );
}

#[test]
fn use_inside_an_unclosed_list() {
    check(
        r#"
use b::{a,
use c::d;
"#,
        expect![[r#"
            SOURCE_FILE@0..22
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..11
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                L_BRACE@8..9 "{"
                USE_NAME@9..10
                  NAME_REF@9..10
                    IDENT@9..10 "a"
                COMMA@10..11 ","
              WHITESPACE@11..12 "\n"
              USE_ITEM@12..21
                USE_KW@12..15 "use"
                WHITESPACE@15..16 " "
                NAME_REF@16..17
                  IDENT@16..17 "c"
                COLON2@17..19 "::"
                USE_NAME@19..20
                  NAME_REF@19..20
                    IDENT@19..20 "d"
                SEMICOLON@20..21 ";"
              WHITESPACE@21..22 "\n"
            error 10..11: expected `}`
        "#]],
    );
}

#[test]
fn use_item_accessors() {
    let parse = crate::parse(
        r#"
use a::b::{x, y as z};
use a::x;
"#,
    );
    let shown: Vec<String> = parse
        .tree()
        .use_items()
        .map(|item| {
            let path: Vec<String> = item.segments().map(|s| s.text()).collect();
            let names: Vec<String> = item
                .names()
                .map(|n| {
                    let name = n.name_ref().unwrap().text();
                    match n.rename() {
                        Some(rename) => format!("{name} as {}", rename.text()),
                        None => name,
                    }
                })
                .collect();
            format!("{}: {}", path.join("::"), names.join(", "))
        })
        .collect();
    assert_eq!(shown, ["a::b: x, y as z", "a: x"]);
}

/// A trait written without `requires` or without `=` reports one error on
/// the first token that is out of place, and its fix yields a clean trait.
#[test]
fn trait_missing_requires_or_eq() {
    let cases = [
        (
            r#"
trait Min = {
}
"#,
            "{",
            "expected `requires` before `{`: a trait lists its members in `requires { ... }`",
            "Insert `requires`",
            r#"
trait Min = requires {
}
"#,
        ),
        (
            r#"
trait Min {
}
"#,
            "{",
            "expected `=`: a trait is declared `trait Name = requires { ... }`",
            "Insert `= requires`",
            r#"
trait Min = requires {
}
"#,
        ),
        (
            r#"
trait Min requires {
}
"#,
            "requires",
            "expected `=`: a trait is declared `trait Name = requires { ... }`",
            "Insert `=`",
            r#"
trait Min = requires {
}
"#,
        ),
        (
            r#"
trait Min = unsafe {
}
"#,
            "{",
            "expected `requires` before `{`: a trait lists its members in `requires { ... }`",
            "Insert `requires`",
            r#"
trait Min = unsafe requires {
}
"#,
        ),
        (
            r#"
trait Min unsafe {
}
"#,
            "unsafe",
            "expected `=`: a trait is declared `trait Name = requires { ... }`",
            "Insert `= requires`",
            r#"
trait Min = unsafe requires {
}
"#,
        ),
        (
            r#"
trait Min unsafe requires {
}
"#,
            "unsafe",
            "expected `=`: a trait is declared `trait Name = requires { ... }`",
            "Insert `=`",
            r#"
trait Min = unsafe requires {
}
"#,
        ),
    ];
    // Validation rejects `unsafe` traits on their own; that error is not
    // part of the recovery.
    let errors = |text: &str| -> Vec<crate::SyntaxError> {
        crate::parse(text)
            .errors()
            .iter()
            .filter(|err| err.message != "`unsafe` traits are not supported yet")
            .cloned()
            .collect()
    };
    for (text, blamed, message, label, fixed) in cases {
        let [err] = &errors(text)[..] else {
            panic!("expected one error for {text:?}: {:?}", errors(text));
        };
        assert_eq!(&text[err.range], blamed, "{text:?}");
        assert_eq!(err.message, message, "{text:?}");
        let fix = err.fix.as_ref().expect("expected a fix");
        assert_eq!(fix.label, label);
        assert_eq!(apply_fix(text, fix), fixed);
        assert!(errors(fixed).is_empty(), "{fixed:?}");
    }
}

/// The member block of a trait missing `requires` parses as its body: the
/// missing keyword is the only error.
#[test]
fn trait_missing_requires_keeps_members() {
    check_errors(
        r#"
trait Min = {
    min: fn(Self, Self) -> Self;
};
"#,
        expect![[r#"
            13..14: expected `requires` before `{`: a trait lists its members in `requires { ... }`
        "#]],
    );
}

/// Each parser error is blamed on the innermost node that was open when it
/// was reported and began before it: inside nested nodes, after a sibling
/// finished, and for two errors reported with no node started between them.
#[test]
fn errors_blame_the_innermost_open_node_begun_before_them() {
    let input = r#"
static f = fn() -> () {
    let x = (1 + );
    g(h(1) 2);
    let = ;
};
"#;
    let parse = crate::parse(input);
    let root = parse.syntax_node();
    let text: String = parse
        .errors()
        .iter()
        .enumerate()
        .map(|(index, err)| {
            let blame = parse.blame(index);
            let kinds: Vec<_> = root
                .descendants()
                .filter(|node| node.text_range() == blame)
                .map(|node| format!("{:?}", node.kind()))
                .collect();
            format!(
                "{:?}: {} => {} {:?}\n",
                err.range,
                err.message,
                kinds.join("/"),
                blame
            )
        })
        .collect();
    expect![[r#"
        42..43: expected an expression => BIN_EXPR 38..41
        56..57: expected `,` or `)` => ARG_LIST 50..58
        68..69: expected a binding name => LET_STMT 64..71
        70..71: expected an expression => LET_STMT 64..71
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_record_field_written_name_colon_value_parses_its_value() {
    // `p.n + 1` reads as either up to `p`; the `.n` only a value has makes
    // it the retired construction spelling: one error on the `:`, and the
    // value is a value (no "unknown type `p`", no cascade through the call
    // and the block).
    check_errors(
        "static f = fn { g(struct { n: p.n + 1, m: p.m }); h() };",
        expect![[r#"
            28..29: record fields are defined with `=` (`name = value`); `:` annotates a type
            40..41: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_comparison_a_list_or_a_defining_record_after_a_colon_is_a_value() {
    // No `>` closes `n < limit` as a generic list, `[a, b]` is a list, and
    // `struct { a = 1 }` defines a field: each is a value by its form, with
    // the one `:` error.
    check_errors(
        "static f = fn { g(struct { ok: n < limit, xs: [a, b], p: struct { a = 1 } }) };",
        expect![[r#"
            29..30: record fields are defined with `=` (`name = value`); `:` annotates a type
            44..45: record fields are defined with `=` (`name = value`); `:` annotates a type
            55..56: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_field_operand_a_value_could_spell_is_left_to_hir() {
    // A name, a generic path, a repeat array and a borrow are each a type
    // or a value by what their name resolves to, which hir judges; `()`
    // and a fn type with no body are types, so those fields have no value.
    check_errors(
        "static f = fn { g(struct { a: other, b: Vec::<usize>, c: [n; 3], d: n.&, e: (), h: fn() }) };",
        expect![[r#"
            73..78: this field has a type but no value; write `name: Type = value` (or `name = value`)
            80..87: this field has a type but no value; write `name: Type = value` (or `name = value`)
        "#]],
    );
}

#[test]
fn a_record_type_after_a_fields_colon_is_a_record_type() {
    // Before its `=`, `struct { a: usize }` reads as either; no field
    // defines a value, so it is the record type.
    check(
        "static s = struct { x: struct { a: usize } = v };",
        expect![[r#"
            SOURCE_FILE@0..49
              STATIC_ITEM@0..49
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..48
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..46
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    RECORD_TYPE@23..42
                      STRUCT_KW@23..29 "struct"
                      WHITESPACE@29..30 " "
                      L_BRACE@30..31 "{"
                      WHITESPACE@31..32 " "
                      RECORD_TYPE_FIELD@32..40
                        NAME@32..33
                          IDENT@32..33 "a"
                        COLON@33..34 ":"
                        WHITESPACE@34..35 " "
                        PATH_TYPE@35..40
                          NAME_REF@35..40
                            IDENT@35..40 "usize"
                      WHITESPACE@40..41 " "
                      R_BRACE@41..42 "}"
                    WHITESPACE@42..43 " "
                    EQ@43..44 "="
                    WHITESPACE@44..45 " "
                    PATH_EXPR@45..46
                      NAME_REF@45..46
                        IDENT@45..46 "v"
                  WHITESPACE@46..47 " "
                  R_BRACE@47..48 "}"
                SEMICOLON@48..49 ";"
        "#]],
    );
}

#[test]
fn a_half_written_field_type_keeps_its_own_errors() {
    // A `type` declaration's field is a type, so a half-written one gets
    // the type grammar's errors.
    check_errors(
        "type P = struct { xs: Vec::<usize };",
        expect![[r#"
        32..33: expected `>`
        34..35: expected `,` or `>`
    "#]],
    );
}

#[test]
fn deeply_nested_field_operands_parse_once() {
    // Each `f:` holds the next literal in a `const { }` block. Every
    // operand is parsed once, so each level costs the same however deep
    // the rest goes.
    let mut field = "x: p.x - 1".to_owned();
    for _ in 0..30 {
        field = format!("f: [usize; const {{ g(struct {{ {field} }}) }}]");
    }
    let parse = crate::parse(&format!("static s = g(struct {{ {field} }});"));
    // Only the innermost operand is a value by its form; each `f:` is an
    // array type or value by what `usize` names, which hir judges.
    let count = |prefix: &str| {
        parse
            .errors()
            .iter()
            .filter(|err| err.message.starts_with(prefix))
            .count()
    };
    let retired = count("record fields are defined with `=`");
    let valueless = count("this field has a type but no value");
    assert_eq!((retired, valueless), (1, 0), "{:?}", parse.errors());
    assert_eq!(parse.errors().len(), 1, "{:?}", parse.errors());
}

#[test]
fn a_field_type_with_turbofish_commas_still_finds_its_value() {
    // The turbofish's `,` is inside the type, not the end of the field.
    check_errors(
        "static f = fn (q: Pair::<usize, bool>) { struct { p: Pair::<usize, bool> = q, n: a < b, c: 1 }; };",
        expect![[r#"
            79..80: record fields are defined with `=` (`name = value`); `:` annotates a type
            89..90: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_field_type_with_angle_groups_still_finds_its_value() {
    // A nested turbofish and the bare-angle typo are annotations; the typo
    // keeps its own `::` correction.
    check_errors(
        "static f = fn (q: usize) { struct { a: A::<B::<usize, bool>, u8> = q, b: Pair<usize, bool> = q }; };",
        expect![[r#"
            73..90: generic arguments use the turbofish: write `Pair::<...>`
        "#]],
    );
}

#[test]
fn an_unclosed_turbofish_in_a_field_type_ends_at_the_literal() {
    // TODO: halvko/must#242: the open generic list swallows `n: 1`, so its
    // retired `:` goes unreported.
    check_errors(
        "static f = fn (q: usize) { struct { p: Vec::<usize = q, n: 1 }; };",
        expect![[r#"
            57..58: expected `,` or `>`
        "#]],
    );
}

#[test]
fn a_value_less_fn_type_field_is_a_type_and_fn_literals_are_values() {
    // A `fn` with no body is a type, so it has no value; a `fn` literal and
    // an array are values written with the retired `:`.
    check_errors(
        "static s = struct { f: fn(usize) -> usize, g: fn (a: usize) { a }, xs: [1, 2] };",
        expect![[r#"
            20..41: this field has a type but no value; write `name: Type = value` (or `name = value`)
            44..45: record fields are defined with `=` (`name = value`); `:` annotates a type
            69..70: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

/// A `fn` literal with no parameter list is a value, so the `(` its type
/// reading would want is never reported: the one error is the `:`.
#[test]
fn a_paramless_fn_literal_after_a_fields_colon_gets_only_the_colon_error() {
    check_errors(
        "static s = struct { k: fn { 1 } };",
        expect![[r#"
            21..22: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_struct_type_in_a_fn_signature_is_not_a_fn_body() {
    // The `{` after `struct` opens a record type, not a body, so `h` stays a
    // fn type; `k`'s body comes after its `struct { .. }` return type.
    check_errors(
        "static s = struct { h: fn(struct { a: usize }) -> struct { b: usize }, k: fn() -> struct { b: usize } { 1 } };",
        expect![[r#"
            20..69: this field has a type but no value; write `name: Type = value` (or `name = value`)
            72..73: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_record_field_missing_its_comma_keeps_the_ordinary_errors() {
    // The comma error is the only syntax error: `n: usize` stays an
    // annotation, and hir reports that it has no value.
    check_errors(
        "static p = struct { n: usize m = 1 };",
        expect![[r#"
            29..30: expected `,` or `}`
        "#]],
    );
}
