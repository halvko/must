//! Capabilities: what you can DO with a value of a type.
//!
//! One capability exists so far — `forget`, the ability to let a value go
//! out of scope with nothing done about it. Every type has it by default;
//! a declaration sheds it by saying so (`type S = struct { … } without
//! forget;`), and a container inherits the loss from its parts.
//!
//! A type WITHOUT `forget` is a linear type: a value of it must be consumed
//! exactly once on every path. That is the whole of the disposal story —
//! there is no destructor, no drop glue and no unwinding, so the checker
//! (see [`crate::linear_check`]) is the only thing that runs, and it runs at
//! check time only. Codegen never learns that a type is linear.
//!
//! Two rules and their reasons:
//!
//! - **Containment infects.** A record whose field lacks `forget` lacks it;
//!   an enum whose payload lacks it lacks it. It could not be otherwise: if
//!   the container could be dropped on the floor, so could the part, and the
//!   part is the thing with the obligation.
//! - **Indirection does not.** A borrow, a raw pointer, or a `fn` value
//!   mentioning a linear type has `forget`. A borrow is not the value; a raw
//!   pointer's story is `unsafe` and always was; a `fn` type is a signature,
//!   not an inhabitant. Losing any of them loses no obligation, because the
//!   obligation stayed with the owner.
//!
//! And the default BOUND, which is the same fact read at a binder: a generic
//! type parameter requires `forget` unless it says `without forget`. The
//! bound is the default because retrofitting one against an existing
//! ecosystem is the thing that cannot be done later — pre-ecosystem is the
//! cheap moment, and Must is pre-ecosystem.

use base_db::Db;

use crate::ItemLoc;
use crate::ty::{NamedTy, Ty, VariantTy};

/// Whether values of `ty` may be forgotten — let go with nothing done about
/// them. `false` makes the type LINEAR: [`crate::linear_check`] then demands
/// exactly one consumption on every path.
///
/// Total and silent on broken types ([`Ty::Error`], unresolved variables,
/// declarations that failed to lower): an unknown type answers `true`, so a
/// broken program never grows must-consume errors on top of the error it
/// already has.
pub fn has_forget(db: &dyn Db, ty: &Ty) -> bool {
    forget_of(db, ty, &mut Vec::new())
}

/// A one-line explanation of why `ty` lacks `forget`, for the diagnostic
/// that refuses it — `None` when it has the capability. Names what shed the
/// capability and, when the loss came through containment, the path that
/// carried it (`through field \`buf\``).
pub fn no_forget_reason(db: &dyn Db, ty: &Ty) -> Option<String> {
    let mut path = Vec::new();
    let root = blame(db, ty, &mut Vec::new(), &mut path)?;
    if path.is_empty() {
        return Some(root);
    }
    path.reverse();
    Some(format!("{root}, reached {}", path.join(", ")))
}

fn forget_of(db: &dyn Db, ty: &Ty, seen: &mut Vec<ItemLoc>) -> bool {
    match ty {
        // A declaration that shed the capability, or one that contains
        // something which did.
        Ty::Named(named) => {
            if decl_without_forget(db, &named.decl) {
                return false;
            }
            with_decl(seen, &named.decl, |seen| {
                named_components(db, named)
                    .iter()
                    .all(|component| forget_of(db, component, seen))
            })
        }
        // A variant-typed value is its payload tuple; the enum's own
        // opt-out covers it too, since widening one to the other must not
        // change what has to happen to it.
        Ty::Variant(variant) => {
            if decl_without_forget(db, &variant.decl) {
                return false;
            }
            with_decl(seen, &variant.decl, |seen| {
                crate::ty::variant_payloads_for(db, variant)
                    .unwrap_or_default()
                    .iter()
                    .all(|payload| forget_of(db, payload, seen))
            })
        }
        Ty::Record(record) => record
            .fields
            .iter()
            .all(|(_, field)| forget_of(db, field, seen)),
        Ty::Array { elem, .. } => forget_of(db, elem, seen),
        // A rigid parameter is whatever its binder promised. No opt-out
        // means the caller had to supply a forgettable type, so the body
        // may forget it; an opt-out means the body may not assume that,
        // and is checked as if the parameter were linear.
        Ty::Param(param) => !param_without_forget(db, &param.item, param.index),
        // Indirection, scalars, signatures, and everything broken.
        _ => true,
    }
}

/// Run `f` with `decl` marked in-progress, answering `true` if it is already
/// there. A declaration reached while computing its own answer is a
/// recursive type: the cycle adds no obligation of its own, so it
/// contributes `true` and the answer is decided by the non-recursive parts.
fn with_decl(
    seen: &mut Vec<ItemLoc>,
    decl: &ItemLoc,
    f: impl FnOnce(&mut Vec<ItemLoc>) -> bool,
) -> bool {
    if seen.contains(decl) {
        return true;
    }
    seen.push(decl.clone());
    let answer = f(seen);
    seen.pop();
    answer
}

