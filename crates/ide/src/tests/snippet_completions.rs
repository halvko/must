//! Completions that insert a snippet.

use super::*;

#[test]
fn completions_snippet_builtin_member_call_with_params() {
    // A builtin member is fn-shaped like any other dot candidate, so
    // accepting it inserts the call, not the bare name. The receiver is
    // not a written argument, so the index is the only tab stop.
    assert_eq!(
        completion_insert(
            r#"
static f = fn (s: str) {
    s.$0
};
"#,
            "next_char",
        ),
        crate::InsertText::Snippet {
            snippet: "next_char($1)".to_owned(),
            plain: "next_char()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_fn_call_with_params_inserts_a_snippet() {
    // A parameterful fn in expression position offers `name($1)`, falling
    // back to a bare `name()` for a snippet-incapable client.
    assert_eq!(
        completion_insert(
            r#"
static add = fn (a: usize, b: usize) -> usize { a };
static main = fn {
    $0
};
"#,
            "add",
        ),
        crate::InsertText::Snippet {
            snippet: "add($1)".to_owned(),
            plain: "add()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_zero_arity_fn_call_is_plain() {
    // No parameters, nothing for a tab stop to land on: always the plain
    // `name()`, snippet or not.
    assert_eq!(
        completion_insert(
            r#"
static go = fn { 1 };
static main = fn {
    $0
};
"#,
            "go",
        ),
        crate::InsertText::Plain("go()".to_owned())
    );
}

#[test]
fn completions_snippet_fn_typed_expectation_suppresses_the_call_snippet() {
    // Under a `fn`-typed expectation the position wants the fn *passed*,
    // not called — even though `helper` itself is nominally zero-arity, the
    // insertion is the bare name with no call syntax at all (distinct from
    // the zero-arity case above, which still inserts `()`).
    assert_eq!(
        completion_insert(
            r#"
static helper: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let f: fn() -> usize = h$0;
};
"#,
            "helper",
        ),
        crate::InsertText::Plain("helper".to_owned())
    );
}

#[test]
fn completions_snippet_fn_local_under_its_return_type_inserts_a_call() {
    // A `fn`-valued local completes as a call when the position wants what
    // it returns.
    assert_eq!(
        completion_insert(
            r#"
static main = fn() -> () {
    let xfun = fn { 11 };
    let xtra: usize = 1;
    let y = 11 + xf$0;
};
"#,
            "xfun",
        ),
        crate::InsertText::Plain("xfun()".to_owned())
    );
}

#[test]
fn completions_snippet_fn_local_with_params_inserts_a_snippet() {
    assert_eq!(
        completion_insert(
            r#"
static main = fn {
    let add = fn (a: usize) -> usize { a };
    let y: usize = a$0;
};
"#,
            "add",
        ),
        crate::InsertText::Snippet {
            snippet: "add($1)".to_owned(),
            plain: "add()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_fn_local_under_a_fn_expectation_is_bare() {
    // The position wants the `fn` itself: pass it, don't call it.
    assert_eq!(
        completion_insert(
            r#"
static main = fn {
    let helper = fn () -> usize { 1 };
    let f: fn() -> usize = h$0;
};
"#,
            "helper",
        ),
        crate::InsertText::Plain("helper".to_owned())
    );
}

#[test]
fn completions_snippet_builtin_fn_call_with_params() {
    // Builtins go through the same snippet rule as file items — `print`
    // takes one param.
    assert_eq!(
        completion_insert(
            r#"
static main = fn {
    $0
};
"#,
            "print",
        ),
        crate::InsertText::Snippet {
            snippet: "print($1)".to_owned(),
            plain: "print()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_type_item_rhs_struct_and_enum() {
    let fixture_text = "type X = $0;";
    assert_eq!(
        completion_insert(fixture_text, "struct"),
        crate::InsertText::Snippet {
            snippet: "struct { $1 }".to_owned(),
            plain: "struct { }".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "enum"),
        crate::InsertText::Snippet {
            snippet: "enum { $1 }".to_owned(),
            plain: "enum { }".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_block_keywords_write_their_block() {
    let fixture_text = r#"
static main = fn {
    $0
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "loop"),
        crate::InsertText::Snippet {
            snippet: "loop {$0}".to_owned(),
            plain: "loop".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "if"),
        crate::InsertText::Snippet {
            snippet: "if $0".to_owned(),
            plain: "if".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "match"),
        crate::InsertText::Snippet {
            snippet: "match $0".to_owned(),
            plain: "match".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_match_arm_payload_variant_qualified_context() {
    // Past an already-typed qualifier, a payload-carrying variant inserts
    // just the bare name plus its own tab-stopped parens; a payload-less
    // one is untouched (still a plain bare name, the payload-less form).
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "Circle"),
        crate::InsertText::Snippet {
            snippet: "Circle($1)".to_owned(),
            plain: "Circle".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "Point"),
        crate::InsertText::Plain("Point".to_owned())
    );
}

#[test]
fn completions_snippet_record_literal_missing_field_inserts_a_snippet() {
    assert_eq!(
        completion_insert(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    Point(struct { x = 1, $0 })
};
"#,
            "y",
        ),
        crate::InsertText::Snippet {
            snippet: "y = $1".to_owned(),
            plain: "y = ".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_record_literal_gold_local_stays_a_plain_bare_name() {
    // The shorthand-matching local (`y` meaning `y: y`) is not the field
    // slot itself — it keeps inserting just its own name, snippet-free.
    assert_eq!(
        completion_insert_kind(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let y = 5;
    Point(struct { x = 1, $0 })
};
"#,
            "y",
            crate::CompletionItemKind::Variable,
        ),
        crate::InsertText::Plain("y".to_owned())
    );
}
