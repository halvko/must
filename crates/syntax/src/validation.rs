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
        } else if let Some(trait_item) = ast::TraitItem::cast(node.clone()) {
            validate_trait_item(&trait_item, &mut errors);
        } else if let Some(type_param) = ast::TypeParam::cast(node.clone()) {
            validate_type_param_bounds(&type_param, &mut errors);
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
            // reserve, the same pattern as `pub` fields. `T.&raw` is a
            // distinct node (`RawPtrType`) and never lands here.
            errors.push(SyntaxError {
                message: "references are not supported yet".to_owned(),
                range: ref_type.syntax().text_range(),
                fix: None,
            });
        }
        // `x.&` / `x.&mut` and `T.&::<@a>` / `T.&mut::<@a>` — the postfix
        // SAFE borrows, duals of `.*`. Un-reserved; hir owns them from here
        // (region kinds, reborrow, exclusivity), and `x.&raw` / `T.&raw`
        // stay distinct nodes that never land here.
    }
    errors
}

/// The semantically-supported member contexts: inherent members
/// (`impl Self { ... }` in a type's plain group) and trait-impl
/// members — a bare-name-headed impl element in a plain group of a
/// NON-generic `type` declaration (type-side home) or of a `trait`
/// declaration (trait-side home). Everything outside these contexts is
/// covered by its own single "not supported yet" reservation, so
/// member-level checks stay quiet there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberContext {
    /// `impl Self { ... }` on a `type` declaration.
    Inherent,
    /// `impl Trait { ... }` on a type / `impl Type { ... }` on a trait.
    TraitImpl,
}

/// The semantic member context enclosing `node`, or `None` in reserved
/// territory (modifier heads, non-plain groups, generic-owner trait
/// impls, requirement bodies — a requirement member is not an impl
/// member).
pub fn semantic_member_context(node: &SyntaxNode) -> Option<MemberContext> {
    let impl_element = node.ancestors().find_map(ast::ImplElement::cast)?;
    let group = impl_element
        .syntax()
        .parent()
        .and_then(ast::WithGroup::cast)
        .filter(ast::WithGroup::is_plain)?;
    let owner = group.syntax().parent()?;
    if ast::TypeItem::can_cast(owner.kind()) {
        if impl_element.is_self_head() {
            return Some(MemberContext::Inherent);
        }
        if impl_element_bare_head(&impl_element).is_some() {
            // Trait impls on a GENERIC type are reserved (generic-type
            // impls); the element carries the reservation.
            let owner_generic = ast::TypeItem::cast(owner)
                .and_then(|it| it.body())
                .is_some_and(|body| match body {
                    ast::Expr::RecordExpr(record) => record.generic_param_list().is_some(),
                    ast::Expr::EnumExpr(en) => en.generic_param_list().is_some(),
                    _ => false,
                });
            return (!owner_generic).then_some(MemberContext::TraitImpl);
        }
        return None;
    }
    if ast::TraitItem::can_cast(owner.kind()) && impl_element_bare_head(&impl_element).is_some() {
        // A RESERVED generic trait (`requires::<...>`) must not go
        // semantically live through its chain — mirror of the
        // generic-owner exclusion above (reserved for generic traits;
        // nothing may silently depend on it being ignored).
        let owner_generic = ast::TraitItem::cast(owner)
            .and_then(|it| it.requires_def())
            .is_some_and(|def| def.generic_param_list().is_some());
        return (!owner_generic).then_some(MemberContext::TraitImpl);
    }
    None
}

/// The impl head when it is a bare name other than `Self` (`impl Display`,
/// `impl usize`) — the trait-naming / implementer-naming element form.
pub fn impl_element_bare_head(impl_element: &ast::ImplElement) -> Option<ast::NameRef> {
    match impl_element.head()? {
        ast::Type::PathType(path)
            if path.generic_arg_list().is_none() && path.variant_name_ref().is_none() =>
        {
            let name = path.name_ref()?;
            (name.text() != "Self").then_some(name)
        }
        _ => None,
    }
}

