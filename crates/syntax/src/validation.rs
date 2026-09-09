//! Checks for trees the grammar deliberately over-accepts.
//!
//! Parsing a superset keeps the user's intent in the tree (so inference,
//! hover and friends work on broken code) and gives errors found here
//! exactly the information a quick fix needs.

use crate::ast::{self, AstNode};
use crate::{BRACE_RULE, Fix, SyntaxError, SyntaxKind, SyntaxNode, SyntaxToken, TextEdit};
use text_size::TextRange;

pub(crate) fn validate(root: &SyntaxNode) -> Vec<SyntaxError> {
    let mut errors = Vec::new();
    for node in root.descendants() {
        if let Some(fn_literal) = ast::FnLiteral::cast(node.clone()) {
            if fn_literal.is_extern() {
                validate_extern_fn(&fn_literal, &mut errors);
            } else if let Some(body) = fn_literal.body() {
                // The parser reports "expected `{`" itself when the body is
                // absent entirely.
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
        } else if let Some(fn_type) = ast::FnType::cast(node.clone()) {
            validate_fn_type(&fn_type, &mut errors);
        } else if let Some(static_item) = ast::StaticItem::cast(node.clone()) {
            validate_extern_static(&static_item, &mut errors);
        } else if let Some(type_item) = ast::TypeItem::cast(node.clone()) {
            reject_extern_marker(&node, &mut errors);
            reject_type_item_annotation(&type_item, &mut errors);
        } else if let Some(trait_item) = ast::TraitItem::cast(node.clone()) {
            reject_extern_marker(&node, &mut errors);
            validate_trait_item(&trait_item, &mut errors);
        } else if let Some(type_param) = ast::TypeParam::cast(node.clone()) {
            validate_type_param_bounds(&type_param, &mut errors);
        } else if let Some(params) = ast::GenericParamList::cast(node.clone()) {
            require_regions_first(&params, &mut errors);
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
        } else if let Some(literal_pat) = ast::LiteralPat::cast(node.clone()) {
            reject_non_char_literal_pat(&literal_pat, &mut errors);
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
            // Reserved: an `unsafe fn` LITERAL parses whole so validation
            // can refuse it by name. The `unsafe fn(...)` TYPE is live; a
            // marker on the literal would declare nothing the value's type
            // does not already say, so the spelling stays undecided. Any
            // other non-block body gets the ordinary wrap-in-braces
            // treatment. The parser reports the missing-body case itself.
            match unsafe_block.expr() {
                Some(ast::Expr::FnLiteral(fn_lit)) => errors.push(SyntaxError {
                    message: "an `unsafe fn` literal is not supported yet; the \
                              `unsafe fn(...)` type is live, so annotate the value \
                              and write a plain `fn`"
                        .to_owned(),
                    range: fn_lit.syntax().text_range(),
                    fix: None,
                }),
                Some(body) => require_block(&body, "`unsafe` blocks", &mut errors),
                None => {}
            }
        } else if let Some(group) = ast::WithGroup::cast(node.clone()) {
            validate_with_group(&group, &mut errors);
        } else if let Some(without) = ast::WithoutClause::cast(node.clone()) {
            validate_without_clause(&without, &mut errors);
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
        } else if let Some(path_type) = ast::PathType::cast(node.clone()) {
            reject_bare_angle_generic_args(
                path_type.name_ref(),
                path_type.generic_arg_list(),
                &mut errors,
            );
        } else if let Some(path_expr) = ast::PathExpr::cast(node.clone()) {
            reject_bare_angle_generic_args(
                path_expr.name_ref(),
                path_expr.generic_arg_list(),
                &mut errors,
            );
        } else if let Some(borrow_expr) = ast::BorrowExpr::cast(node.clone()) {
            // `x.&` / `x.&mut` — the postfix SAFE borrow, dual of `.*`, which
            // hir owns from here (region kinds, reborrow, exclusivity) — and
            // its RETIRED prefix spelling `&x` / `&mut x`, superset-parsed
            // into this same node (see `grammar::prefix_borrow_expr`); told
            // apart by the missing `DOT`. `x.&raw` stays a distinct node
            // (`AddrOfExpr`) and never lands here.
            reject_prefix_borrow_expr(&borrow_expr, &mut errors);
            reject_region_arg_on_borrow_expr(&borrow_expr, &mut errors);
        } else if let Some(borrow_type) = ast::BorrowType::cast(node.clone()) {
            // `T.&::<@a>` / `T.&mut::<@a>` and the RETIRED prefix spellings
            // `&T` / `&mut T` — mirror of the expression-side split above
            // (see `grammar::type_core`'s AMP arm). `T.&raw` stays a distinct
            // node (`RawPtrType`).
            reject_prefix_borrow_type(&borrow_type, &mut errors);
        }
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

/// Where a capability opt-out may sit, and which capabilities it may name.
///
/// ONE home is live: a `type` declaration, which does not have the
/// capability it sheds. The superset-parsed homes (`static`/`const`/`trait`
/// items, which share the item shape) are rejected here, each in its own
/// words, because "why not" differs: a value item has no say over its
/// type's capabilities, and a trait classifies types rather than being one.
/// A generic PARAMETER carries no capability clause at all — what a body
/// needs of its parameter is written in the parameter's BOUNDS (T22), so
/// the grammar no longer parses one there and this function never sees it.
///
/// Which names may be written is read off [`CAPABILITIES`], the one list
/// this crate keeps: `forget` is the only capability that exists, and the
/// doctrine's reserved names are an error rather than a silent no-op — the
/// message says which it is, so the reservation reads as a reservation.
fn validate_without_clause(without: &ast::WithoutClause, errors: &mut Vec<SyntaxError>) {
    let anchor = without
        .without_token()
        .map(|t| t.text_range())
        .unwrap_or_else(|| without.syntax().text_range());
    let parent = without.syntax().parent().map(|p| p.kind());
    let misplaced = match parent {
        Some(SyntaxKind::TYPE_ITEM) => None,
        Some(SyntaxKind::STATIC_ITEM) => Some(
            "a capability opt-out belongs on a `type` declaration; \
             a `static` has whatever capabilities its type has",
        ),
        Some(SyntaxKind::TRAIT_ITEM) => {
            Some("a capability opt-out belongs on a `type` declaration, not on a `trait`")
        }
        _ => Some("a capability opt-out cannot go here"),
    };
    if let Some(message) = misplaced {
        errors.push(SyntaxError {
            message: message.to_owned(),
            range: anchor,
            fix: None,
        });
        return;
    }
    // Every capability this DECLARATION has already shed, in source order
    // across all its clauses — so `without forget without forget` and
    // `without forget + forget` are the same mistake and get the same
    // answer. Saying it twice is not saying it twice as hard; it means the
    // writer thought one of the two was doing something else.
    let mut seen: Vec<String> = Vec::new();
    let owner = without.syntax().parent();
    let mut reached_this_clause = false;
    for clause in owner
        .iter()
        .flat_map(|owner| owner.children())
        .filter_map(ast::WithoutClause::cast)
    {
        let is_this = clause.syntax() == without.syntax();
        for capability in clause.capabilities() {
            let name = capability.text();
            if is_this {
                if seen.contains(&name) {
                    errors.push(SyntaxError {
                        message: format!("`{name}` is already opted out of here"),
                        range: capability.syntax().text_range(),
                        fix: None,
                    });
                    continue;
                }
                let message = match capability_named(&name) {
                    Some(known) if known.clause => {
                        seen.push(name);
                        continue;
                    }
                    Some(_) => format!(
                        "the `{name}` capability does not exist yet; \
                         `forget` is the only one that can be opted out of"
                    ),
                    None => format!(
                        "unknown capability `{name}`; \
                         `forget` is the only one that can be opted out of"
                    ),
                };
                errors.push(SyntaxError {
                    message,
                    range: capability.syntax().text_range(),
                    fix: None,
                });
            }
            seen.push(name);
        }
        if is_this {
            reached_this_clause = true;
            break;
        }
    }
    debug_assert!(reached_this_clause, "the clause is a child of its parent");
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
        // `name = unsafe fn ...` — the `unsafe fn` LITERAL reservation
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

/// One capability: a thing you can DO with a value, and where its name may
/// be written. A table of FACTS — every layer that asks phrases its own
/// refusal, so no diagnostic text lives here for another crate to import.
pub struct Capability {
    pub name: &'static str,
    /// Whether a `type` declaration may name it in its capability clause.
    pub clause: bool,
    /// Whether a generic body may require it of a parameter (`T: forget`).
    pub bound: bool,
}

/// The capability vocabulary — the language's own words, live or reserved.
/// ONE list, asked rather than enumerated: validation reads it to decide
/// what a clause may shed and what a bound is asking for, and `hir` reads
/// it for the same two questions one layer down (P10's idiom, the way
/// `keywords!` is the one keyword table).
///
/// `forget` is the only capability that exists. The rest are named in the
/// capability doctrine and mean nothing yet, which is why they are here at
/// all: a reservation that is not written down reads as a typo.
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        name: "forget",
        clause: true,
        bound: true,
    },
    Capability {
        name: "send",
        clause: false,
        bound: false,
    },
    Capability {
        name: "sync",
        clause: false,
        bound: false,
    },
    Capability {
        name: "destruct",
        clause: false,
        bound: false,
    },
];

/// The capability `name` spells, if it spells one. Deliberately scope-free:
/// the vocabulary is the language's, so no file can declare its way into or
/// out of one.
pub fn capability_named(name: &str) -> Option<&'static Capability> {
    CAPABILITIES
        .iter()
        .find(|capability| capability.name == name)
}

/// Whether a bound names a capability rather than a trait. A BARE path
/// only: `forget::<usize>` is not this capability wearing arguments, it is
/// a shape the bound rules already refuse.
fn names_capability(bound: &ast::Type) -> bool {
    let ast::Type::PathType(path) = bound else {
        return false;
    };
    path.variant_name_ref().is_none()
        && path.generic_arg_list().is_none()
        && path
            .name_ref()
            .is_some_and(|name| capability_named(&name.text()).is_some())
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
            // A CAPABILITY bound on the data side is not "not yet": it is
            // the thing T22 deleted. A container's capabilities are read
            // off what it CONTAINS — `Option::<T>` is linear exactly when
            // the `T` it was given is — so there is nothing for a
            // data-side parameter to require, now or later.
            let message = if type_param.bounds().any(|bound| names_capability(&bound)) {
                "a `type` declaration's parameters carry no capability bounds: \
                 a container is linear when what it holds is"
            } else {
                "bounds on a `type` declaration's binder are not supported yet"
            };
            errors.push(SyntaxError {
                message: message.to_owned(),
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

/// Only CHARACTER literals are patterns so far. The grammar takes every
/// literal kind (see `grammar::match_pattern`) so this can say which one
/// was written and that it is a "not yet", rather than the parser handing
/// back a blank "expected a pattern" for `match n { 0 => ... }`.
///
/// The reason `char` goes first is not favouritism: an integer pattern
/// wants a range/exhaustiveness story (`0..=9`, and which widths a bare
/// `0` covers) that a scalar with no arithmetic simply doesn't need, and a
/// string pattern wants an equality-on-slices story. Both are pattern-
/// language work, not char work.
fn reject_non_char_literal_pat(literal_pat: &ast::LiteralPat, errors: &mut Vec<SyntaxError>) {
    let Some(kind) = literal_pat.literal().and_then(|lit| lit.kind()) else {
        return;
    };
    let what = match kind {
        ast::LiteralKind::Char(_) => return,
        ast::LiteralKind::Int(_) => "integer",
        ast::LiteralKind::Str(_) => "string",
        ast::LiteralKind::Bool(_) => "boolean",
    };
    errors.push(SyntaxError {
        message: format!(
            "{what} literal patterns are not supported yet; \
             only character literals (`'x'`) can be matched"
        ),
        range: literal_pat.syntax().text_range(),
        fix: None,
    });
}

/// `Boxed<usize>` / `f<usize>(3)` — the bare-angle typo for `::<...>`, on
/// either side of the grammar (`PathType`, `PathExpr`). Bare angles are
/// permanently illegal (G06: turbofish everywhere), so the grammar parses
/// the typo into the shape it plainly means — the same `GENERIC_ARG_LIST`
/// the turbofish spelling builds — and this pass names the missing `::`
/// (X03: a never-legal-but-recognizable form is corrected, not refused by
/// the parser). The arguments themselves are judged exactly as a
/// correctly-spelled mention's are; nothing about arity or kinds is said
/// here.
///
/// The tell is the token immediately before the list, never
/// `colon2_token()` — for `f<usize>::assoc` that would find the TRAILING
/// `::`. A second segment's own list hangs inside `MemberGenericArgs`, out
/// of reach of the direct-child `generic_arg_list()` accessors, so it can
/// never be mistaken for the owner's.
///
/// Only a PATH'S OWNER is corrected: the grammar takes the bare form on a
/// first segment, so `Pair::first<usize>` is still read as a comparison
/// and reported as one.
fn reject_bare_angle_generic_args(
    name_ref: Option<ast::NameRef>,
    arg_list: Option<ast::GenericArgList>,
    errors: &mut Vec<SyntaxError>,
) {
    let (Some(name_ref), Some(arg_list)) = (name_ref, arg_list) else {
        return;
    };
    let mut prev = arg_list.syntax().prev_sibling_or_token();
    while let Some(element) = prev {
        if element.kind().is_trivia() {
            prev = element.prev_sibling_or_token();
            continue;
        }
        if element.kind() == SyntaxKind::COLON2 {
            return;
        }
        break;
    }
    let name = name_ref.text();
    errors.push(SyntaxError {
        message: format!("generic arguments use the turbofish: write `{name}::<...>`"),
        // The name and its arguments, not the whole path node: a trailing
        // qualified segment (`f<usize>::assoc`) is not part of the mistake.
        range: name_ref
            .syntax()
            .text_range()
            .cover(arg_list.syntax().text_range()),
        fix: Some(Fix {
            label: "Insert `::`".to_owned(),
            edits: vec![TextEdit {
                range: TextRange::empty(name_ref.syntax().text_range().end()),
                insert: "::".to_owned(),
            }],
        }),
    });
}

/// Whether a `BorrowExpr`/`BorrowType`/`RawPtrType` node is the RETIRED
/// prefix spelling (`&x` / `&T` / `&raw T`, built by
/// `grammar::prefix_borrow_expr` / `type_core`'s AMP arms) rather than the
/// real postfix `.&` / `.&mut` / `.&raw`. Both shapes complete into the same
/// node kind; the postfix form always carries the operator's own `DOT` as a
/// direct child (`operand DOT AMP ...`), which the prefix form — `AMP`
/// first, no `DOT` anywhere — never produces. An unambiguous, purely
/// structural tell.
fn is_prefix_spelling(node: &SyntaxNode) -> bool {
    !node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .any(|t| t.kind() == SyntaxKind::DOT)
}

/// The rewrite a retired prefix borrow needs: delete everything before the
/// operand (the leading `&`/`&mut `) and append the postfix spelling after
/// it. Two edits, so it is a validation-built [`Fix`] rather than a
/// parser-level single insertion.
fn rewrite_to_postfix_fix(node: TextRange, operand: TextRange, suffix: &str) -> Fix {
    Fix {
        label: "Rewrite as postfix".to_owned(),
        edits: vec![
            TextEdit {
                range: TextRange::new(node.start(), operand.start()),
                insert: String::new(),
            },
            TextEdit {
                range: TextRange::empty(operand.end()),
                insert: suffix.to_owned(),
            },
        ],
    }
}

/// `&x` / `&mut x` — the retired prefix safe borrow, superset-parsed into
/// the node the postfix form produces (see the `BorrowExpr` arm above). A
/// corrective migration diagnostic naming the postfix spelling, with a quick
/// fix that rewrites it; phrased like its `&raw` sibling in
/// `grammar::addr_of_expr`. No rebinding guard is needed on this side (the
/// type side has [`postfix_binds_to_whole_type`]): the operand parses at the
/// postfix tier already, so the only expressions it can END in are the other
/// prefix forms — `&-x`, `&&raw x`, `&&x` — and every one of those is a
/// borrow of something that is not a place, rejected with the same error
/// before and after the rewrite. (The rewrite rebinds there exactly as it
/// would in type position, `&&mut y` becoming `&mut y.&`; nothing rides on
/// it, because neither spelling compiles.)
fn reject_prefix_borrow_expr(borrow: &ast::BorrowExpr, errors: &mut Vec<SyntaxError>) {
    if !is_prefix_spelling(borrow.syntax()) {
        return;
    }
    let range = borrow.syntax().text_range();
    let suffix = if borrow.is_mut() { ".&mut" } else { ".&" };
    let fix = borrow
        .receiver()
        .map(|receiver| rewrite_to_postfix_fix(range, receiver.syntax().text_range(), suffix));
    errors.push(SyntaxError {
        message: "borrows are spelled postfix: `x.&` / `x.&mut`".to_owned(),
        range,
        fix,
    });
}

/// Does a postfix `.&` appended to this type's text bind to the WHOLE type?
/// Only when the type does not already END in a nested type of its own,
/// which the operator would claim instead — and with no type grouping there
/// is no other spelling to fall back on (see G08's "pointer or borrow to a
/// fn type" in `docs/design/grammar-and-syntax.md`). Three type forms end
/// that way: a `fn(..) -> T` (`&fn() -> usize` is a borrow OF a fn,
/// `fn() -> usize.&` a fn RETURNING one), a retired prefix raw pointer
/// (`&raw T.&` is a pointer TO a borrow) and a retired prefix borrow
/// (`&mut T.&` is a MUT borrow of a shared one — appending to `&&mut T`
/// would swap the two borrows' mutability; `&&T` survives only because it
/// is symmetric). The inner spelling migrates first, and once it is postfix
/// the outer's fix is sound again and returns. The match is exhaustive on
/// purpose: a new type form has to be classified, not defaulted.
fn postfix_binds_to_whole_type(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::FnType(it) => it.ret_type().is_none(),
        ast::Type::RawPtrType(it) => !is_prefix_spelling(it.syntax()),
        ast::Type::BorrowType(it) => !is_prefix_spelling(it.syntax()),
        // Each ends in a token or a closing bracket of its own.
        ast::Type::UnitType(_)
        | ast::Type::NeverType(_)
        | ast::Type::PathType(_)
        | ast::Type::HoleType(_)
        | ast::Type::RecordType(_)
        | ast::Type::ArrayType(_) => true,
    }
}

/// `&T` / `&mut T` — the retired prefix safe borrow TYPE, mirror of
/// [`reject_prefix_borrow_expr`]. The rewrite leaves the region turbofish
/// off: `T.&` needs one everywhere (no elision exists for a hand-written
/// type), and hir's own "must name its region" diagnostic says so with the
/// spelling — a name invented here would be a guess. It is withheld
/// entirely where postfix cannot express the same type
/// ([`postfix_binds_to_whole_type`]); the migration itself still reports.
fn reject_prefix_borrow_type(borrow: &ast::BorrowType, errors: &mut Vec<SyntaxError>) {
    if !is_prefix_spelling(borrow.syntax()) {
        return;
    }
    let range = borrow.syntax().text_range();
    let suffix = if borrow.is_mut() { ".&mut" } else { ".&" };
    let fix = borrow
        .ty()
        .filter(postfix_binds_to_whole_type)
        .map(|ty| rewrite_to_postfix_fix(range, ty.syntax().text_range(), suffix));
    errors.push(SyntaxError {
        message: "borrow types are spelled postfix: `T.&` / `T.&mut`".to_owned(),
        range,
        fix,
    });
}

/// A region argument on an expression-position borrow — `x.&::<@a>`,
/// `p.*.&mut::<@_>`. Regions are written only in type positions, and a
/// borrow is an operation (G11), so the whole list is refused whatever it
/// holds: a borrow's turbofish can only ever have carried a region, so
/// there is no other argument to keep and no name to resolve first, and
/// `@_` says nothing an omitted turbofish does not. The wrong-kind case
/// (`x.&::<usize>`) folds in — a kind message presupposes a slot, and an
/// operation has none; the type side keeps its kind and arity messages
/// because there a slot exists. Twin of `hir::diag::REGION_ARG_AT_MENTION`
/// (the same rule at a call), phrased to read as one sentence with it. The
/// type-position `T.&::<@a>` is untouched. The example follows the
/// operator, so copying it keeps `.&mut` exclusive.
fn region_arg_at_borrow(mutable: bool) -> String {
    let op = if mutable { ".&mut" } else { ".&" };
    format!(
        "regions are inferred at a borrow, never written: drop this argument — \
         a region belongs in a type position, so assert it with an annotation \
         (`let r: _{op}::<@a> = x{op};`)"
    )
}

/// Reports [`region_arg_at_borrow`] with the one-token fix: delete the
/// turbofish. A turbofish is the `::` plus the list, and the `::` is bumped
/// into the borrow node ahead of the list (`grammar::borrow_op_generic_args`),
/// so the range is contiguous. The fix is not semantics-neutral — the
/// argument constrained, so deleting it drops an assertion rather than
/// relocating one; the message names the annotation that keeps it.
fn reject_region_arg_on_borrow_expr(borrow: &ast::BorrowExpr, errors: &mut Vec<SyntaxError>) {
    let (Some(colon2), Some(args)) = (borrow.colon2_token(), borrow.generic_arg_list()) else {
        return;
    };
    let range = TextRange::new(
        colon2.text_range().start(),
        args.syntax().text_range().end(),
    );
    errors.push(SyntaxError {
        message: region_arg_at_borrow(borrow.is_mut()),
        range,
        fix: Some(Fix {
            label: "Drop the region argument".to_owned(),
            edits: vec![TextEdit {
                range,
                insert: String::new(),
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
    // An INHERENT member's binder is live for REGIONS and TYPES, and
    // reserved for CONSTS. Types were the redundant-sugar case only for as
    // long as a member could not say anything the owner's binder could not
    // already say; `flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self)`
    // is the counter-example — `U` varies per CALL, and no binder on
    // `Option` can express that. Consts stay reserved on a different
    // ground entirely, and the message says so: a const argument is part
    // of an INSTANCE's identity, so it would have to reach
    // `Rvalue::Instantiate`'s argument list and the mangled symbol — and
    // the one place a member's arguments are carried reads them off the
    // receiver's own type, which cannot supply what the receiver does not
    // have. Granting it deletes the error below and nothing else here.
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
                ast::GenericParam::RegionParam(_) | ast::GenericParam::TypeParam(_) => continue,
                ast::GenericParam::ConstParam(_) => {
                    "a member's own const parameters are not supported yet \
                     (a const argument is part of an instance's identity, and a \
                     member's arguments are read off the receiver's own type)"
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

/// Region parameters come FIRST in every declared binder list
/// (`fn::<@a, @b, T, U>`), before every type and const parameter.
///
/// The rule exists because regions are ELIDED at every mention: the written
/// turbofish spells the binder's type and const parameters only. Eliding a
/// contiguous PREFIX leaves the written arguments looking exactly like the
/// parameters they spend — `fn::<@b, U>` called `f::<usize>` reads as one
/// argument for one parameter. Eliding an interior slot would not:
/// `fn::<T, @a, U>` called `f::<usize, bool>` would read as a list with a
/// hole silently collapsed out of the middle of it, and a reader would have
/// to know the binder to count. (Checking is *not* what needs the prefix:
/// `instantiate_mention` filters the region slots out under any order. The
/// reader is.)
///
/// Checked on the `GenericParamList` node, so it is one rule at every binder
/// position — fn literals, `struct`/`enum` declarations, trait requirements
/// — and a binder reads the same way wherever it is declared.
fn require_regions_first(list: &ast::GenericParamList, errors: &mut Vec<SyntaxError>) {
    let mut spellable: Option<String> = None;
    for param in list.params() {
        match &param {
            ast::GenericParam::RegionParam(region) => {
                let Some(earlier) = &spellable else {
                    continue;
                };
                let name = region.name().unwrap_or_else(|| "@".to_owned());
                errors.push(SyntaxError {
                    message: format!(
                        "region parameters come first in a binder; \
                         move `{name}` before `{earlier}`"
                    ),
                    range: param.syntax().text_range(),
                    fix: None,
                });
            }
            // A nameless (broken) parameter still counts as one: the
            // ordering is about the KINDS, and a placeholder keeps the
            // message readable rather than skipping the check.
            ast::GenericParam::TypeParam(type_param) => {
                spellable = spellable.or_else(|| {
                    Some(
                        type_param
                            .name()
                            .map(|n| n.text())
                            .unwrap_or_else(|| "T".to_owned()),
                    )
                });
            }
            ast::GenericParam::ConstParam(const_param) => {
                spellable = spellable.or_else(|| {
                    Some(
                        const_param
                            .name()
                            .map(|n| n.text())
                            .unwrap_or_else(|| "N".to_owned()),
                    )
                });
            }
        }
    }
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

/// The RETIRED initializer form of a host import,
/// `static name = extern fn(...) -> T;`. Superset-parsed into the same
/// `FN_LITERAL` it always produced (never a silent reinterpretation — the
/// postfix-borrow migration precedent), and refused here with the rewrite.
///
/// Why it is retired: an import **sets nothing to anything**. There is no
/// value on the right-hand side to write down — the declaration promises
/// that a name of this type exists and the host (or the linker, or the
/// environment) is what provides it. That is a DECLARATION, so it is spelled
/// like one: `extern static read: unsafe fn(...) -> T;`.
fn validate_extern_fn(fn_literal: &ast::FnLiteral, errors: &mut Vec<SyntaxError>) {
    let item = fn_literal.syntax().parent().and_then(ast::StaticItem::cast);
    let name = item
        .as_ref()
        .and_then(|it| it.name())
        .map(|n| n.text())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "name".to_owned());
    // The rewrite is offered only for the shape it can rewrite CORRECTLY:
    // a bodyless, non-`const`, non-generic literal initializing a plain
    // `static`. The shapes the old spelling rejected (a body, `const extern fn`, a
    // binder) keep the message alone — a fix that produced a second broken
    // item would be worse than none.
    let fix = item
        .as_ref()
        .filter(|item| !item.is_const() && item.extern_token().is_none() && item.ty().is_none())
        .filter(|_| {
            fn_literal.body().is_none()
                && fn_literal.const_token().is_none()
                && fn_literal.generic_param_list().is_none()
        })
        .and_then(|item| {
            Some((
                item,
                item.name()?.syntax().text_range().end(),
                fn_literal.extern_token()?,
            ))
        })
        .map(|(item, name_end, extern_token)| Fix {
            label: "Rewrite as an `extern static` declaration".to_owned(),
            edits: vec![
                TextEdit {
                    range: TextRange::empty(item.syntax().text_range().start()),
                    insert: "extern ".to_owned(),
                },
                // ` = extern` becomes `: unsafe`: the annotation slot takes
                // the signature the initializer used to hold, and the
                // import's price moves onto the TYPE where T19 put it.
                TextEdit {
                    range: TextRange::new(name_end, extern_token.text_range().end()),
                    insert: ": unsafe".to_owned(),
                },
            ],
        });
    // The retired spelling with an ANNOTATION writes the signature TWICE.
    // The annotation is the home hir keeps (the respelling has nowhere else
    // to put it), so the message says so — an initializer signature that
    // disagrees with it is dropped, and a migration never drops something
    // silently.
    let annotation = item
        .as_ref()
        .filter(|item| item.declares_host_import())
        .and_then(|item| item.ty());
    let twice = if annotation.is_some() {
        " — the annotation is the contract, and this signature is dropped"
    } else {
        ""
    };
    errors.push(SyntaxError {
        message: format!(
            "a host import is a DECLARATION, not an initializer: write \
             `extern static {name}: unsafe fn(...) -> T;`{twice}"
        ),
        range: fn_literal.syntax().text_range(),
        fix,
    });
    // ... and that annotation is an IMPORT's annotation, held to the same
    // rules the live spelling's is. Retiring a spelling must not relax what
    // the programs written in it are held to.
    if let Some(ty) = annotation {
        validate_import_annotation(&ty, errors);
    }
}

/// `extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;`
/// — a HOST IMPORT declaration (G22).
///
/// Everything refused here is refused because the DECLARATION IS THE WHOLE
/// CONTRACT: the item's name is the import's field name, the annotation is
/// the one machine signature the host must provide, and there is nothing
/// else — no value, no body, no second place for the truth to live.
fn validate_extern_static(item: &ast::StaticItem, errors: &mut Vec<SyntaxError>) {
    let Some(marker) = item.extern_token() else {
        return;
    };
    // An item the parser could not read at all (`extern fn g(...)`, the C
    // spelling: it recovers by taking the rest as one `ERROR`). Its message
    // names the one thing that is wrong; ours would be a consequence.
    if item
        .syntax()
        .children()
        .any(|child| child.kind() == SyntaxKind::ERROR)
    {
        return;
    }
    // `extern const x` — `const` is copied per mention and an import is one
    // identity. (Same message a stray `extern` on a `type`/`trait` item
    // gets: one marker, one home.)
    if item.is_const() {
        errors.push(SyntaxError {
            message: EXTERN_ONLY_ON_STATIC.to_owned(),
            range: marker.text_range(),
            fix: None,
        });
        return;
    }
    if let Some(eq) = item.eq_token() {
        let end = item
            .body()
            .map(|body| body.syntax().text_range().end())
            .unwrap_or_else(|| eq.text_range().end());
        let range = TextRange::new(eq.text_range().start(), end);
        errors.push(SyntaxError {
            message: "an `extern static` has no initializer: the declaration is the \
                      whole contract, and an import sets nothing to anything"
                .to_owned(),
            range,
            fix: Some(Fix {
                label: "Remove the initializer".to_owned(),
                edits: vec![TextEdit {
                    // From the END OF THE DECLARATION — its annotation when
                    // it has one, its name otherwise. Cutting from the name
                    // would take the type with it, and the type is the whole
                    // contract.
                    range: TextRange::new(
                        item.ty()
                            .map(|ty| ty.syntax().text_range().end())
                            .or_else(|| item.name().map(|n| n.syntax().text_range().end()))
                            .unwrap_or_else(|| range.start()),
                        end,
                    ),
                    insert: String::new(),
                }],
            }),
        });
    }
    // Everything below is about the CONTRACT, and only an item that
    // actually declares an import has one: `extern static x: T = v;` is an
    // ordinary item with a written value (the value wins, the marker is
    // dropped), so refusing its annotation for having a `_` the body fills
    // — or for not being a function type — would be a sentence that is not
    // true of the program it is printed on.
    if !item.declares_host_import() {
        return;
    }
    match item.ty() {
        Some(ty) => validate_import_annotation(&ty, errors),
        None => errors.push(SyntaxError {
            message: "an import must declare its type: \
                      `extern static name: unsafe fn(...) -> T;`"
                .to_owned(),
            range: item
                .name()
                .map(|n| n.syntax().text_range())
                .unwrap_or_else(|| marker.text_range()),
            fix: None,
        }),
    }
}

/// The rules an import's ANNOTATION answers to — one home for them, because
/// both spellings put the contract in the same place once the retired one
/// writes an annotation at all (`static r: unsafe fn(...) -> T = extern
/// fn(...);` is an import whose annotation is an import's annotation).
fn validate_import_annotation(ty: &ast::Type, errors: &mut Vec<SyntaxError>) {
    match ty {
        ast::Type::FnType(fn_type) => {
            // NOTHING in the contract may be left to inference. A `_` in an
            // ordinary annotation names a type the body determines; an
            // import has no body, and this type is what the host is judged
            // against and what the backend emits — so an unwritten piece of
            // it is unwritten for good.
            for hole in fn_type
                .syntax()
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::HOLE_TYPE)
            {
                errors.push(SyntaxError {
                    message: "an import's type must be written in full: the declaration \
                              is the whole contract, and there is no body for `_` to be \
                              inferred from"
                        .to_owned(),
                    range: hole.text_range(),
                    fix: None,
                });
            }
            // The written `unsafe` is REQUIRED (G22), and required
            // CONSERVATIVELY — not as a law about imports.
            //
            // The split: DECLARING a signature is itself a vouch
            // (a misdeclared import is undefined behavior before anything
            // calls it), while CALLING is priced per function by its TYPE —
            // `read` really is `unsafe fn`, but a correctly declared
            // `now: fn() -> i64` would be safe to call. The declaration-side
            // vouch has no spelling yet. Until it has one, accepting a
            // safe-call import would leave the vouch obligation with no home
            // at all: nothing written anywhere would say that someone
            // checked this signature against the host. So every import
            // carries the marker for now, and the message says "for now".
            if fn_type.unsafe_token().is_none() {
                let range = fn_type.syntax().text_range();
                errors.push(SyntaxError {
                    message: "an import must be declared `unsafe fn` for now: a \
                              safe-to-call import needs the declaration-side `unsafe` \
                              marker, and that marker does not exist yet"
                        .to_owned(),
                    range,
                    fix: Some(Fix {
                        label: "Write `unsafe fn`".to_owned(),
                        edits: vec![TextEdit {
                            range: TextRange::empty(range.start()),
                            insert: "unsafe ".to_owned(),
                        }],
                    }),
                });
            }
            // An import has exactly ONE machine signature, so there is
            // nothing for a binder to range over and nothing to
            // monomorphize it into. (The old spelling's refusal, restored
            // at the shape the respell moved it to.)
            if let Some(binder) = fn_type.generic_param_list() {
                errors.push(SyntaxError {
                    message: "an import cannot be generic: it has exactly one machine \
                              signature, and there is nothing to monomorphize it into"
                        .to_owned(),
                    range: binder.syntax().text_range(),
                    fix: None,
                });
            }
        }
        // A DATA import is newly expressible and is RESERVED (G22) —
        // parse-and-reserve, so granting it later deletes a diagnostic
        // instead of inventing a syntax.
        other => errors.push(SyntaxError {
            message: "data imports are not supported yet — an import must have \
                      a function type"
                .to_owned(),
            range: other.syntax().text_range(),
            fix: None,
        }),
    }
}

/// The one sentence every misplaced `extern` marker gets: the marker
/// declares a NAME the host provides a value for, and only a `static`
/// declares one of those.
const EXTERN_ONLY_ON_STATIC: &str =
    "only a `static` can be `extern`: an import declares one name with one type";

/// Rules that hold of EVERY fn type, wherever it is written — an import's
/// annotation is one fn type among many and gets no parameter grammar of
/// its own (its extra refusals are in [`validate_extern_static`]).
///
/// The colon-declared MEMBER signature is the one shape excused: it parses
/// the pattern-shaped [`grammar::param_list`] and its binder is live (a
/// trait requirement may be a generic fn), so its own rules apply.
fn validate_fn_type(fn_type: &ast::FnType, errors: &mut Vec<SyntaxError>) {
    let parent_kind = fn_type.syntax().parent().map(|p| p.kind());
    if parent_kind == Some(SyntaxKind::MEMBER) {
        return;
    }
    // A generic function is declared by an ITEM; a type mentions instances
    // of one. (An import's own version of this refusal says more and fires
    // instead — see `validate_extern_static`.)
    let is_import_annotation = fn_type
        .syntax()
        .parent()
        .and_then(ast::StaticItem::cast)
        .is_some_and(|item| item.is_extern());
    if let Some(binder) = fn_type.generic_param_list()
        && !is_import_annotation
    {
        errors.push(SyntaxError {
            message: "a function type has no generic binder: a generic function is \
                      declared by an item, and a type names one of its instances"
                .to_owned(),
            range: binder.syntax().text_range(),
            fix: None,
        });
    }
    // A parameter with a name and no type. The grammar admits `Type` and
    // `name: Type`; anything with a name and nothing after the colon is a
    // half-written signature, and a fn type has nothing to infer the rest
    // from.
    let Some(list) = fn_type.param_list() else {
        return;
    };
    for param in list.params().filter(|p| p.ty().is_none()) {
        errors.push(SyntaxError {
            message: "this parameter has no type: a function type's parameters are \
                      `name: Type` or `Type`"
                .to_owned(),
            range: param.syntax().text_range(),
            fix: None,
        });
    }
}

/// A stray `extern` marker on a `type`/`trait` item — superset-parsed by
/// `grammar::item` so the rest of the declaration still reads.
fn reject_extern_marker(node: &SyntaxNode, errors: &mut Vec<SyntaxError>) {
    let marker = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == SyntaxKind::EXTERN_KW);
    if let Some(marker) = marker {
        errors.push(SyntaxError {
            message: EXTERN_ONLY_ON_STATIC.to_owned(),
            range: marker.text_range(),
            fix: None,
        });
    }
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
