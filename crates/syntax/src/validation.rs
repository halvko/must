//! Checks for trees the grammar deliberately over-accepts.
//!
//! Parsing a superset keeps the user's intent in the tree (so inference,
//! hover and friends work on broken code) and gives errors found here
//! exactly the information a quick fix needs.

use crate::ast::{self, AstNode};
use crate::{BRACE_RULE, Fix, SyntaxError, SyntaxNode, SyntaxToken, TextEdit};
use text_size::TextRange;

pub(crate) fn validate(root: &SyntaxNode) -> Vec<SyntaxError> {
    let mut errors = Vec::new();
    for node in root.descendants() {
        if let Some(fn_literal) = ast::FnLiteral::cast(node.clone()) {
            // The parser reports "expected `{`" itself when the body is
            // absent entirely.
            if let Some(body) = fn_literal.body() {
                require_block(&body, BRACE_RULE, &mut errors);
            }
        } else if let Some(if_expr) = ast::IfExpr::cast(node.clone()) {
            if let Some(then) = if_expr.then_branch() {
                require_block(&then, "`if` branches are blocks", &mut errors);
            }
            // `else if` chains: the nested IfExpr validates itself.
            if let Some(els) = if_expr.else_branch()
                && !matches!(els, ast::Expr::IfExpr(_))
            {
                require_block(&els, "`else` branches are blocks", &mut errors);
            }
        } else if let Some(loop_expr) = ast::LoopExpr::cast(node.clone()) {
            // Same superset as `if` branches: any expression parses as the
            // body, only blocks are legal.
            if let Some(body) = loop_expr.body() {
                require_block(&body, "`loop` bodies are blocks", &mut errors);
            }
        } else if let Some(assign) = ast::AssignStmt::cast(node.clone()) {
            if let Some(lhs) = assign.lhs() {
                require_variable_target(&lhs, &mut errors);
            }
        } else if let Some(let_stmt) = ast::LetStmt::cast(node.clone()) {
            require_mut_names_a_binding(let_stmt.mut_token(), let_stmt.pat(), &mut errors);
        } else if let Some(record_ty) = ast::RecordType::cast(node.clone()) {
            let names = record_ty
                .fields()
                .filter_map(|f| f.name())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicate_fields(names, &mut errors);
            reject_open_record(record_ty.dot3_token(), &mut errors);
            for field in record_ty.fields() {
                reject_pub_field(field.pub_token(), &mut errors);
            }
        } else if let Some(record_expr) = ast::RecordExpr::cast(node.clone()) {
            let names = record_expr
                .fields()
                .filter_map(|f| f.name_ref())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicate_fields(names, &mut errors);
            reject_open_record(record_expr.dot3_token(), &mut errors);
            for field in record_expr.fields() {
                reject_pub_field(field.pub_token(), &mut errors);
            }
        } else if let Some(type_item) = ast::TypeItem::cast(node.clone()) {
            reject_type_item_annotation(&type_item, &mut errors);
        } else if let Some(enum_expr) = ast::EnumExpr::cast(node.clone()) {
            let names = enum_expr
                .variants()
                .filter_map(|v| v.name())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicates(names, "variant", &mut errors);
            require_enum_declares_a_type(&enum_expr, &mut errors);
        } else if let Some(rest) = ast::RestPat::cast(node.clone()) {
            // `..` in a record pattern means "ignore the remaining fields" —
            // legal. Everywhere else a pattern can appear (a variant
            // pattern's positional payload, in v1) it stays reserved: it
            // parses, but is rejected here.
            if !rest
                .syntax()
                .parent()
                .is_some_and(|p| ast::RecordPat::can_cast(p.kind()))
            {
                errors.push(SyntaxError {
                    message: "`..` in patterns is not supported yet".to_owned(),
                    range: rest.syntax().text_range(),
                    fix: None,
                });
            }
        } else if let Some(record_pat) = ast::RecordPat::cast(node.clone()) {
            let names = record_pat
                .fields()
                .filter_map(|f| f.field_name())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicate_fields(names, &mut errors);
        } else if let Some(variant_pat) = ast::VariantPat::cast(node.clone()) {
            reject_unqualified_bare_variant_pat(&variant_pat, &mut errors);
        } else if let Some(param) = ast::Param::cast(node) {
            require_mut_names_a_binding(param.mut_token(), param.pat(), &mut errors);
        }
    }
    errors
}

