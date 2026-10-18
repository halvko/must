//! `extern` items, the vouch marker and fn type parameter names.

use super::*;

// ---- `extern static` — `extern` item declarations -----------------------

#[test]
fn an_extern_item_is_a_declaration_with_no_initializer() {
    // `extern static name: TYPE;` — the marker leads the ITEM, the item has
    // no `=` at all, and the signature is an ordinary `unsafe fn` TYPE in
    // annotation position (named parameters and all).
    check(
        "extern static read: unsafe fn(u8.&raw mut, usize) -> isize;",
        expect![[r#"
            SOURCE_FILE@0..59
              STATIC_ITEM@0..59
                ITEM_HEAD@0..13
                  EXTERN_KW@0..6 "extern"
                  WHITESPACE@6..7 " "
                  STATIC_KW@7..13 "static"
                WHITESPACE@13..14 " "
                NAME@14..18
                  IDENT@14..18 "read"
                COLON@18..19 ":"
                WHITESPACE@19..20 " "
                FN_TYPE@20..58
                  UNSAFE_KW@20..26 "unsafe"
                  WHITESPACE@26..27 " "
                  FN_KW@27..29 "fn"
                  PARAM_LIST@29..49
                    L_PAREN@29..30 "("
                    PARAM@30..41
                      RAW_PTR_TYPE@30..41
                        PATH_TYPE@30..32
                          NAME_REF@30..32
                            IDENT@30..32 "u8"
                        DOT@32..33 "."
                        AMP@33..34 "&"
                        RAW_KW@34..37 "raw"
                        WHITESPACE@37..38 " "
                        MUT_KW@38..41 "mut"
                    COMMA@41..42 ","
                    WHITESPACE@42..43 " "
                    PARAM@43..48
                      PATH_TYPE@43..48
                        NAME_REF@43..48
                          IDENT@43..48 "usize"
                    R_PAREN@48..49 ")"
                  WHITESPACE@49..50 " "
                  RET_TYPE@50..58
                    THIN_ARROW@50..52 "->"
                    WHITESPACE@52..53 " "
                    PATH_TYPE@53..58
                      NAME_REF@53..58
                        IDENT@53..58 "isize"
                SEMICOLON@58..59 ";"
            error 0..6: declaring an `extern` item is a VOUCH: write `unsafe extern static`
        "#]],
    );
}

#[test]
fn a_fn_type_parameter_name_is_refused() {
    // A name is refused where it stands and consumed, so the parameter keeps
    // its type and the item parses on.
    check(
        "static f: fn(x: usize) -> usize = fn(n: usize) -> usize { n };",
        expect![[r#"
            SOURCE_FILE@0..62
              STATIC_ITEM@0..62
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                FN_TYPE@10..31
                  FN_KW@10..12 "fn"
                  PARAM_LIST@12..22
                    L_PAREN@12..13 "("
                    PARAM@13..21
                      IDENT@13..14 "x"
                      COLON@14..15 ":"
                      WHITESPACE@15..16 " "
                      PATH_TYPE@16..21
                        NAME_REF@16..21
                          IDENT@16..21 "usize"
                    R_PAREN@21..22 ")"
                  WHITESPACE@22..23 " "
                  RET_TYPE@23..31
                    THIN_ARROW@23..25 "->"
                    WHITESPACE@25..26 " "
                    PATH_TYPE@26..31
                      NAME_REF@26..31
                        IDENT@26..31 "usize"
                WHITESPACE@31..32 " "
                EQ@32..33 "="
                WHITESPACE@33..34 " "
                FN_LITERAL@34..61
                  FN_KW@34..36 "fn"
                  PARAM_LIST@36..46
                    L_PAREN@36..37 "("
                    PARAM@37..45
                      BIND_PAT@37..38
                        NAME@37..38
                          IDENT@37..38 "n"
                      COLON@38..39 ":"
                      WHITESPACE@39..40 " "
                      PATH_TYPE@40..45
                        NAME_REF@40..45
                          IDENT@40..45 "usize"
                    R_PAREN@45..46 ")"
                  WHITESPACE@46..47 " "
                  RET_TYPE@47..55
                    THIN_ARROW@47..49 "->"
                    WHITESPACE@49..50 " "
                    PATH_TYPE@50..55
                      NAME_REF@50..55
                        IDENT@50..55 "usize"
                  WHITESPACE@55..56 " "
                  BLOCK_EXPR@56..61
                    L_BRACE@56..57 "{"
                    WHITESPACE@57..58 " "
                    PATH_EXPR@58..59
                      NAME_REF@58..59
                        IDENT@58..59 "n"
                    WHITESPACE@59..60 " "
                    R_BRACE@60..61 "}"
                SEMICOLON@61..62 ";"
            error 13..14: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
        "#]],
    );
}

#[test]
fn extern_static_error_forms() {
    check_errors(
        "unsafe extern static a: unsafe fn(i64) -> i64 = 1;\n\
         unsafe extern static c: usize;\n\
         unsafe extern static d;\n\
         unsafe extern const e: unsafe fn() -> ();\n\
         unsafe extern type F = usize;\n\
         unsafe extern static g: unsafe fn::<T>(T) -> i64;\n\
         unsafe extern static i: unsafe fn(_) -> _;\n\
         unsafe extern static j: unsafe fn(_) -> i64 = fn(n: i64) -> i64 { n };\n\
         static h: usize;\n",
        expect![[r#"
            46..49: an `extern static` has no initializer: the declaration is the whole contract, and an `extern` item sets nothing to anything
            75..80: `extern` data is not supported yet — an `extern` item must have a function type
            103..104: an `extern` item must declare its type: `unsafe extern static name: fn(...) -> T;`
            113..119: only a `static` can be `extern`: an `extern` item declares one name with one type
            148..154: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
            211..216: an `extern` item cannot be generic: it has exactly one machine signature, and there is nothing to monomorphize it into
            262..263: an `extern` item's type must be written in full: the declaration is the whole contract, and there is no body for `_` to be inferred from
            268..269: an `extern` item's type must be written in full: the declaration is the whole contract, and there is no body for `_` to be inferred from
            315..340: an `extern static` has no initializer: the declaration is the whole contract, and an `extern` item sets nothing to anything
            357..358: expected `=` followed by the item's value
        "#]],
    );
}

#[test]
fn a_vouched_extern_item_may_be_declared_safe_to_call() {
    // A plain `fn` type is accepted: the item vouches, the type says calls are safe.
    for source in [
        "unsafe extern static now: fn() -> i64;",
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;",
        "unsafe extern static tick: fn() -> ();",
    ] {
        assert_eq!(
            crate::parse(source).errors(),
            &[],
            "`{source}` must parse clean"
        );
    }
}

#[test]
fn an_unmarked_extern_item_names_the_vouch_and_offers_it() {
    // Missing `unsafe` is reported with a fix, never inserted silently.
    let source = "extern static read: unsafe fn(u8.&raw mut, usize) -> isize;";
    let parse = crate::parse(source);
    let [err] = parse.errors() else {
        panic!("expected exactly one error, got {:?}", parse.errors());
    };
    assert_eq!(
        err.message,
        "declaring an `extern` item is a VOUCH: write `unsafe extern static`"
    );
    let fix = err.fix.as_ref().expect("the vouch is offered");
    let fixed = apply_fix(source, fix);
    assert_eq!(
        fixed,
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;"
    );
    assert_eq!(crate::parse(&fixed).errors(), &[]);
}

#[test]
fn the_markers_come_first_unsafe_before_extern() {
    // A head in any other order parses as the same item; the fix rewrites it.
    for source in [
        "extern unsafe static read: unsafe fn(i64) -> i64;",
        "static unsafe extern read: unsafe fn(i64) -> i64;",
        "unsafe static extern read: unsafe fn(i64) -> i64;",
    ] {
        let parse = crate::parse(source);
        let [err] = parse.errors() else {
            panic!(
                "expected exactly one error for `{source}`, got {:?}",
                parse.errors()
            );
        };
        assert_eq!(
            err.message,
            "the markers come first, `unsafe` before `extern`: write `unsafe extern static`"
        );
        let fix = err.fix.as_ref().expect("the rewrite is offered");
        let fixed = apply_fix(source, fix);
        assert_eq!(fixed, "unsafe extern static read: unsafe fn(i64) -> i64;");
        assert_eq!(crate::parse(&fixed).errors(), &[]);
    }
}

#[test]
fn an_extern_return_type_must_be_written() {
    let source = "unsafe extern static tick: fn();";
    let parse = crate::parse(source);
    let [err] = parse.errors() else {
        panic!("expected exactly one error, got {:?}", parse.errors());
    };
    let fix = err.fix.as_ref().expect("`-> ()` is offered");
    let fixed = apply_fix(source, fix);
    assert_eq!(fixed, "unsafe extern static tick: fn() -> ();");
    assert_eq!(crate::parse(&fixed).errors(), &[]);
}

#[test]
fn an_extern_const_is_fixed_to_an_extern_static() {
    let source = "unsafe extern const now: fn() -> i64;";
    let parse = crate::parse(source);
    let [err] = parse.errors() else {
        panic!("expected exactly one error, got {:?}", parse.errors());
    };
    let fix = err.fix.as_ref().expect("the rewrite is offered");
    let fixed = apply_fix(source, fix);
    assert_eq!(fixed, "unsafe extern static now: fn() -> i64;");
    assert_eq!(crate::parse(&fixed).errors(), &[]);
}

#[test]
fn a_doubled_vouch_marker_is_reported_once_and_removed() {
    // A duplicate `unsafe` is one diagnostic with a fix.
    let source = "unsafe extern unsafe static read: fn() -> i64;";
    let parse = crate::parse(source);
    let [err] = parse.errors() else {
        panic!("expected exactly one error, got {:?}", parse.errors());
    };
    assert_eq!(err.message, "`unsafe` is written once, not twice");
    let fix = err.fix.as_ref().expect("the removal is offered");
    let fixed = apply_fix(source, fix);
    assert_eq!(fixed, "unsafe extern static read: fn() -> i64;");
    assert_eq!(crate::parse(&fixed).errors(), &[]);
}

#[test]
fn every_marker_and_keyword_arrangement_parses_into_the_head() {
    // The head takes markers and keywords in any order and any number, so
    // the item reads; validation names what is wrong with one fix per
    // mistake.
    for (source, fixed) in [
        (
            "unsafe extern static static read: fn() -> i64;",
            "unsafe extern static read: fn() -> i64;",
        ),
        ("static static x: i64 = 1;", "static x: i64 = 1;"),
        ("static const x: i64 = 1;", "static x: i64 = 1;"),
        ("type type T = i64;", "type T = i64;"),
        ("trait trait T = requires { };", "trait T = requires { };"),
        (
            "unsafe unsafe extern static read: fn() -> i64;",
            "unsafe extern static read: fn() -> i64;",
        ),
        (
            "unsafe extern extern static read: fn() -> i64;",
            "unsafe extern static read: fn() -> i64;",
        ),
    ] {
        let parse = crate::parse(source);
        let [err] = parse.errors() else {
            panic!(
                "expected exactly one error for `{source}`, got {:?}",
                parse.errors()
            );
        };
        let fix = err.fix.as_ref().expect("the removal is offered");
        assert_eq!(apply_fix(source, fix), fixed);
    }
    check(
        "extern unsafe extern static read: fn() -> i64;",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                ITEM_HEAD@0..27
                  EXTERN_KW@0..6 "extern"
                  WHITESPACE@6..7 " "
                  UNSAFE_KW@7..13 "unsafe"
                  WHITESPACE@13..14 " "
                  EXTERN_KW@14..20 "extern"
                  WHITESPACE@20..21 " "
                  STATIC_KW@21..27 "static"
                WHITESPACE@27..28 " "
                NAME@28..32
                  IDENT@28..32 "read"
                COLON@32..33 ":"
                WHITESPACE@33..34 " "
                FN_TYPE@34..45
                  FN_KW@34..36 "fn"
                  PARAM_LIST@36..38
                    L_PAREN@36..37 "("
                    R_PAREN@37..38 ")"
                  WHITESPACE@38..39 " "
                  RET_TYPE@39..45
                    THIN_ARROW@39..41 "->"
                    WHITESPACE@41..42 " "
                    PATH_TYPE@42..45
                      NAME_REF@42..45
                        IDENT@42..45 "i64"
                SEMICOLON@45..46 ";"
            error 0..27: the markers come first, `unsafe` before `extern`: write `unsafe extern static`
            error 14..20: `extern` is written once, not twice
        "#]],
    );
}

#[test]
fn only_an_extern_item_can_be_vouched_for() {
    // `unsafe` is only allowed on `extern static`.
    check_errors(
        r#"
unsafe static a = 1;
unsafe type F = usize;
unsafe trait T = requires { };
static unsafe b = 1;
type unsafe G = usize;
"#,
        expect![[r#"
            1..7: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
            22..28: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
            45..51: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
            83..89: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
            102..108: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
        "#]],
    );
}

#[test]
fn a_lone_vouch_marker_says_what_may_follow_it() {
    // A lone `unsafe` reports once and takes the rest as `ERROR`.
    check_errors(
        "unsafe fn g(n: i64) -> i64;\n",
        expect![[r#"
            7..9: expected `extern static` after `unsafe`: the marker vouches for an `extern` item's declared signature, and only an `extern` item declares one
        "#]],
    );
}

#[test]
fn a_written_value_takes_an_extern_statics_annotation_out_of_the_extern_rules() {
    // `j` in `extern_static_error_forms`: the value wins, so the item is
    // not an `extern` item and its annotation is an ordinary one — the `_` is
    // filled from the body written on the same line. Only the initializer
    // is refused, and none of the contract rules (which would each be a
    // sentence that is not true of this program) fire.
    let source = "unsafe extern static j: unsafe fn(_) -> i64 = fn(n: i64) -> i64 { n };";
    let messages: Vec<_> = crate::parse(source)
        .errors()
        .iter()
        .map(|e| e.message.clone())
        .collect();
    assert_eq!(
        messages,
        vec![
            "an `extern static` has no initializer: the declaration is the whole \
             contract, and an `extern` item sets nothing to anything"
                .to_owned()
        ]
    );
}

#[test]
fn a_fn_types_parameter_name_is_refused_at_every_index() {
    // One error per name, at the name, and none of them derails the item.
    for (source, names) in [
        ("static f: fn(x: usize, y: usize) -> usize = g;", 2),
        ("static f: fn(usize, y: usize) -> usize = g;", 1),
        ("static f: fn(x: usize, usize) -> usize = g;", 1),
        ("static f: fn(_: usize, usize) -> usize = g;", 1),
    ] {
        let parse = crate::parse(source);
        let errors = parse.errors();
        assert_eq!(errors.len(), names, "`{source}`: {errors:?}");
        assert!(
            errors.iter().all(|e| e
                .message
                .starts_with("a function type's parameters are bare types")),
            "`{source}`: {errors:?}"
        );
    }
}

#[test]
fn a_fn_type_admits_no_patterns() {
    // A fn LITERAL's parameters are patterns and they BIND; a fn type's
    // are bare types, so the pattern grammar is not admitted at all.
    // Refused where it is written, never reinterpreted as something else.
    check_errors(
        "static a: fn(mut x: usize) -> usize = g;\n\
         static b: fn(struct { x: usize }: usize) -> usize = g;\n\
         static c: fn(x:) -> usize = g;\n",
        expect![[r#"
            13..16: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
            73..74: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
            109..110: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
            111..112: expected a type
        "#]],
    );
}

#[test]
fn removing_an_extern_statics_initializer_keeps_the_declaration() {
    // The fix cuts the VALUE, not the contract: an `extern` item with its type
    // taken away would be a second error where there was one, and the type
    // is the whole of what the declaration says.
    let source = "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize = f;";
    let parse = crate::parse(source);
    let err = parse
        .errors()
        .iter()
        .find(|e| {
            e.message
                .starts_with("an `extern static` has no initializer")
        })
        .expect("the initializer is refused");
    let fix = err.fix.as_ref().expect("with a removal fix");
    let fixed = apply_fix(source, fix);
    assert_eq!(
        fixed,
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;"
    );
    assert_eq!(crate::parse(&fixed).errors(), &[]);
}

#[test]
fn extern_fn_in_an_initializer_is_an_ordinary_parse_error() {
    // `extern` marks an item, not a fn literal: the old `static g = extern
    // fn(...);` spelling gets the parser's own errors, with `extern` opening
    // the next item.
    let source = "static g = extern fn(n: usize) -> usize;";
    check_errors(
        source,
        expect![[r#"
            9..10: expected `;`
            11..17: expected an expression
            18..20: expected `static` after `extern`: an `extern` item declares one name with one type
        "#]],
    );
}
