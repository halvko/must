//! "Insert inferred signature": writes what inference found for an item
//! that is not fully annotated into its source, so it becomes a firewall
//! (T11). Only what is missing is written: a `_` is replaced, a missing
//! parameter or return type inserted, and written parts are kept as is.

use base_db::{Db, SourceFile};
use syntax::ast::{self, AstNode};
use syntax::{SyntaxKind, TextEdit, TextRange, TextSize};

use crate::{ItemId, Ty};

/// The fix writing `item`'s inferred signature, its edits all in `item`'s
/// own file; `None` when the item's signature is not inference's (see
/// [`inferrable_source`]) or inference found nothing to write. With
/// `offered_at`, also `None` unless that range is on the item's head.
pub fn insert_inferred_signature(
    db: &dyn Db,
    item: ItemId<'_>,
    offered_at: Option<TextRange>,
) -> Option<syntax::Fix> {
    let source = inferrable_source(db, item)?;
    if let Some(range) = offered_at {
        let head = head_range(&source);
        // A cursor at either end of the head is on it; a selection has to
        // overlap it, not just touch it.
        if !head.contains_range(range) && head.intersect(range).is_none_or(|it| it.is_empty()) {
            return None;
        }
    }
    let sig = crate::signature(db, item);
    let writer = Writer {
        db,
        file: item.file(db),
        edits: Vec::new(),
    };
    let edits = writer.write(&source, &sig)?;
    let label = if matches!(sig, Ty::Fn(_)) {
        "Insert inferred signature"
    } else {
        "Insert inferred type"
    };
    Some(syntax::Fix {
        label: label.to_owned(),
        edits,
    })
}

/// The item's head, where the action is offered: from its start up to its
/// value, or for a fn literal up to the literal's body. Not the body, where
/// the action would crowd every other one.
fn head_range(source: &ast::StaticItem) -> TextRange {
    let start = source.syntax().text_range().start();
    let end = match source.body() {
        Some(ast::Expr::FnLiteral(literal)) => literal
            .body()
            .map_or(literal.syntax().text_range().end(), |body| {
                body.syntax().text_range().start()
            }),
        Some(body) => body.syntax().text_range().start(),
        None => source.syntax().text_range().end(),
    };
    TextRange::new(start, end)
}

/// `item`'s source when its signature comes from inference: a top-level,
/// non-generic, non-`extern` value item with a value and no complete
/// contract. Generic items and members are fully annotated by rule, and an
/// `extern` item is nothing but its annotation.
fn inferrable_source(db: &dyn Db, item: ItemId<'_>) -> Option<ast::StaticItem> {
    if item.member(db).is_some() || crate::is_extern(db, item) {
        return None;
    }
    let data = crate::item_data(db, item).as_ref()?;
    if !matches!(data.kind, crate::ItemKind::Value(_)) || !data.generics.is_empty() {
        return None;
    }
    if data.type_ref.as_ref().is_some_and(|it| it.is_fully_typed()) {
        return None;
    }
    crate::body::body(db, item).root?;
    match crate::item_source(db, item)? {
        ast::Item::StaticItem(source) => Some(source),
        _ => None,
    }
}

/// The written head of an fn literal or an fn type: what [`Writer::fill_fn`]
/// fills.
struct FnHead {
    /// The end of the `fn` keyword.
    fn_end: TextSize,
    params: Option<ast::ParamList>,
    ret: Option<ast::RetType>,
}

impl FnHead {
    fn of_literal(literal: &ast::FnLiteral) -> FnHead {
        FnHead::new(literal.syntax(), literal.param_list(), literal.ret_type())
    }

    fn of_type(ty: &ast::FnType) -> FnHead {
        FnHead::new(ty.syntax(), ty.param_list(), ty.ret_type())
    }

    fn new(
        node: &syntax::SyntaxNode,
        params: Option<ast::ParamList>,
        ret: Option<ast::RetType>,
    ) -> FnHead {
        let fn_end = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::FN_KW)
            .map_or(node.text_range().start(), |it| it.text_range().end());
        FnHead {
            fn_end,
            params,
            ret,
        }
    }
}

struct Writer<'a> {
    db: &'a dyn Db,
    file: SourceFile,
    edits: Vec<TextEdit>,
}

