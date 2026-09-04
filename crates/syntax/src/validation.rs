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
            reject_nested_generic_binder(&fn_literal, &mut errors);
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
            reject_stray_type_binder(
                record_expr.generic_param_list(),
                record_expr.syntax(),
                &mut errors,
            );
            // A CONSTRUCTION literal (any record expr that is not a `type`
            // declaration's RHS) must define every field: an annotation
            // with no value has nothing to construct from. Type-decl RHS
            // fields are the opposite (annotation-only) — hir judges those.
            let is_type_decl_rhs = record_expr
                .syntax()
                .parent()
                .is_some_and(|p| ast::TypeItem::can_cast(p.kind()));
            for field in record_expr.fields() {
                reject_pub_field(field.pub_token(), &mut errors);
                if !is_type_decl_rhs
                    && field.colon_token().is_some()
                    && field.eq_token().is_none()
                    && field.expr().is_none()
                {
                    errors.push(SyntaxError {
                        message: "this field has a type but no value; \
                                  write `name: Type = value` (or `name = value`)"
                            .to_owned(),
                        range: field.syntax().text_range(),
                        fix: None,
                    });
                }
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
            reject_stray_type_binder(
                enum_expr.generic_param_list(),
                enum_expr.syntax(),
                &mut errors,
            );
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
        } else if let Some(param) = ast::Param::cast(node.clone()) {
            require_mut_names_a_binding(param.mut_token(), param.pat(), &mut errors);
        } else if let Some(unsafe_block) = ast::UnsafeBlockExpr::cast(node.clone()) {
            // Reserved: `unsafe fn` parses whole (its real payoff is
            // API-contract signalling, which wants doc conventions before
            // mechanism); any other non-block body gets the ordinary
            // wrap-in-braces treatment. The parser reports the
            // missing-body case itself.
            match unsafe_block.expr() {
                Some(ast::Expr::FnLiteral(fn_lit)) => errors.push(SyntaxError {
                    message: "`unsafe fn` is not supported yet; use `unsafe { ... }` blocks \
                              inside a plain `fn`"
                        .to_owned(),
                    range: fn_lit.syntax().text_range(),
                    fix: None,
                }),
                Some(body) => require_block(&body, "`unsafe` blocks", &mut errors),
                None => {}
            }
        } else if let Some(group) = ast::WithGroup::cast(node.clone()) {
            validate_with_group(&group, &mut errors);
        } else if let Some(unsafe_element) = ast::UnsafeElement::cast(node.clone()) {
            // Reserved (TR01): the `unsafe` modifier head — `unsafe impl
            // send;` markers and `unsafe { ... }` element groups.
            if let Some(token) = unsafe_element.unsafe_token() {
                errors.push(SyntaxError {
                    message: "`unsafe` impl elements are not supported yet".to_owned(),
                    range: token.text_range(),
                    fix: None,
                });
            }
        } else if let Some(for_element) = ast::ForElement::cast(node.clone()) {
            // Reserved (TR01): covered impls (`for Box::<Self> impl ...`),
            // the anchor home.
            if let Some(token) = for_element.for_token() {
                errors.push(SyntaxError {
                    message: "`for` (covered) impl elements are not supported yet".to_owned(),
                    range: token.text_range(),
                    fix: None,
                });
            }
        } else if let Some(impl_element) = ast::ImplElement::cast(node.clone()) {
            validate_impl_element(&impl_element, &mut errors);
        } else if let Some(member) = ast::Member::cast(node.clone()) {
            validate_member(&member, &mut errors);
        } else if let Some(ref_type) = ast::RefType::cast(node.clone()) {
            // `&T`/`&mut T` stay unclaimed for real references — parse-and-
            // reserve, the same pattern as `pub` fields. `&raw T` is a
            // distinct node (`RawPtrType`) and never lands here.
            errors.push(SyntaxError {
                message: "references are not supported yet".to_owned(),
                range: ref_type.syntax().text_range(),
                fix: None,
            });
        }
    }
    errors
}

/// Whether `node` sits inside the one semantically supported element
/// context: a plain `with { ... }` group directly on a `type` declaration,
/// inside an `impl Self { ... }` element that is a DIRECT child of the
/// group (not under a reserved `unsafe`/`for` head — those wrap their
/// payload in their own node, so the parent-cast below already excludes
/// them). Everything outside this context is covered by its own single
/// "not supported yet" reservation, so member-level checks stay quiet
/// there.
pub fn in_inherent_member_context(node: &SyntaxNode) -> bool {
    let Some(impl_element) = node
        .ancestors()
        .find_map(ast::ImplElement::cast)
        .filter(ast::ImplElement::is_self_head)
    else {
        return false;
    };
    let Some(group) = impl_element
        .syntax()
        .parent()
        .and_then(ast::WithGroup::cast)
        .filter(ast::WithGroup::is_plain)
    else {
        return false;
    };
    group
        .syntax()
        .parent()
        .is_some_and(|p| ast::TypeItem::can_cast(p.kind()))
}