/// Every type a value of this nominal mention CONTAINS: a struct's fields,
/// or every variant's payloads. Generic arguments are already substituted
/// (both projections do it), so this is the mention's own component list,
/// not the declaration's generic one.
fn named_components(db: &dyn Db, named: &NamedTy) -> Vec<Ty> {
    if let Some(Ty::Record(record)) = crate::ty::type_underlying_for(db, named) {
        return record.fields.iter().map(|(_, ty)| ty.clone()).collect();
    }
    let Some(variants) = crate::ty::enum_variants(db, named.decl.to_id(db)).as_ref() else {
        return Vec::new();
    };
    variants
        .iter()
        .flat_map(|(_, payload)| payload.iter())
        .map(|ty| crate::ty::substitute_args(ty, &named.decl, &named.args))
        .collect()
}

/// Whether the `type` declaration at `decl` wrote `without forget`.
pub fn decl_without_forget(db: &dyn Db, decl: &ItemLoc) -> bool {
    crate::item_data(db, decl.to_id(db))
        .as_ref()
        .is_some_and(|data| data.without_forget)
}

/// Whether the generic parameter at `(item, index)` opted out of the
/// default `forget` bound. A missing or non-type parameter answers `false`
/// — the bound is the default, and a broken binder does not relax it.
pub fn param_without_forget(db: &dyn Db, item: &ItemLoc, index: u32) -> bool {
    crate::item_data(db, item.to_id(db))
        .as_ref()
        .and_then(|data| data.generics.get(index as usize))
        .is_some_and(|param| param.without_forget)
}

/// The named-in-the-message half of [`no_forget_reason`]: the clause that
/// shed the capability, said as a sentence, plus the containment path back
/// to `ty`. A declaration IS declared `without forget`; a type parameter is
/// not declared anything — it opted out of a bound every parameter has by
/// default — so the two roots read differently.
fn blame(db: &dyn Db, ty: &Ty, seen: &mut Vec<ItemLoc>, path: &mut Vec<String>) -> Option<String> {
    match ty {
        Ty::Named(named) => {
            if decl_without_forget(db, &named.decl) {
                return Some(declared_without_forget(&named.decl));
            }
            if seen.contains(&named.decl) {
                return None;
            }
            seen.push(named.decl.clone());
            let found = blame_components(db, named, seen, path);
            seen.pop();
            found
        }
        Ty::Variant(variant) => {
            if decl_without_forget(db, &variant.decl) {
                return Some(declared_without_forget(&variant.decl));
            }
            if seen.contains(&variant.decl) {
                return None;
            }
            seen.push(variant.decl.clone());
            let found = blame_variant(db, variant, seen, path);
            seen.pop();
            found
        }
        Ty::Record(record) => record.fields.iter().find_map(|(name, field)| {
            let root = blame(db, field, seen, path)?;
            path.push(format!("through field `{name}`"));
            Some(root)
        }),
        Ty::Array { elem, .. } => {
            let root = blame(db, elem, seen, path)?;
            path.push("through the array's element type".to_owned());
            Some(root)
        }
        Ty::Param(param) if param_without_forget(db, &param.item, param.index) => Some(format!(
            "`{}` is a type parameter written `without forget`",
            param.name
        )),
        _ => None,
    }
}

fn declared_without_forget(decl: &ItemLoc) -> String {
    format!("`{}` is declared `without forget`", decl.display_name())
}

fn blame_components(
    db: &dyn Db,
    named: &NamedTy,
    seen: &mut Vec<ItemLoc>,
    path: &mut Vec<String>,
) -> Option<String> {
    if let Some(Ty::Record(record)) = crate::ty::type_underlying_for(db, named) {
        return record.fields.iter().find_map(|(name, field)| {
            let root = blame(db, field, seen, path)?;
            path.push(format!("through field `{name}`"));
            Some(root)
        });
    }
    let variants = crate::ty::enum_variants(db, named.decl.to_id(db))
        .as_ref()?
        .clone();
    variants.iter().enumerate().find_map(|(index, (name, _))| {
        let variant = VariantTy {
            decl: named.decl.clone(),
            args: named.args.clone(),
            index: index as u32,
            name: name.as_str().into(),
        };
        let payloads = crate::ty::variant_payloads_for(db, &variant)?;
        let root = payloads
            .iter()
            .find_map(|payload| blame(db, payload, seen, path))?;
        path.push(format!("through the `::{name}` payload"));
        Some(root)
    })
}

fn blame_variant(
    db: &dyn Db,
    variant: &VariantTy,
    seen: &mut Vec<ItemLoc>,
    path: &mut Vec<String>,
) -> Option<String> {
    let payloads = crate::ty::variant_payloads_for(db, variant)?;
    let root = payloads
        .iter()
        .find_map(|payload| blame(db, payload, seen, path))?;
    path.push(format!("through the `::{}` payload", variant.name));
    Some(root)
}
