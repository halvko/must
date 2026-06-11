//! Salsa database core: inputs and the `parse` query.
//!
//! Everything downstream (hir, ide) computes via tracked queries over these
//! inputs; the LSP layer mutates inputs (which bumps the revision and cancels
//! in-flight queries on database clones) and reads queries off clones.

#[salsa::input]
pub struct SourceFile {
    /// Identity/debugging only — the `Uri ↔ SourceFile` map is server state,
    /// not part of the database.
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
}

#[salsa::db]
pub trait Db: salsa::Database {}

#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
}

impl RootDatabase {
    /// A database that reports salsa events to `callback` (used by tests to
    /// assert what does and does not get recomputed).
    pub fn with_event_callback(
        callback: Box<dyn Fn(salsa::Event) + Send + Sync + 'static>,
    ) -> Self {
        RootDatabase {
            storage: salsa::Storage::new(Some(callback)),
        }
    }
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl Db for RootDatabase {}

#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn Db, file: SourceFile) -> syntax::Parse {
    syntax::parse(file.text(db))
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
    fn database_clones_share_memoized_results() {
        let (db, executions) = counting_db();
        let file = SourceFile::new(&db, "test.must".to_owned(), "static a = 1;".to_owned());

        let snapshot = db.clone();
        let _ = parse(&snapshot, file);
        let _ = parse(&db, file);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
    }
}
