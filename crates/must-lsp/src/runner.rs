//! `must-lsp run file.must -e 'expr'`: evaluate an expression in a file's
//! scope with run-mode semantics. The same MIR and machine the editor's
//! const eval uses — only the driver differs.
//!
//! Programs run even when they don't typecheck: execution proceeds until it
//! reaches a trap, then crashes with the diagnostic the editor would show,
//! located.

use std::io::Write;

use base_db::{RootDatabase, SourceFile};
use eval::{EvalErrorKind, Machine, RunMode, Value};
use line_index::LineIndex;

/// The entry expression is wrapped in a synthetic item appended to the file:
/// running *is* evaluating that item's root body, just at `const_depth` 0.
const ENTRY_NAME: &str = "__must_entry";

/// Read, evaluate, print. Returns the process exit code.
pub fn run(path: &str, expr: &str) -> i32 {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read `{path}`: {err}");
            return 2;
        }
    };
    let stdout = std::io::stdout();
    match evaluate(text, path, expr, stdout.lock()) {
        Ok(Some(value)) => {
            println!("{value}");
            0
        }
        Ok(None) => 0,
        Err(rendered) => {
            eprintln!("{rendered}");
            1
        }
    }
}

/// Evaluate `expr` in `text`'s file scope, writing `print` output to `out`.
/// `Ok(Some(_))` is a non-unit result rendered for display; errors come back
/// rendered with a `path:line:col` location when one is known.
pub fn evaluate(
    text: String,
    path: &str,
    expr: &str,
    out: impl Write,
) -> Result<Option<String>, String> {
    let db = RootDatabase::default();
    let original_len = text.len();
    let full = format!("{text}\nstatic {ENTRY_NAME} = ({expr});\n");
    let file = SourceFile::new(&db, path.to_owned(), full);

    // The program may be arbitrarily broken — that's the point — but the
    // *entry expression* must at least parse, or running it means nothing.
    if let Some(err) = base_db::parse(&db, file)
        .errors()
        .iter()
        .find(|err| usize::from(err.range.start()) > original_len)
    {
        return Err(format!("error: invalid entry expression: {}", err.message));
    }

    let Some(&entry) = hir::file_item_ids(&db, file)
        .iter()
        .find(|&&item| item.name(&db) == ENTRY_NAME)
    else {
        return Err("error: invalid entry expression".to_owned());
    };

    let mut machine = Machine::new(&db, RunMode { out });
    match machine.eval_root(hir::item_loc(&db, entry)) {
        Ok(Value::Unit) => Ok(None),
        Ok(value) => Ok(Some(value.display())),
        Err(err) => {
            let prefix = match err.kind {
                // A deferred diagnostic was executed.
                EvalErrorKind::Trap => "error",
                EvalErrorKind::Panic => "panicked",
                EvalErrorKind::Runtime => "runtime error",
                EvalErrorKind::NotConst => "error",
            };
            let mut rendered = format!("{prefix}: {}", err.message);
            if let Some(location) = locate(&db, file, path, err.origin) {
                rendered.push_str(&format!("\n  --> {location}"));
            }
            Err(rendered)
        }
    }
}

fn locate(
    db: &RootDatabase,
    file: SourceFile,
    path: &str,
    origin: Option<(hir::ItemLoc, hir::ExprId)>,
) -> Option<String> {
    let (loc, expr) = origin?;
    // Single-file world: the origin's file is the one we run.
    let item = loc.to_id(db)?;
    let (_, source_map) = hir::body_with_source_map(db, item);
    let range = source_map.node_for_expr(expr)?.text_range();
    let line_col = LineIndex::new(file.text(db)).line_col(range.start());
    Some(format!(
        "{path}:{}:{}",
        line_col.line + 1,
        line_col.col + 1
    ))
}

#[cfg(test)]
mod tests {
    /// Runs `expr` against `text`, rendering print output, then the result
    /// or the error.
    fn check(text: &str, expr: &str, expect: expect_test::Expect) {
        let mut out = Vec::new();
        let result = super::evaluate(text.to_owned(), "test.must", expr, &mut out);
        let mut rendered = String::from_utf8(out).unwrap();
        match result {
            Ok(Some(value)) => rendered.push_str(&format!("=> {value}\n")),
            Ok(None) => {}
            Err(err) => rendered.push_str(&format!("{err}\n")),
        }
        expect.assert_eq(&rendered);
    }

    #[test]
    fn runs_main_and_prints() {
        check(
            r#"
static greeting = "hello world";
static main = fn {
    print(greeting);
};
"#,
            "main()",
            expect_test::expect![[r#"
                hello world
            "#]],
        );
    }

    #[test]
    fn non_unit_results_are_displayed() {
        check(
            r#"
static fib = fn (n: usize) -> usize {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
"#,
            "fib(10)",
            expect_test::expect![[r#"
                => 55
            "#]],
        );
    }

    #[test]
    fn broken_programs_run_until_the_trap() {
        check(
            r#"
static main = fn {
    print("before");
    let v: usize = "s";
    print("after");
};
"#,
            "main()",
            expect_test::expect![[r#"
                before
                error: type mismatch: expected `usize`, found `str`
                  --> test.must:4:20
            "#]],
        );
    }

    #[test]
    fn panics_are_located() {
        check(
            r#"
static checked_div = fn (a: usize, b: usize) -> usize {
    if b == 0 { panic("divide by zero") } else { a / b }
}
"#,
            "checked_div(1, 0)",
            expect_test::expect![[r#"
                panicked: divide by zero
                  --> test.must:3:17
            "#]],
        );
    }

    #[test]
    fn entry_expression_must_parse() {
        check("static main = fn {};", "main(", expect_test::expect![[r#"
            error: invalid entry expression: expected `)`
        "#]]);
    }
}