/// Report every repeat of a field name after its first occurrence. Empty
/// names (missing in broken code) are ignored so they never collide.
fn report_duplicate_fields(
    fields: impl Iterator<Item = (String, TextRange)>,
    errors: &mut Vec<SyntaxError>,
) {
    report_duplicates(fields, "field", errors);
}

/// Report every repeat of a name after its first occurrence — record fields
/// and enum variants share the rule. Empty names (missing in broken code)
/// are ignored so they never collide.
fn report_duplicates(
    names: impl Iterator<Item = (String, TextRange)>,
    what: &str,
    errors: &mut Vec<SyntaxError>,
) {
    let mut seen = std::collections::HashSet::new();
    for (name, range) in names {
        if name.is_empty() {
            continue;
        }
        if !seen.insert(name.clone()) {
            errors.push(SyntaxError {
                message: format!("duplicate {what} `{name}`"),
                range,
                fix: None,
            });
        }
    }
}

/// An `enum` literal declares a type and nothing else: the grammar parses it
/// as an expression (so a misplaced one keeps its shape in the tree), but
/// the only legal position is directly as a `type` item's value.
fn require_enum_declares_a_type(enum_expr: &ast::EnumExpr, errors: &mut Vec<SyntaxError>) {
    if enum_expr
        .syntax()
        .parent()
        .is_some_and(|p| ast::TypeItem::can_cast(p.kind()))
    {
        return;
    }
    errors.push(SyntaxError {
        message: "an `enum` literal can only appear as a `type` declaration's value".to_owned(),
        range: enum_expr.syntax().text_range(),
        fix: None,
    });
}

/// `...` in a record type or literal parses (reserving the syntax) but is
/// always rejected: open records are not supported yet.
fn reject_open_record(dot3: Option<SyntaxToken>, errors: &mut Vec<SyntaxError>) {
    let Some(dot3) = dot3 else {
        return;
    };
    errors.push(SyntaxError {
        message: "open record types are not supported yet".to_owned(),
        range: dot3.text_range(),
        fix: None,
    });
}

/// The message for a non-variable assignment target. A `pub const` because
/// `mir` traps such targets with exactly the squiggle's text (the
/// single-render rule of `hir::diag`, except this message originates here
/// in validation rather than in a semantic analysis).
pub const CAN_ONLY_ASSIGN_TO_A_VARIABLE: &str = "can only assign to a variable";

/// The message for a field-assignment target (`p.x = 1;`). Same sharing
/// contract as [`CAN_ONLY_ASSIGN_TO_A_VARIABLE`]: `mir` traps a field target
/// with exactly this text.
pub const CANNOT_ASSIGN_TO_A_FIELD: &str = "assigning to a field is not supported yet";

/// The grammar superset-parses any expression as an assignment's LHS;
/// reject anything but a plain variable. A field access gets its own honest
/// message (it *is* a place, structurally — just not one assignment
/// supports yet) rather than the generic "not a variable" wording.
fn require_variable_target(expr: &ast::Expr, errors: &mut Vec<SyntaxError>) {
    let message = match expr {
        ast::Expr::PathExpr(_) => return,
        ast::Expr::FieldExpr(_) => CANNOT_ASSIGN_TO_A_FIELD,
        _ => CAN_ONLY_ASSIGN_TO_A_VARIABLE,
    };
    errors.push(SyntaxError {
        message: message.to_owned(),
        range: expr.syntax().text_range(),
        fix: None,
    });
}

/// `mut` on a hole pattern (`let mut _ = ...` / `fn (mut _: T)`) has nothing
/// to act on: a hole binds no name, so it can never be the target of an
/// assignment. Offers a fix that drops the redundant `mut` (and the
/// whitespace between it and `_`) rather than making the user hand-edit.
///
/// `mut` on a destructuring pattern (`let mut struct { x } = ...`) is a
/// different mistake: `mut` binds per-field inside the pattern (`let struct
/// { mut x } = ...`), not to the pattern as a whole — flagged with its own
/// message, no fix (there's no single field to move it to).
fn require_mut_names_a_binding(
    mut_token: Option<SyntaxToken>,
    pat: Option<ast::Pat>,
    errors: &mut Vec<SyntaxError>,
) {
    let Some(mut_token) = mut_token else {
        return;
    };
    match pat {
        Some(ast::Pat::BindPat(bind)) => {
            let Some(name) = bind.name() else {
                return;
            };
            if !name.is_hole() {
                return;
            }
            errors.push(SyntaxError {
                message: "`mut` has no effect on `_`: a hole can never be assigned".to_owned(),
                range: mut_token.text_range().cover(name.syntax().text_range()),
                fix: Some(Fix {
                    label: "Remove `mut`".to_owned(),
                    edits: vec![TextEdit {
                        range: TextRange::new(
                            mut_token.text_range().start(),
                            name.syntax().text_range().start(),
                        ),
                        insert: String::new(),
                    }],
                }),
            });
        }
        Some(other) => {
            errors.push(SyntaxError {
                message: "`mut` applies to individual bindings in a destructuring pattern"
                    .to_owned(),
                range: mut_token.text_range().cover(other.syntax().text_range()),
                fix: None,
            });
        }
        None => {}
    }
}

