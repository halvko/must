//! Which files a command-line tool loads: the file it was given and, when a
//! `must.toml` sits at or above it, every file of that library.
//!
//! The nearest `must.toml` at or above a file marks its library's root, the
//! directory `use` paths start from. A `must.toml` inside another library's
//! tree starts a new library, so no file belongs to two. A file with none
//! above it is standalone: a library of one file, with nothing to import.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base_db::{RootDatabase, SourceFile};

/// The file that marks a library's root.
pub const MANIFEST: &str = "must.toml";

/// The root of the library `file` belongs to: the nearest directory at or
/// above it that holds a `must.toml`. `file` is absolute.
pub fn find_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
}

/// Every `.must` file of the library rooted at `root`, sorted. The walk
/// does not enter a directory holding its own `must.toml` (another
/// library), a hidden directory (`.git`), or a symbolic link to a
/// directory (which could loop). `.gitignore` is not honoured.
pub fn library_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if !hidden && !path.join(MANIFEST).is_file() {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "must") && path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Why a `must.toml` is refused: it has no keys, so any key or table is
/// unknown. Refusing them means a key added to the format never has to
/// guess what an old manifest meant by it.
pub fn check_manifest(text: &str) -> Result<(), String> {
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let what = match line.strip_prefix('[') {
            Some(table) => format!("table `[{}`", table.split('#').next().unwrap_or("").trim()),
            None => format!("key `{}`", line.split('=').next().unwrap_or("").trim()),
        };
        return Err(format!("unknown {what}: `{MANIFEST}` has no keys"));
    }
    Ok(())
}

/// The files a tool loaded, and how to show each to the user.
#[derive(Default)]
pub struct Sources {
    /// Each file's display path and its text as read from disk.
    files: HashMap<SourceFile, (String, String)>,
    /// Each library file by its absolute path, so a library loads once.
    by_path: HashMap<PathBuf, SourceFile>,
}

impl Sources {
    /// Loads the file at `path` into `db` with its library, if it has one,
    /// and returns it. `edit` turns the file's text into what is analyzed
    /// (`must run` appends its entry item); the other files are analyzed as
    /// read. A file whose library is already loaded is returned as loaded.
    ///
    /// Errors come back rendered: a file that cannot be read, or a
    /// `must.toml` that is refused.
    pub fn load(
        &mut self,
        db: &mut RootDatabase,
        path: &str,
        edit: impl FnOnce(&str) -> String,
    ) -> Result<SourceFile, String> {
        let read = |path: &Path| {
            std::fs::read_to_string(path)
                .map_err(|err| format!("error: cannot read `{}`: {err}", path.display()))
        };
        let absolute = std::fs::canonicalize(path)
            .map_err(|err| format!("error: cannot read `{path}`: {err}"))?;
        let Some(root) = find_root(&absolute) else {
            let text = read(Path::new(path))?;
            let file = base_db::new_standalone(db, path.to_owned(), edit(&text));
            self.files.insert(file, (path.to_owned(), text));
            return Ok(file);
        };
        if let Some(&file) = self.by_path.get(&absolute) {
            return Ok(file);
        }
        let manifest = root.join(MANIFEST);
        check_manifest(&read(&manifest)?)
            .map_err(|err| format!("error: {}: {err}", display(&manifest)))?;
        let paths = library_files(&root)
            .map_err(|err| format!("error: cannot read `{}`: {err}", root.display()))?;
        let Some(main) = paths.iter().position(|p| *p == absolute) else {
            return Err(format!(
                "error: `{path}` is not a module of the library at `{}` \
                 (hidden directories are not part of it, and modules end in `.must`)",
                display(&root)
            ));
        };
        let texts = (paths.iter().map(|p| read(p))).collect::<Result<Vec<_>, _>>()?;
        let mut inputs: Vec<(String, String)> = (paths.iter().zip(&texts))
            .map(|(p, text)| (p.to_string_lossy().into_owned(), text.clone()))
            .collect();
        inputs[main].1 = edit(&texts[main]);
        let files = base_db::new_library(db, &root.to_string_lossy(), inputs);
        for (i, ((file_path, text), &file)) in paths.into_iter().zip(texts).zip(&files).enumerate()
        {
            let shown = if i == main {
                path.to_owned()
            } else {
                display(&file_path)
            };
            self.files.insert(file, (shown, text));
            self.by_path.insert(file_path, file);
        }
        Ok(files[main])
    }

    /// Records `file`, which a tool made from `text` without [`Sources::load`]
    /// and shows to the user as `display`.
    pub fn insert(&mut self, file: SourceFile, display: String, text: String) {
        self.files.insert(file, (display, text));
    }

    /// How `file` is shown to the user: as it was named for the file a tool
    /// was given, else relative to the working directory where it can be.
    /// `None` for a file no tool loaded (the prelude).
    pub fn display(&self, file: SourceFile) -> Option<&str> {
        self.files.get(&file).map(|(path, _)| path.as_str())
    }

    /// `file`'s text as read, before any edit; `None` as for
    /// [`Sources::display`].
    pub fn text(&self, file: SourceFile) -> Option<&str> {
        self.files.get(&file).map(|(_, text)| text.as_str())
    }
}

/// `path` relative to the working directory, if it is below it.
fn display(path: &Path) -> String {
    let relative = std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf));
    relative.as_deref().unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_has_no_keys() {
        assert_eq!(check_manifest(""), Ok(()));
        assert_eq!(
            check_manifest(
                r#"
# a comment

"#
            ),
            Ok(())
        );
        assert_eq!(
            check_manifest(
                r#"
name = "aoc"
"#
            ),
            Err("unknown key `name`: `must.toml` has no keys".to_owned())
        );
        assert_eq!(
            check_manifest(
                r#"
[dependencies] # later
"#
            ),
            Err("unknown table `[dependencies]`: `must.toml` has no keys".to_owned())
        );
    }

    #[test]
    fn the_walk_stops_at_nested_libraries_and_hidden_directories() {
        let root = std::env::temp_dir().join(format!("must-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for path in [
            "must.toml",
            "main.must",
            "notes.txt",
            "a/b.must",
            "a/c/d.must",
            "inner/must.toml",
            "inner/lib.must",
            ".git/x.must",
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let files: Vec<String> = (library_files(&root).unwrap().iter())
            .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
            .collect();
        assert_eq!(files, ["a/b.must", "a/c/d.must", "main.must"]);
        assert_eq!(find_root(&root.join("a/c/d.must")), Some(root.clone()));
        assert_eq!(
            find_root(&root.join("inner/lib.must")),
            Some(root.join("inner"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
