//! The item tree: a range-free summary of a file's top-level items.
//!
//! Because it carries no text positions, this value compares equal across
//! edits that only touch bodies or whitespace — salsa then backdates it and
//! nothing downstream of name-level information re-runs.

use base_db::{Db, SourceFile, parse};
use syntax::ast;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ItemTree {
    pub items: Vec<ItemData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemData {
    /// Empty string when the name is missing (broken code); the
    /// disambiguator in [`crate::ItemId`] still keeps identity stable.
    pub name: String,
    pub is_const: bool,
    pub type_ref: Option<TypeRef>,
}

/// Syntax-free representation of a type annotation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Unit,
    Never,
    Fn {
        params: Vec<TypeRef>,
        ret: Option<Box<TypeRef>>,
    },
    Ref(Box<TypeRef>),
    Path(String),
    Hole,
    Error,
}

impl TypeRef {
    pub fn from_ast(ty: ast::Type) -> TypeRef {
        match ty {
            ast::Type::UnitType(_) => TypeRef::Unit,
            ast::Type::NeverType(_) => TypeRef::Never,
            ast::Type::FnType(it) => TypeRef::Fn {
                params: it.param_types().map(TypeRef::from_ast).collect(),
                ret: it
                    .ret_type()
                    .and_then(|rt| rt.ty())
                    .map(|t| Box::new(TypeRef::from_ast(t))),
            },
            ast::Type::RefType(it) => match it.ty() {
                Some(inner) => TypeRef::Ref(Box::new(TypeRef::from_ast(inner))),
                None => TypeRef::Error,
            },
            ast::Type::PathType(it) => match it.name_ref() {
                Some(name) => TypeRef::Path(name.text()),
                None => TypeRef::Error,
            },
            ast::Type::HoleType(_) => TypeRef::Hole,
        }
    }

    pub fn from_opt_ast(ty: Option<ast::Type>) -> Option<TypeRef> {
        ty.map(TypeRef::from_ast)
    }
}

#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    let tree = parse(db, file).tree();
    let items = tree
        .items()
        .map(|item| ItemData {
            name: item.name().map(|n| n.text()).unwrap_or_default(),
            is_const: item.is_const(),
            type_ref: TypeRef::from_opt_ast(item.ty()),
        })
        .collect();
    ItemTree { items }
}

/// The syntax node for `item`, looked up by (name, disambiguator). Range
/// information enters here and only here; callers that want to stay in the
/// firewall must not feed this back into range-free queries.
pub fn item_source(db: &dyn Db, item: crate::ItemId<'_>) -> Option<ast::StaticItem> {
    let file = item.file(db);
    let tree = parse(db, file).tree();
    let name = item.name(db);
    let mut seen = 0;
    for ast_item in tree.items() {
        let item_name = ast_item.name().map(|n| n.text()).unwrap_or_default();
        if item_name == *name {
            if seen == item.disambiguator(db) {
                return Some(ast_item);
            }
            seen += 1;
        }
    }
    None
}