/// The reservation checks for one attachment group: `with`-chains attach
/// to `type` and `trait` declarations, not to `static` items, and only the
/// plain form is supported (no binders, no clauses).
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
            message: "`with` attachment groups do not belong on a `static` item".to_owned(),
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
        } else if clause.region_ident_token().is_some() {
            "`with @a: ...` outlives groups are not supported yet"
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

/// The liveness/reservation checks for one `impl` element. Live forms:
/// `impl Self { members }` on a type, `impl Trait { members }`
/// on a non-generic type and `impl Type { members }` on a trait
/// (the two homes of TR01). Reserved: marker (body-elided)
/// impls, trait impls on generic types, non-bare heads, and everything
/// under a modifier head or non-plain group (those carry their own
/// reservations).
fn validate_impl_element(impl_element: &ast::ImplElement, errors: &mut Vec<SyntaxError>) {
    let anchor = impl_element
        .head()
        .map(|head| head.syntax().text_range())
        .or_else(|| impl_element.impl_token().map(|t| t.text_range()))
        .unwrap_or_else(|| impl_element.syntax().text_range());
    let error = |errors: &mut Vec<SyntaxError>, message: &str| {
        errors.push(SyntaxError {
            message: message.to_owned(),
            range: anchor,
            fix: None,
        });
    };
    let Some(group) = impl_element
        .syntax()
        .parent()
        .and_then(ast::WithGroup::cast)
        .filter(ast::WithGroup::is_plain)
    else {
        // Under a modifier head or in a binder/clause group: the head's
        // (or group's) own reservation covers the element.
        return;
    };
    let Some(owner) = group.syntax().parent() else {
        return;
    };
    let body_elided = impl_element.l_brace_token().is_none();
    if ast::TraitItem::can_cast(owner.kind()) {
        if impl_element.is_self_head() {
            error(
                errors,
                "an impl in a trait's `with`-chain names the IMPLEMENTING type, not `Self`",
            );
            return;
        }
        if impl_element_bare_head(impl_element).is_none() {
            error(
                errors,
                "an impl in a trait's `with`-chain must name its implementer with a bare \
                 type name",
            );
            return;
        }
        if body_elided {
            error(errors, "marker impls (`impl name;`) are not supported yet");
            return;
        }
        if semantic_member_context(impl_element.syntax()) != Some(MemberContext::TraitImpl) {
            // The owner is a RESERVED generic trait: nothing in its chain
            // may go live (reserved for generic traits).
            error(
                errors,
                "impls in a generic trait's `with`-chain are not supported yet \
                 (generic traits are reserved)",
            );
        }
        return;
    }
    if !ast::TypeItem::can_cast(owner.kind()) {
        // `with` on a `static` item: the group's own rejection covers it.
        return;
    }
    if impl_element.is_self_head() {
        if body_elided {
            // `impl Self;` — a body-elided inherent impl declares nothing.
            error(errors, "`impl Self` requires a member body (`{ ... }`)");
        }
        return;
    }
    if impl_element_bare_head(impl_element).is_none() {
        error(
            errors,
            "an impl head must be `Self` (inherent members) or a bare trait name",
        );
        return;
    }
    if body_elided {
        error(errors, "marker impls (`impl name;`) are not supported yet");
        return;
    }
    if semantic_member_context(impl_element.syntax()) != Some(MemberContext::TraitImpl) {
        // A bare-head impl on a GENERIC type: reserved (generic-type impls).
        error(errors, "trait impls on generic types are not supported yet");
    }
}

/// The member rules for the reserved leading-keyword spellings, shared by
/// every member context: `type Item ...;` and `const N: ...;` stay
/// reserved. Returns `true` when one fired (the member is judged).
fn reject_reserved_member_keywords(member: &ast::Member, errors: &mut Vec<SyntaxError>) -> bool {
    if let Some(token) = member.type_token() {
        errors.push(SyntaxError {
            message: "associated types are not supported yet".to_owned(),
            range: token.text_range(),
            fix: None,
        });
        return true;
    }
    if let Some(token) = member.const_token() {
        errors.push(SyntaxError {
            message: "associated consts are not supported yet".to_owned(),
            range: token.text_range(),
            fix: None,
        });
        return true;
    }
    false
}

/// The rules for one REQUIREMENT member (a member of a trait's
/// `requires { ... }` body): the live form is a colon-declared fn
/// signature (`name: fn(...) -> R;`); equals-defined members (defaults),
/// associated types/consts and `unsafe fn` signatures parse and are
/// reserved.
fn validate_requirement_member(member: &ast::Member, errors: &mut Vec<SyntaxError>) {
    let range = member.syntax().text_range();
    if reject_reserved_member_keywords(member, errors) {
        return;
    }
    if let Some(eq) = member.eq_token() {
        let end = member
            .value()
            .map(|value| value.syntax().text_range().end())
            .unwrap_or_else(|| eq.text_range().end());
        errors.push(SyntaxError {
            message: "default members are not supported yet; a trait declares requirements \
                      (`name: fn(...) -> ...;`)"
                .to_owned(),
            range: TextRange::new(eq.text_range().start(), end),
            fix: None,
        });
        return;
    }
    if member.colon_token().is_none() {
        errors.push(SyntaxError {
            message: "a requirement declares its signature: `name: fn(...) -> ...;`".to_owned(),
            range,
            fix: None,
        });
        return;
    }
    match member.ty() {
        Some(ast::Type::FnType(fn_type)) => {
            if let Some(token) = fn_type.unsafe_token() {
                errors.push(SyntaxError {
                    message: "`unsafe` trait members are not supported yet".to_owned(),
                    range: token.text_range(),
                    fix: None,
                });
            }
            reject_requirement_param_patterns(&fn_type, errors);
        }
        Some(other) => errors.push(SyntaxError {
            message: "a requirement's signature must be an `fn` signature".to_owned(),
            range: other.syntax().text_range(),
            fix: None,
        }),
        // `name: ;` — the parse error covers it.
        None => {}
    }
}

/// A requirement DECLARES a signature; it has no body to bind anything in.
/// The parameter grammar it shares with a fn literal ([`param_list`]) can
/// still spell `mut` and a destructuring pattern, and both would be
/// promises about an implementation the declaration does not contain: the
/// binding mode and the shape a body picks apart are the implementer's
/// business, one per impl. So a requirement's parameter is a plain
/// `name: Type`, and everything else is refused here rather than silently
/// ignored by the signature reader.
fn reject_requirement_param_patterns(fn_type: &ast::FnType, errors: &mut Vec<SyntaxError>) {
    let Some(list) = fn_type.param_list() else {
        return;
    };
    for param in list.params() {
        let Some(pat) = param.pat() else {
            continue;
        };
        let plain = matches!(pat, ast::Pat::BindPat(_)) && param.mut_token().is_none();
        if plain {
            continue;
        }
        let start = param
            .mut_token()
            .map(|token| token.text_range().start())
            .unwrap_or_else(|| pat.syntax().text_range().start());
        errors.push(SyntaxError {
            message: "a requirement's parameter is a plain `name: Type`".to_owned(),
            range: TextRange::new(start, pat.syntax().text_range().end()),
            fix: None,
        });
    }
}

/// The member rules, applied only in the supported contexts (see
/// [`semantic_member_context`]): members are `=`-defined `fn` literals;
/// colon-declared members, type ascriptions, associated types/consts and
/// non-`fn` values are rejected (declare-only impl members are
/// unimplementable promises; the rest is reserved).
fn validate_member(member: &ast::Member, errors: &mut Vec<SyntaxError>) {
    if member
        .syntax()
        .parent()
        .is_some_and(|p| ast::RequiresDef::can_cast(p.kind()))
    {
        validate_requirement_member(member, errors);
        return;
    }
    let Some(context) = semantic_member_context(member.syntax()) else {
        return;
    };
    let range = member.syntax().text_range();
    if reject_reserved_member_keywords(member, errors) {
        return;
    }
    match (member.colon_token(), member.eq_token()) {
        (Some(_), None) => {
            let message = match context {
                MemberContext::Inherent => {
                    "a declare-only inherent member is an unimplementable promise; \
                     define it: `name = fn(...) -> ... { ... };`"
                }
                MemberContext::TraitImpl => {
                    "an impl member is defined with `=`; the colon-declared requirement \
                     form belongs in the trait declaration"
                }
            };
            errors.push(SyntaxError {
                message: message.to_owned(),
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

/// The reservation checks for one `trait` declaration: the live form
/// is the plain non-generic `requires { ... }` constructor. Aliases,
/// `unsafe requires`, `requires::<...>` binders and supertrait clauses
/// parse cleanly and are reserved; a superset-parsed `: Type` annotation
/// is rejected like a `type` item's.
fn validate_trait_item(trait_item: &ast::TraitItem, errors: &mut Vec<SyntaxError>) {
    if let Some(colon) = trait_item.colon_token() {
        let end = trait_item
            .ty()
            .map(|ty| ty.syntax().text_range().end())
            .unwrap_or_else(|| colon.text_range().end());
        let range = TextRange::new(colon.text_range().start(), end);
        errors.push(SyntaxError {
            message: "a `trait` declaration takes no type annotation".to_owned(),
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
    if let Some(alias) = trait_item.trait_alias() {
        errors.push(SyntaxError {
            message: "trait aliases are not supported yet; \
                      declare the trait with `requires { ... }`"
                .to_owned(),
            range: alias.syntax().text_range(),
            fix: None,
        });
    }
    let Some(requires) = trait_item.requires_def() else {
        return;
    };
    if let Some(token) = requires.unsafe_token() {
        errors.push(SyntaxError {
            message: "`unsafe` traits are not supported yet".to_owned(),
            range: token.text_range(),
            fix: None,
        });
    }
    if let Some(list) = requires.generic_param_list() {
        errors.push(SyntaxError {
            message: "generic traits are not supported yet".to_owned(),
            range: list.syntax().text_range(),
            fix: None,
        });
    }
    for clause in requires.clauses() {
        errors.push(SyntaxError {
            message: "supertrait clauses are not supported yet".to_owned(),
            range: clause.syntax().text_range(),
            fix: None,
        });
    }
}

/// Bound-position rules on one generic type parameter (`T: Display`).
/// Bounds are LIVE on fn binders (item fns, impl members, requirement
/// signatures); a bound on a `type` declaration's `struct`/`enum` binder
/// is reserved (bounded type declarations).
/// Shape rules apply everywhere live: a bound is a bare trait name —
/// turbofished bounds wait for generic traits, and nothing else can be a
/// bound at all. (Whether the name IS a trait is hir's judgement.)
fn validate_type_param_bounds(type_param: &ast::TypeParam, errors: &mut Vec<SyntaxError>) {
    if type_param.colon_token().is_none() {
        return;
    }
    let owner = type_param.syntax().parent().and_then(|list| list.parent());
    if let Some(owner) = &owner {
        // Reserved binder homes carry group-level reservations of their
        // own (`with::<...>`, `requires::<...>`); a bound inside one adds
        // no extra noise. A `type` declaration's binder is live grammar
        // with reserved bounds — say so precisely.
        if ast::WithGroup::can_cast(owner.kind()) || ast::RequiresDef::can_cast(owner.kind()) {
            return;
        }
        if ast::RecordExpr::can_cast(owner.kind()) || ast::EnumExpr::can_cast(owner.kind()) {
            errors.push(SyntaxError {
                message: "bounds on a `type` declaration's binder are not supported yet".to_owned(),
                range: type_param.syntax().text_range(),
                fix: None,
            });
            return;
        }
    }
    for bound in type_param.bounds() {
        match &bound {
            ast::Type::PathType(path)
                if path.variant_name_ref().is_none() && path.generic_arg_list().is_none() => {}
            ast::Type::PathType(path) if path.generic_arg_list().is_some() => {
                errors.push(SyntaxError {
                    message: "generic traits are not supported yet; \
                              a bound is a bare trait name"
                        .to_owned(),
                    range: bound.syntax().text_range(),
                    fix: None,
                });
            }
            _ => {
                errors.push(SyntaxError {
                    message: "only a trait name can be a bound".to_owned(),
                    range: bound.syntax().text_range(),
                    fix: None,
                });
            }
        }
    }
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
    // A member's defining fn literal: a TRAIT-IMPL member carries its own
    // binder (a requirement may be a generic fn — Display's `fmt` over
    // `W: Write`), so the whole binder is live there.
    //
    // An INHERENT member's binder is live for REGIONS and reserved for
    // everything else. The split is not arbitrary: the type's own type and
    // const params already flow into every member, so a member-own one is
    // redundant sugar. A region has no such source — regions on type
    // declarations are themselves reserved — so a member that takes a
    // borrow of `Self` has nowhere else to bind the per-call region it
    // needs, and with no elision it may not decline to name one. Granting
    // the other two kinds deletes the two errors below and nothing else.
    if fn_literal
        .syntax()
        .parent()
        .is_some_and(|p| ast::Member::can_cast(p.kind()))
    {
        if semantic_member_context(fn_literal.syntax()) == Some(MemberContext::TraitImpl) {
            return;
        }
        for param in generic_param_list.params() {
            let message = match &param {
                ast::GenericParam::RegionParam(_) => continue,
                ast::GenericParam::TypeParam(_) => {
                    "a member's own type parameters are not supported yet \
                     (the type's own binders are already in scope)"
                }
                ast::GenericParam::ConstParam(_) => {
                    "a member's own const parameters are not supported yet \
                     (the type's own binders are already in scope)"
                }
            };
            errors.push(SyntaxError {
                message: message.to_owned(),
                range: param.syntax().text_range(),
                fix: None,
            });
        }
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
