//! A file's library: the files of one directory tree. Each file is a module,
//! its own namespace, and sees another module's items only through a `use`
//! item naming it by its path from the library's root; inference, the
//! trait-impl index and coherence span the library.

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

use base_db::{Db, Library, SourceFile, parse};
use syntax::ast::{self, AstNode as _};

use crate::item_tree::{ItemData, ItemKind, item_tree};
use crate::{ItemId, ItemLoc, file_item_ids, item_loc};

/// What identifies a library: its [`Library`] input, or the file itself for a
/// file in no library. Stable while files join and leave a library, so the
/// library-wide queries keyed on it re-run on a membership change rather than
/// starting over under a new key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum LibraryKey {
    Library(Library),
    Lone(SourceFile),
}

/// The key of every library-wide query.
#[salsa::interned(debug)]
pub struct LibraryId<'db> {
    pub key: LibraryKey,
}

impl<'db> LibraryId<'db> {
    /// The library's files, in the library's order.
    pub fn files(self, db: &'db dyn Db) -> Vec<SourceFile> {
        match self.key(db) {
            LibraryKey::Library(library) => library.files(db).clone(),
            LibraryKey::Lone(file) => vec![file],
        }
    }
}

/// The library `file` belongs to.
pub fn library_of(db: &dyn Db, file: SourceFile) -> LibraryId<'_> {
    let key = match file.library(db) {
        Some(library) => LibraryKey::Library(library),
        None => LibraryKey::Lone(file),
    };
    LibraryId::new(db, key)
}

/// Every top-level item of `library`, file by file.
#[salsa::tracked(returns(ref))]
pub fn library_item_ids<'db>(db: &'db dyn Db, library: LibraryId<'db>) -> Vec<ItemId<'db>> {
    library
        .files(db)
        .into_iter()
        .flat_map(|file| file_item_ids(db, file).iter().copied())
        .collect()
}

/// What a file imports: each `use`'s path error, and each imported name in
/// source order (the order of [`imported_names`] over
/// [`ast::SourceFile::use_items`]). Range-free, so body edits backdate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Imports {
    /// One per `use`, in source order: why its path names no module, if it
    /// names none.
    pub paths: Vec<Option<PathError>>,
    pub names: Vec<Import>,
}

/// One imported name: what the importing file calls it, and what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The `as` rename, or the imported name itself.
    pub name: String,
    pub target: ImportTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportTarget {
    Item {
        loc: ItemLoc,
        kind: ItemKind,
    },
    /// The module exists but declares no top-level item of that name.
    NoItem {
        path: String,
        name: String,
    },
    /// The module declares more than one top-level item of that name: the
    /// import is an error, and the name is ambiguous in the importer.
    /// `items` is every declaration, in source order.
    Ambiguous {
        path: String,
        name: String,
        items: Vec<(ItemLoc, ItemKind)>,
    },
    /// The `use`'s path names no module; [`Imports::paths`] says why.
    NoPath,
}

/// Why a `use`'s path names no module of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The parse lost the path; the parse error says so.
    Broken,
    /// No module of the library has the path the segments spell.
    NoModule { path: String },
}

#[salsa::tracked(returns(ref))]
pub fn file_imports(db: &dyn Db, file: SourceFile) -> Imports {
    let root = ast::SourceFile::cast(parse(db, file).syntax_node()).expect("a source file");
    let mut imports = Imports::default();
    for use_item in root.use_items() {
        let segments: Vec<String> = use_item.segments().map(|s| s.text().to_string()).collect();
        let path = match segments.as_slice() {
            [] => Err(PathError::Broken),
            _ => imported_file(db, file, &segments).ok_or_else(|| PathError::NoModule {
                path: segments.join("::"),
            }),
        };
        for (use_name, name_ref) in imported_names(&use_item) {
            let imported = name_ref.text().to_string();
            let name = use_name
                .rename()
                .map(|rename| rename.text().to_string())
                .unwrap_or_else(|| imported.clone());
            let target = match &path {
                Ok(target_file) => match top_level_items(db, *target_file, &imported).as_slice() {
                    [(loc, kind)] => ImportTarget::Item {
                        loc: loc.clone(),
                        kind: *kind,
                    },
                    [] => ImportTarget::NoItem {
                        path: segments.join("::"),
                        name: imported,
                    },
                    items => ImportTarget::Ambiguous {
                        path: segments.join("::"),
                        name: imported,
                        items: items.to_vec(),
                    },
                },
                Err(_) => ImportTarget::NoPath,
            };
            imports.names.push(Import { name, target });
        }
        imports.paths.push(path.err());
    }
    imports
}

