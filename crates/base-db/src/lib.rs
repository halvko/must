//! Salsa database core: inputs and the `parse` and `line_index` queries.
//!
//! Everything downstream (hir, ide) computes via tracked queries over these
//! inputs; the LSP layer mutates inputs (which bumps the revision and cancels
//! in-flight queries on database clones) and reads queries off clones.

/// A file the compiler knows about, identified by its salsa handle.
///
/// Only what the *program* is made of belongs here. Whether an editor
/// currently holds the buffer open does not: `parse` must produce the same
/// tree either way, a query that read such a flag would recompute on focus
/// changes rather than on code changes, and the server already tracks
/// openness next to its `Uri` map — a second copy here could disagree with it.
#[salsa::input(debug)]
pub struct SourceFile {
    /// Display/debug only — the `Uri ↔ SourceFile` map is server state,
    /// not part of the database.
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
    /// The library the file belongs to, whose files it can import from; a
    /// file in no library is a library by itself.
    #[default]
    pub library: Option<Library>,
}

/// The files of one directory tree. Each file is a module; a `use` path
/// names one by its path from `root` (`use a::b::x` reads `root/a/b.must`).
#[salsa::input(debug)]
pub struct Library {
    /// The directory `use` paths start from.
    #[returns(ref)]
    pub root: String,
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Creates a library rooted at `root` of the files `(path, text)` and
/// returns them in that order.
pub fn new_library(db: &mut dyn Db, root: &str, files: Vec<(String, String)>) -> Vec<SourceFile> {
    use salsa::Setter as _;
    let library = Library::new(db, root.to_owned(), Vec::new());
    let files: Vec<SourceFile> = files
        .into_iter()
        .map(|(path, text)| {
            let file = SourceFile::new(db, path, text);
            file.set_library(db).to(Some(library));
            file
        })
        .collect();
    library.set_files(db).to(files.clone());
    files
}

/// The file the compiler-provided declarations belong to: one per
/// database, so every file that names `ReadLineResult` names the same
/// declaration. It has no text; its declarations come from hir's table.
#[salsa::input(singleton)]
pub struct Prelude {
    pub file: SourceFile,
}

/// Settings the editor or command line may change. One per database.
#[salsa::input(singleton)]
pub struct Settings {
    /// An inference group covering more expressions than this (its
    /// members' bodies plus the fully typed bodies that feed it) warns at
    /// each body's first use of its unannotated items: every edit to a body
    /// it covers re-runs all of it. `None` turns the warning off.
    pub big_inference_group: Option<u32>,
}

impl Settings {
    /// About 4ms of group inference per edit in a release build: timing a
    /// cold `signature` query over generated groups of 300 to 25000
    /// expressions was linear, about 0.4µs per expression.
    pub const DEFAULT_BIG_INFERENCE_GROUP: Option<u32> = Some(10_000);
}

/// The settings of `db`.
pub fn settings(db: &dyn Db) -> Settings {
    Settings::get(db)
}

#[salsa::db]
pub trait Db: salsa::Database {}

/// The prelude file of `db`.
pub fn prelude_file(db: &dyn Db) -> SourceFile {
    Prelude::get(db).file(db)
}

#[salsa::db]
#[derive(Clone)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

impl Default for RootDatabase {
    fn default() -> Self {
        RootDatabase::with_storage(salsa::Storage::default())
    }
}

impl RootDatabase {
    /// A database that reports salsa events to `callback` (used by tests to
    /// assert what does and does not get recomputed).
    pub fn with_event_callback(
        callback: Box<dyn Fn(salsa::Event) + Send + Sync + 'static>,
    ) -> Self {
        RootDatabase::with_storage(salsa::Storage::new(Some(callback)))
    }

    fn with_storage(storage: salsa::Storage<Self>) -> Self {
        let db = RootDatabase { storage };
        let file = SourceFile::new(&db, "<prelude>".to_owned(), String::new());
        Prelude::new(&db, file);
        Settings::new(&db, Settings::DEFAULT_BIG_INFERENCE_GROUP);
        db
    }
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl Db for RootDatabase {}

/// The files of a test fixture: each `//- path` line starts a file, and
/// text before the first one is ignored. A fixture with no such line is one
/// file, `test.must`.
pub fn fixture_files(fixture: &str) -> Vec<(String, String)> {
    if !fixture.lines().any(|line| line.starts_with("//- ")) {
        return vec![("test.must".to_owned(), fixture.to_owned())];
    }
    let mut files: Vec<(String, String)> = Vec::new();
    for line in fixture.split_inclusive('\n') {
        if let Some(path) = line.strip_prefix("//- ") {
            files.push((path.trim().to_owned(), String::new()));
        } else if let Some((_, text)) = files.last_mut() {
            text.push_str(line);
        }
    }
    files
}

#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn Db, file: SourceFile) -> syntax::Parse {
    syntax::parse(file.text(db))
}

/// The file's line index, built once per text revision and shared by
/// every caller that converts offsets to lines.
#[salsa::tracked(returns(ref))]
pub fn line_index(db: &dyn Db, file: SourceFile) -> ::line_index::LineIndex {
    ::line_index::LineIndex::new(file.text(db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use salsa::Setter as _;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn counting_db() -> (RootDatabase, Arc<AtomicUsize>) {
        let executions = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&executions);
        let db = RootDatabase::with_event_callback(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }));
        (db, executions)
    }

    #[test]
    fn parse_is_memoized_and_invalidated_by_edits() {
        let (mut db, executions) = counting_db();
        let file = SourceFile::new(&db, "test.must".to_owned(), "static a = 1;".to_owned());

        let first = parse(&db, file);
        assert_eq!(first.errors(), &[]);
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        // Same revision: memoized, no new execution.
        let _ = parse(&db, file);
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        // Edit: new revision, parse re-runs and sees the new text.
        file.set_text(&mut db).to("static a = ;".to_owned());
        let reparsed = parse(&db, file);
        assert_eq!(reparsed.errors().len(), 1);
        assert_eq!(executions.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn fixture_files_split_at_path_lines() {
        let files = fixture_files(
            r#"ignored
//- a.must
static a = 1;
//- dir/b.must
"#,
        );
        assert_eq!(
            files,
            [
                ("a.must".to_owned(), "static a = 1;\n".to_owned()),
                ("dir/b.must".to_owned(), String::new()),
            ]
        );
        assert_eq!(
            fixture_files("static a = 1;"),
            [("test.must".to_owned(), "static a = 1;".to_owned())]
        );
    }

    #[test]
    fn database_clones_share_memoized_results() {
        let (db, executions) = counting_db();
        let file = SourceFile::new(&db, "test.must".to_owned(), "static a = 1;".to_owned());

        let snapshot = db.clone();
        let _ = parse(&snapshot, file);
        let _ = parse(&db, file);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
    }
}