/// `pub` superset-parses on any record-type/record-literal field (reserving
/// the syntax for both `type Foo = struct { pub x: usize };` declarations
/// and inline record-type annotations); field visibility isn't supported
/// yet, so it is always rejected — a record *literal*'s fields have no
/// declaration to attach visibility to in the first place, but the message
/// is the same honest "not yet" either way.
fn reject_pub_field(pub_token: Option<SyntaxToken>, errors: &mut Vec<SyntaxError>) {
    let Some(pub_token) = pub_token else {
        return;
    };
    errors.push(SyntaxError {
        message: "field visibility is not supported yet".to_owned(),
        range: pub_token.text_range(),
        fix: None,
    });
}

/// The grammar superset-parses a `: Type` annotation on a `type` item (the
/// item shape is shared with `static`/`const`); reject it — a `type`
/// declaration *is* the type, there is nothing to annotate — with a fix
/// that drops the annotation.
fn reject_type_item_annotation(type_item: &ast::TypeItem, errors: &mut Vec<SyntaxError>) {
    let Some(colon) = type_item.colon_token() else {
        return;
    };
    let end = type_item
        .ty()
        .map(|ty| ty.syntax().text_range().end())
        .unwrap_or_else(|| colon.text_range().end());
    let range = TextRange::new(colon.text_range().start(), end);
    errors.push(SyntaxError {
        message: "a `type` declaration takes no type annotation".to_owned(),
        range,
        fix: Some(Fix {
            label: "Remove the annotation".to_owned(),
            edits: vec![TextEdit {
                range,
                insert: String::new(),
            }],
        }),
    });
}

/// `Circle(r)` with no leading `::` and no qualifying enum used to be a
/// variant pattern (v1's bare shorthand); the owner retired it alongside
/// removing bind-pattern reinterpretation (`Circle` gains this shorthand
/// back the moment some *other* enum happens to declare a `Circle`
/// variant, and there'd be no way to tell which one the pattern meant) —
/// so an unmarked `Name(...)` is now an honest mistake, not a call
/// (patterns have no calls). Offers a fix that inserts the elided `::`.
///
/// `colon2_token().is_none()` reaches here only via the bare-`Name(...)`
/// grammar path: the qualified and elided sigil spellings both carry a
/// `COLON2`, so this cannot misfire on them.
fn reject_unqualified_bare_variant_pat(
    variant_pat: &ast::VariantPat,
    errors: &mut Vec<SyntaxError>,
) {
    if variant_pat.colon2_token().is_some() {
        return;
    }
    let Some(name_ref) = variant_pat.variant_name_ref() else {
        return;
    };
    let name = name_ref.text();
    errors.push(SyntaxError {
        message: format!("write `::{name}(...)` to match a variant, or remove `(...)` to bind"),
        range: variant_pat.syntax().text_range(),
        fix: Some(Fix {
            label: "Insert `::`".to_owned(),
            edits: vec![TextEdit {
                range: TextRange::empty(name_ref.syntax().text_range().start()),
                insert: "::".to_owned(),
            }],
        }),
    });
}

/// The grammar parses any expression where the language requires a block;
/// reject the superset with a wrap-in-braces fix.
fn require_block(expr: &ast::Expr, rule: &str, errors: &mut Vec<SyntaxError>) {
    if matches!(expr, ast::Expr::BlockExpr(_)) {
        return;
    }
    let range = expr.syntax().text_range();
    errors.push(SyntaxError {
        message: format!("{rule}; wrap this expression in `{{ }}`"),
        range,
        fix: Some(Fix {
            label: "Wrap in `{ }`".to_owned(),
            edits: vec![
                TextEdit {
                    range: TextRange::empty(range.start()),
                    insert: "{ ".to_owned(),
                },
                TextEdit {
                    range: TextRange::empty(range.end()),
                    insert: " }".to_owned(),
                },
            ],
        }),
    });
}
