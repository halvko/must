//! `must-lsp check file.must ...`: batch diagnostics as text. The same
//! `ide::Analysis` the editor drives — only the presentation differs: text
//! with source snippets instead of published LSP diagnostics.

use ide::{AnalysisHost, LineIndex, Severity};

/// Check every file, print rendered diagnostics, return the exit code:
/// 0 clean (warnings allowed), 1 with errors, 2 on usage/IO problems.
pub fn check(paths: &[String]) -> i32 {
    if paths.is_empty() {
        eprintln!("usage: must-lsp check <file.must>...");
        return 2;
    }
    let mut host = AnalysisHost::new();
    let mut files = Vec::new();
    for path in paths {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let file = host.create_file(path.clone(), text.clone());
                files.push((path.clone(), text, file));
            }
            Err(err) => {
                eprintln!("error: cannot read `{path}`: {err}");
                return 2;
            }
        }
    }
    let analysis = host.snapshot();
    let mut errors = 0;
    let mut warnings = 0;
    for (path, text, file) in &files {
        let rendered = render(path, text, &analysis.diagnostics(*file));
        errors += rendered.errors;
        warnings += rendered.warnings;
        print!("{}", rendered.text);
    }
    if errors > 0 || warnings > 0 {
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let mut summary = format!("found {errors} error{}", plural(errors));
        if warnings > 0 {
            summary.push_str(&format!(" and {warnings} warning{}", plural(warnings)));
        }
        eprintln!("{summary}");
    }
    if errors > 0 { 1 } else { 0 }
}

pub struct Rendered {
    pub text: String,
    pub errors: usize,
    pub warnings: usize,
}

/// Render one file's diagnostics rustc-style: severity + message, location,
/// the offending source line with a caret span, then the related locations
/// as notes. Info-severity diagnostics are skipped — they are companions
/// synthesized for editors that render related information poorly; in text
/// the notes already carry that content.
pub fn render(path: &str, text: &str, diagnostics: &[ide::Diagnostic]) -> Rendered {
    let line_index = LineIndex::new(text);
    let mut out = String::new();
    let mut errors = 0;
    let mut warnings = 0;
    for diag in diagnostics {
        let severity = match diag.severity {
            Severity::Error => {
                errors += 1;
                "error"
            }
            Severity::Warning => {
                warnings += 1;
                "warning"
            }
            Severity::Info => continue,
        };
        let start = line_index.line_col(diag.range.start());
        out.push_str(&format!("{severity}: {}\n", diag.message));
        out.push_str(&format!(
            "  --> {path}:{}:{}\n",
            start.line + 1,
            start.col + 1
        ));
        out.push_str(&snippet(text, &line_index, start.line, diag.range));
        if let Some(fix) = &diag.fix {
            out.push_str(&format!("   = help: {}\n", fix.label));
        }
        for related in &diag.related {
            // Single-file world: every related location is in this file.
            let pos = line_index.line_col(related.range.start());
            out.push_str(&format!(
                "   = note: {} ({path}:{}:{})\n",
                related.message,
                pos.line + 1,
                pos.col + 1
            ));
        }
        out.push('\n');
    }
    Rendered {
        text: out,
        errors,
        warnings,
    }
}

/// The source line the range starts on, with a caret marker under the range
/// (clamped to that line).
fn snippet(text: &str, line_index: &LineIndex, line: u32, range: syntax::TextRange) -> String {
    let Some(line_start) = line_index.offset(line_index::LineCol { line, col: 0 }) else {
        return String::new();
    };
    let line_start = usize::from(line_start);
    let line_text = text[line_start..].lines().next().unwrap_or("");
    let col = usize::from(range.start()) - line_start;
    let len = usize::from(range.len())
        .min(line_text.len().saturating_sub(col))
        .max(1);
    let number = format!("{}", line + 1);
    let gutter = " ".repeat(number.len());
    format!(
        "{gutter} |\n{number} | {line_text}\n{gutter} | {}{}\n",
        " ".repeat(col),
        "^".repeat(len),
    )
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    fn check_render(text: &str, expect: expect_test::Expect) {
        let mut host = ide::AnalysisHost::new();
        let file = host.create_file("test.must".to_owned(), text.to_owned());
        let analysis = host.snapshot();
        let rendered = super::render("test.must", text, &analysis.diagnostics(file));
        expect.assert_eq(&rendered.text);
    }

    #[test]
    fn renders_blame_with_notes() {
        check_render(
            r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    constrainer("", x);
}
"#,
            expect![[r#"
                error: type mismatch: expected `usize`, found `str`
                  --> test.must:5:25
                  |
                5 |     let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
                  |                         ^^
                   = note: this call requires `usize` (test.must:6:5)
                   = note: this argument needs to be `usize` (test.must:6:21)

                error: type mismatch: expected `usize`, found `str`
                  --> test.must:5:49
                  |
                5 |     let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
                  |                                                 ^^
                   = note: this call requires `usize` (test.must:6:5)
                   = note: this argument needs to be `usize` (test.must:6:21)

            "#]],
        );
    }

    #[test]
    fn renders_parse_error_with_fix_help() {
        check_render(
            "static f = fn 42;",
            expect![[r#"
                error: function bodies are blocks; wrap this expression in `{ }`
                  --> test.must:1:15
                  |
                1 | static f = fn 42;
                  |               ^^
                   = help: Wrap in `{ }`

            "#]],
        );
    }

    #[test]
    fn clean_file_renders_nothing() {
        check_render(r#"static main = fn { print("hi"); };"#, expect![[r#""#]]);
    }

    #[test]
    fn renders_const_check_findings() {
        check_render(
            "static double = fn (n: usize) -> usize { n * 2 };\nstatic x: usize = double(2);",
            expect![[r#"
                error: cannot call `double` in a const context; marking it `const fn` would allow this
                  --> test.must:2:19
                  |
                2 | static x: usize = double(2);
                  |                   ^^^^^^
                   = help: Mark `double` as `const fn`
                   = note: `double` is defined here (test.must:1:8)
                   = note: this item's initializer is a const context (test.must:2:1)

            "#]],
        );
    }

    #[test]
    fn renders_dead_code_warning_distinctly_from_errors() {
        // A hole-named item is a warning, not an error: rendered with the
        // "warning" severity word and counted separately in the summary.
        let mut host = ide::AnalysisHost::new();
        let file = host.create_file("test.must".to_owned(), "static _ = 5;".to_owned());
        let analysis = host.snapshot();
        let rendered = super::render("test.must", "static _ = 5;", &analysis.diagnostics(file));
        expect_test::expect![[r#"
            warning: this item binds nothing and its value cannot be used
              --> test.must:1:8
              |
            1 | static _ = 5;
              |        ^

        "#]]
        .assert_eq(&rendered.text);
        assert_eq!(rendered.errors, 0);
        assert_eq!(rendered.warnings, 1);
    }
}
