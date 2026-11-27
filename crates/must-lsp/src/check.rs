//! `must check file.must ...`: batch diagnostics as text. The same
//! `ide::Analysis` the editor drives — only the presentation differs: text
//! with source snippets instead of published LSP diagnostics. Each file is
//! checked with its whole library, when a `must.toml` sits at or above it:
//! every error in the library is reported, the named files' first.

use base_db::{RootDatabase, SourceFile};
use ide::{LineIndex, Severity};

use crate::project::Sources;

/// Check every file, print rendered diagnostics, return the exit code:
/// 0 clean (warnings allowed), 1 with errors, 2 on usage/IO problems.
pub fn check(paths: &[String]) -> i32 {
    if paths.is_empty() {
        eprintln!("usage: must check <file.must>...");
        return 2;
    }
    let mut db = RootDatabase::default();
    let mut sources = Sources::default();
    let mut files = Vec::new();
    for path in paths {
        match sources.load(&mut db, path, str::to_owned) {
            Ok(file) => files.push(file),
            Err(message) => {
                eprintln!("{message}");
                return 2;
            }
        }
    }
    let mut checked: Vec<SourceFile> = files.clone();
    for &file in &files {
        for other in library_files(&db, file) {
            if !checked.contains(&other) {
                checked.push(other);
            }
        }
    }
    let analysis = ide::Analysis::new(db);
    let mut errors = 0;
    let mut warnings = 0;
    for &file in &checked {
        let rendered = render(&analysis, &sources, file, &analysis.diagnostics(file));
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

/// Every module of `file`'s library, `file` first; just `file` when it
/// stands alone.
pub fn library_files(db: &RootDatabase, file: SourceFile) -> Vec<SourceFile> {
    let mut files = vec![file];
    let library = hir::library::library_of(db, file);
    files.extend(library.files(db).into_iter().filter(|&other| other != file));
    files
}

pub struct Rendered {
    pub text: String,
    pub errors: usize,
    pub warnings: usize,
}

/// Render one file's diagnostics rustc-style: severity + message, location,
/// the offending source line with a caret span, then the related locations
/// as notes, each in its own file. Info-severity diagnostics are skipped —
/// they are companions synthesized for editors that render related
/// information poorly; in text the notes already carry that content.
pub fn render(
    analysis: &ide::Analysis,
    sources: &Sources,
    file: SourceFile,
    diagnostics: &[ide::Diagnostic],
) -> Rendered {
    let path = sources.display(file).expect("a rendered file was loaded");
    let text = sources.text(file).expect("a rendered file was loaded");
    let line_index = analysis.line_index(file);
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
        out.push_str(&snippet(text, line_index, start.line, diag.range));
        if let Some(fix) = &diag.fix {
            out.push_str(&format!("   = help: {}\n", fix.label));
        }
        for related in &diag.related {
            // A location in a file no tool loaded (the prelude) has no
            // text to point into.
            let Some(related_path) = sources.display(related.file) else {
                out.push_str(&format!("   = note: {}\n", related.message));
                continue;
            };
            let pos = analysis
                .line_index(related.file)
                .line_col(related.range.start());
            out.push_str(&format!(
                "   = note: {} ({related_path}:{}:{})\n",
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

    fn render(text: &str) -> super::Rendered {
        let mut host = ide::AnalysisHost::new();
        let file = host.create_file("test.must".to_owned(), text.to_owned());
        let mut sources = crate::project::Sources::default();
        sources.insert(file, "test.must".to_owned(), text.to_owned());
        let analysis = host.snapshot();
        super::render(&analysis, &sources, file, &analysis.diagnostics(file))
    }

    fn check_render(text: &str, expect: expect_test::Expect) {
        expect.assert_eq(&render(text).text);
    }

    #[test]
    fn renders_blame_with_notes() {
        check_render(
            r#"
static constrainer = fn (_s: str, _u: usize) {}

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
            "static f = fn true;",
            expect![[r#"
                error: function bodies are blocks; wrap this expression in `{ }`
                  --> test.must:1:15
                  |
                1 | static f = fn true;
                  |               ^^^^
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
        let rendered = render("static _ = true;");
        expect_test::expect![[r#"
            warning: this item binds nothing and its value cannot be used
              --> test.must:1:8
              |
            1 | static _ = true;
              |        ^

        "#]]
        .assert_eq(&rendered.text);
        assert_eq!(rendered.errors, 0);
        assert_eq!(rendered.warnings, 1);
    }
}
