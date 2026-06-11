use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

fn check_diagnostics(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let rendered = crate::file_diagnostics(&db, file)
        .into_iter()
        .map(|d| format!("{:?}: {}\n", d.range, d.message))
        .collect::<String>();
    expect.assert_eq(&rendered);
}

#[test]
fn unresolved_name() {
    check_diagnostics(
        "static main = fn { missing() };",
        expect![[r#"
            19..26: unresolved name `missing`
        "#]],
    );
}

#[test]
fn locals_params_and_shadowing_resolve() {
    check_diagnostics(
        r#"
static f = fn (a: usize) {
    let b = a;
    let b = b;
    print(b);
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn let_initializer_does_not_see_its_own_binding() {
    check_diagnostics(
        "static f = fn { let x = x; };",
        expect![[r#"
            24..25: unresolved name `x`
        "#]],
    );
}

#[test]
fn mutual_recursion_and_self_reference_resolve() {
    check_diagnostics(
        r#"
const fib1 = fn (n: usize) fib2(n-1) + fib2(n-2);
static fib2 = fn (n: usize) fib1(n-1) + fib2(n-2);
"#,
        expect![[r#""#]],
    );
}

#[test]
fn builtins_resolve() {
    check_diagnostics(
        r#"static main = fn { print("hi"); panic("boom"); };"#,
        expect![[r#""#]],
    );
}

#[test]
fn fn_params_scoped_to_their_literal() {
    check_diagnostics(
        r#"
static f = fn {
    (fn (inner: usize) inner)(1);
    print(inner);
}
"#,
        expect![[r#"
            61..66: unresolved name `inner`
        "#]],
    );
}