/// The reservation checks for one attachment group: `with`-chains attach
/// to `type` declarations only, and only the plain form is supported
/// (no binders, no clauses).
fn validate_with_group(group: &ast::WithGroup, errors: &mut Vec<SyntaxError>) {
    let anchor = group
        .with_token()
        .map(|t| t.text_range())
        .unwrap_or_else(|| group.syntax().text_range());
    if group
        .syntax()
        .parent()
        .is_some_and(|p| ast::StaticItem::can_cast(p.kind()))
    {
        errors.push(SyntaxError {
            message: "`with` attachment groups belong on `type` declarations only".to_owned(),
            range: anchor,
            fix: None,
        });
        return;
    }
    if let Some(list) = group.generic_param_list() {
        errors.push(SyntaxError {
            message: "`with::<...>` binder groups are not supported yet".to_owned(),
            range: list.syntax().text_range(),
            fix: None,
        });
    }
    for clause in group.clauses() {
        let message = if clause.eq_token().is_some() {
            "`with T = ...` pin groups are not supported yet"
        } else {
            "`with T: ...` constrained groups are not supported yet"
        };
        errors.push(SyntaxError {
            message: message.to_owned(),
            range: clause.syntax().text_range(),
            fix: None,
        });
    }
}

/// The reservation checks for one `impl` element: only `impl Self { members }`
/// is supported — anything with a non-`Self` head (a trait impl, a marker)
/// is reserved, and `impl Self` requires a member body.
fn validate_impl_element(impl_element: &ast::ImplElement, errors: &mut Vec<SyntaxError>) {
    let anchor = impl_element
        .head()
        .map(|head| head.syntax().text_range())
        .or_else(|| impl_element.impl_token().map(|t| t.text_range()))
        .unwrap_or_else(|| impl_element.syntax().text_range());
    if !impl_element.is_self_head() {
        // One honest reservation per element; its members stay unjudged.
        errors.push(SyntaxError {
            message: "trait impls are not supported yet; \
                      only `impl Self { ... }` (inherent members) is"
                .to_owned(),
            range: anchor,
            fix: None,
        });
        return;
    }
    if impl_element.l_brace_token().is_none() {
        // `impl Self;` — a body-elided inherent impl declares nothing.
        errors.push(SyntaxError {
            message: "`impl Self` requires a member body (`{ ... }`)".to_owned(),
            range: anchor,
            fix: None,
        });
    }
}