/// The names `use_item` imports that the parse kept, each with the name
/// it reads in the imported file: the order [`Imports::names`] follows.
pub fn imported_names(
    use_item: &ast::UseItem,
) -> impl Iterator<Item = (ast::UseName, ast::NameRef)> + use<> {
    use_item
        .names()
        .filter_map(|use_name| Some((use_name.clone(), use_name.name_ref()?)))
}

/// The file of `file`'s library that `segments` name. A path is read from
/// the library's root (`use a::b::x` names `root/a/b.must`, from any file),
/// or from the importer when it starts with `self` (its children) or with
/// `super`s (each one module up, a root module's parent being the library).
/// A file in no library has no module to import.
fn imported_file(db: &dyn Db, file: SourceFile, segments: &[String]) -> Option<SourceFile> {
    let library = file.library(db)?;
    let root = Path::new(library.root(db));
    let mut wanted = root.to_path_buf();
    let mut rest = segments;
    if let [first, ..] = segments
        && (first == "self" || first == "super")
    {
        // The importer's own module: its path without the extension, the
        // directory that holds its children.
        wanted = Path::new(file.path(db)).with_extension("");
        if !wanted.starts_with(root) {
            return None;
        }
        if first == "self" {
            rest = &segments[1..];
        }
        while let [first, tail @ ..] = rest
            && first == "super"
        {
            if wanted == root {
                return None;
            }
            wanted.pop();
            rest = tail;
        }
    }
    for segment in rest {
        wanted.push(segment);
    }
    wanted.set_extension("must");
    library_modules(db, library).get(&wanted).copied()
}

/// Each file of `library` by its path. Paths never change with an edit, so
/// this backdates across every body edit.
#[salsa::tracked(returns(ref))]
fn library_modules(db: &dyn Db, library: Library) -> FxHashMap<PathBuf, SourceFile> {
    (library.files(db).iter())
        .map(|&file| (PathBuf::from(file.path(db)), file))
        .collect()
}

/// Whether `use` can import the item: a named top-level `static`, `const`,
/// `type` or `trait`, never a type's member.
fn importable(data: &ItemData) -> bool {
    !data.name.is_empty() && !matches!(data.kind, ItemKind::Member)
}

/// Every importable item named `name` in `file`.
fn top_level_items(db: &dyn Db, file: SourceFile, name: &str) -> Vec<(ItemLoc, ItemKind)> {
    let ids = file_item_ids(db, file).iter();
    ids.zip(item_tree(db, file).items.iter())
        .filter(|(_, data)| data.name == name && importable(data))
        .map(|(&id, data)| (item_loc(db, id), data.kind))
        .collect()
}

/// An item `use` can import, and the path of the module that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub module: String,
    pub loc: ItemLoc,
    pub kind: ItemKind,
}

/// Every importable item of `library`, by its name.
#[salsa::tracked(returns(ref))]
pub fn library_declarations<'db>(
    db: &'db dyn Db,
    library: LibraryId<'db>,
) -> FxHashMap<String, Vec<Declaration>> {
    let mut declarations: FxHashMap<String, Vec<Declaration>> = FxHashMap::default();
    for (loc, data) in crate::scopes::library_items(db, library) {
        if !importable(data) {
            continue;
        }
        let Some(module) = module_path(db, loc.file) else {
            continue;
        };
        (declarations.entry(data.name.clone()).or_default()).push(Declaration {
            module,
            loc,
            kind: data.kind,
        });
    }
    declarations
}

/// The path a `use` names `file` by: its path from the library's root,
/// without the extension (`root/a/b.must` is `a::b`). A file in no library
/// has none.
pub fn module_path(db: &dyn Db, file: SourceFile) -> Option<String> {
    let library = file.library(db)?;
    let path = Path::new(file.path(db))
        .strip_prefix(library.root(db))
        .ok()?
        .with_extension("");
    let segments: Option<Vec<&str>> = path.iter().map(|segment| segment.to_str()).collect();
    Some(segments?.join("::"))
}