impl Writer<'_> {
    /// The edits for `source`, `None` when there are none.
    fn write(mut self, source: &ast::StaticItem, sig: &Ty) -> Option<Vec<TextEdit>> {
        if let Some(annotation) = source.ty() {
            // The item-level annotation is the contract whenever it is
            // written; the literal's own annotations are not consulted.
            self.fill(&annotation, sig);
        } else if let (Some(ast::Expr::FnLiteral(literal)), Ty::Fn(sig)) = (source.body(), sig) {
            // The literal's type is the signature, so its head lines up.
            self.fill_fn(FnHead::of_literal(&literal), sig, true);
        } else {
            let at = source.name()?.syntax().text_range().end();
            self.write_at(TextRange::empty(at), ": ", sig);
        }
        (!self.edits.is_empty()).then_some(self.edits)
    }

    /// Replace `range` with `prefix` and `ty`'s spelling, unless it is a
    /// bare `_`: that writes nothing inference found.
    fn write_at(&mut self, range: TextRange, prefix: &str, ty: &Ty) {
        let text = crate::annotation_text(self.db, self.file, ty);
        if text != "_" {
            self.edits.push(TextEdit {
                range,
                insert: format!("{prefix}{text}"),
            });
        }
    }

    fn insert(&mut self, at: TextSize, prefix: &str, ty: &Ty) {
        self.write_at(TextRange::empty(at), prefix, ty);
    }

    /// [`Self::fill`] under a written postfix `.&raw` or `.&`, where a `_`
    /// stays when its type can't be spelled there.
    fn fill_pointee(&mut self, written: &ast::Type, ty: &Ty) {
        match (written, ty) {
            (ast::Type::HoleType(_), _) if !crate::ty::spells_before_postfix(ty) => {}
            (ast::Type::FnType(written), Ty::Fn(sig)) => {
                self.fill_fn(FnHead::of_type(written), sig, false)
            }
            _ => self.fill(written, ty),
        }
    }

    /// Fill the holes of a written type with the matching parts of `ty`.
    /// A written type lowers part by part, so `ty` has its shape; a part
    /// that lowered to an error has nothing to fill.
    fn fill(&mut self, written: &ast::Type, ty: &Ty) {
        match (written, ty) {
            (ast::Type::HoleType(hole), _) => self.write_at(hole.syntax().text_range(), "", ty),
            (ast::Type::FnType(written), Ty::Fn(sig)) => {
                self.fill_fn(FnHead::of_type(written), sig, true)
            }
            (ast::Type::PathType(written), Ty::Named(named)) => {
                let args = written
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|it| it.args());
                for (arg, value) in args.zip(&named.args) {
                    if let (ast::GenericArg::TypeArg(arg), crate::GenericArg::Ty(ty)) = (arg, value)
                        && let Some(arg) = arg.ty()
                    {
                        self.fill(&arg, ty);
                    }
                }
            }
            (ast::Type::RawPtrType(written), Ty::RawPtr { pointee, .. }) => {
                if let Some(inner) = written.ty() {
                    self.fill_pointee(&inner, pointee);
                }
            }
            (ast::Type::BorrowType(written), Ty::Borrow { referent, .. }) => {
                if let Some(inner) = written.ty() {
                    self.fill_pointee(&inner, referent);
                }
            }
            (ast::Type::ArrayType(written), Ty::Array { elem, .. }) => {
                if let Some(inner) = written.ty() {
                    self.fill(&inner, elem);
                }
            }
            (ast::Type::RecordType(written), Ty::Record(record)) => {
                for field in written.fields() {
                    if let (Some(name), Some(inner)) = (field.name(), field.ty())
                        && let Some(ty) = record.field_ty(&name.text())
                    {
                        self.fill(&inner, ty);
                    }
                }
            }
            _ => {}
        }
    }

    /// Fill a written fn head, a literal's or an fn type's: each parameter
    /// type and the return type are filled where written and inserted where
    /// missing. With `with_ret` false a missing return type stays missing:
    /// in front of a postfix `.&raw` or `.&` it would take the postfix.
    fn fill_fn(&mut self, head: FnHead, sig: &crate::ty::FnTy, with_ret: bool) {
        let params = head.params.iter().flat_map(ast::ParamList::params);
        for (param, ty) in params.zip(&sig.params) {
            match (param.ty(), param.pat()) {
                (Some(written), _) => self.fill(&written, ty),
                (None, Some(pat)) => self.insert(pat.syntax().text_range().end(), ": ", ty),
                (None, None) => {}
            }
        }
        match (head.ret, head.params) {
            (Some(ret), _) => {
                if let Some(written) = ret.ty() {
                    self.fill(&written, &sig.ret);
                }
            }
            _ if !with_ret => {}
            (None, Some(list)) => self.insert(list.syntax().text_range().end(), " -> ", &sig.ret),
            // `fn { ... }`: the return type needs the empty list in front.
            (None, None) => self.insert(head.fn_end, " () -> ", &sig.ret),
        }
    }
}