/// The member rules, applied only in the supported context (see
/// [`in_inherent_member_context`]): members are `=`-defined `fn` literals;
/// colon-declared members, type ascriptions, associated types/consts and
/// non-`fn` values are rejected (declare-only inherent members are
/// unimplementable promises; the rest is reserved).
fn validate_member(member: &ast::Member, errors: &mut Vec<SyntaxError>) {
    if !in_inherent_member_context(member.syntax()) {
        return;
    }
    let range = member.syntax().text_range();
    if let Some(token) = member.type_token() {
        errors.push(SyntaxError {
            message: "associated types are not supported yet".to_owned(),
            range: token.text_range(),
            fix: None,
        });
        return;
    }
    if let Some(token) = member.const_token() {
        errors.push(SyntaxError {
            message: "associated consts are not supported yet".to_owned(),
            range: token.text_range(),
            fix: None,
        });
        return;
    }
    match (member.colon_token(), member.eq_token()) {
        (Some(_), None) => {
            errors.push(SyntaxError {
                message: "a declare-only inherent member is an unimplementable promise; \
                          define it: `name = fn(...) -> ... { ... };`"
                    .to_owned(),
                range,
                fix: None,
            });
            return;
        }
        (Some(colon), Some(_)) => {
            let end = member
                .ty()
                .map(|ty| ty.syntax().text_range().end())
                .unwrap_or_else(|| colon.text_range().end());
            errors.push(SyntaxError {
                message: "member type ascriptions are not supported yet; \
                          the `fn` literal's own annotations are the signature"
                    .to_owned(),
                range: TextRange::new(colon.text_range().start(), end),
                fix: None,
            });
            // The `=`-defined value is still judged below.
        }
        (None, Some(_)) | (None, None) => {}
    }
    match member.value() {
        Some(ast::Expr::FnLiteral(_)) => {}
        // `name = unsafe fn ...` — the existing `unsafe fn` reservation
        // already fires on the wrapped literal; adding a second error here
        // would be noise.
        Some(ast::Expr::UnsafeBlockExpr(inner))
            if matches!(inner.expr(), Some(ast::Expr::FnLiteral(_))) => {}
        Some(value) => errors.push(SyntaxError {
            message: "a member must be defined as an `fn` literal".to_owned(),
            range: value.syntax().text_range(),
            fix: None,
        }),
        // No `=` at all: the declare-only error above (or a parse error)
        // covers it.
        None => {}
    }
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

/// The message for a non-place assignment target. A `pub const` because
/// `mir` traps such targets with exactly the squiggle's text (the
/// single-render rule of `hir::diag`, except this message originates here
/// in validation rather than in a semantic analysis).
pub const CAN_ONLY_ASSIGN_TO_A_VARIABLE: &str = "can only assign to a variable or its fields";

/// The grammar superset-parses any expression as an assignment's LHS;
/// accept a place — a plain variable, or a chain of field accesses rooted
/// at one — and reject everything else. Only the *structure* is judged
/// here; whether the root is `mut` (mutability is transitive from the
/// binding to every field, with no per-field `mut`) is inference's call.
fn require_variable_target(expr: &ast::Expr, errors: &mut Vec<SyntaxError>) {
    let mut place = expr.clone();
    loop {
        match place {
            ast::Expr::PathExpr(_) => return,
            ast::Expr::FieldExpr(field) => match field.receiver() {
                Some(receiver) => place = receiver,
                // `.x = 1` with no receiver at all: broken source, the
                // parse error covers it.
                None => return,
            },
            // A deref is a place segment too (`p.* = v;`, and the chain
            // `p.*.x = v;` — how much of it the language accepts is
            // inference's call, like field-root mutability).
            ast::Expr::DerefExpr(deref) => match deref.receiver() {
                Some(receiver) => place = receiver,
                None => return,
            },
            // An index is a place segment (`a[i] = v;`, `m[0][1] = v;`,
            // `p.buf[i].x = v;`) — root mutability is inference's call.
            ast::Expr::IndexExpr(index) => match index.base() {
                Some(base) => place = base,
                None => return,
            },
            _ => break,
        }
    }
    errors.push(SyntaxError {
        message: CAN_ONLY_ASSIGN_TO_A_VARIABLE.to_owned(),
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

/// A generic binder is only supported where the item tree can read it: on a
/// fn literal that *is* a `static`/`const` item's initializer (TR06: generics
/// are item-level in v1 — generic closures need scheme-typed bindings and
/// wait for the capture story). Anywhere else — a nested literal, a
/// parenthesized initializer — the binder parses but is rejected honestly.
fn reject_nested_generic_binder(fn_literal: &ast::FnLiteral, errors: &mut Vec<SyntaxError>) {
    let Some(generic_param_list) = fn_literal.generic_param_list() else {
        return;
    };
    if fn_literal
        .syntax()
        .parent()
        .is_some_and(|p| ast::StaticItem::can_cast(p.kind()))
    {
        return;
    }
    // A member's defining fn literal: the type's own binders already flow
    // into the member, and member-own binders are reserved — a more
    // precise message than the generic one below.
    if fn_literal
        .syntax()
        .parent()
        .is_some_and(|p| ast::Member::can_cast(p.kind()))
    {
        errors.push(SyntaxError {
            message: "generic members are not supported yet \
                      (the type's own binders are already in scope)"
                .to_owned(),
            range: generic_param_list.syntax().text_range(),
            fix: None,
        });
        return;
    }
    errors.push(SyntaxError {
        message: "generic function literals are only supported as item initializers".to_owned(),
        range: generic_param_list.syntax().text_range(),
        fix: None,
    });
}

/// A generic binder on a `struct`/`enum` literal is only meaningful where
/// the literal declares a type — directly as a `type` item's value
/// (`type Pair = struct::<T> { ... }`). A record *literal* in value
/// position never takes one; the binder parses (superset) and is rejected
/// here. (A misplaced `enum` literal is additionally rejected as a whole by
/// [`require_enum_declares_a_type`].)
fn reject_stray_type_binder(
    list: Option<ast::GenericParamList>,
    literal: &SyntaxNode,
    errors: &mut Vec<SyntaxError>,
) {
    let Some(list) = list else {
        return;
    };
    if literal
        .parent()
        .is_some_and(|p| ast::TypeItem::can_cast(p.kind()))
    {
        return;
    }
    errors.push(SyntaxError {
        message: "a generic binder is only supported on a `type` declaration's `struct`/`enum`"
            .to_owned(),
        range: list.syntax().text_range(),
        fix: None,
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
