//! Value layout: how a Must type occupies WebAssembly storage.
//!
//! **The slot model.** Every value is a flat sequence of `i64` *slots*.
//! A record is its fields' slots end to end, an array is its elements'
//! slots end to end, an enum is one tag slot followed by its payload
//! columns. Function values occupy ZERO slots — monomorphization resolves
//! every callee statically, so a fn value has nothing left to carry at
//! runtime (that is what "dictionaries die into direct calls" means
//! concretely). A safe borrow is ONE slot: the byte address of its
//! referent in linear memory. A value lives in wasm locals unless its
//! place is borrowed; then it lives in its function's shadow-stack frame
//! (see `emit.rs`), laid out in exactly the same slots, 8 bytes apiece.
//!
//! **Uniform `i64` is deliberate, and unobservable.** Storage width is not
//! semantics: what the language pins is the *operations* (`u8 + u8` traps
//! outside `0..=255`, `i32 / i32` traps on `MIN / -1`), and those are
//! right-sized in `emit.rs` regardless of how wide the slot holding them
//! is. A `u8` lives zero-extended in an `i64` slot, an `i8`
//! sign-extended. Nothing can observe the difference: safe code cannot
//! turn a borrow into an integer, and there is no `size_of`, no raw
//! pointer and no FFI in the supported subset.
//!
//! **Field order is MIR's canonical order — which for records means
//! NAME-SORTED, not definition order (X14).** The definition-ordering
//! doctrine says user-visible order is definition order and the internal
//! name-sorted canonicalization must never leak; at the MIR boundary the
//! canonical order is the only one that exists (`Ty::Record` sorts, and
//! `ProjElem::Field` indices are already resolved against that sort), so
//! the backend inherits it. Since layout is unobservable here, nothing
//! leaks. The moment layout *becomes* observable — FFI, raw pointers into
//! aggregates, `size_of` — definition order has to be plumbed down to the
//! backend.

use base_db::Db;
use hir::{ConstArgValue, GenericArg, Ty};

/// A construct this backend does not compile (yet). Carries the phrase
/// that names it in the refusal diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

impl Unsupported {
    pub fn new(what: impl Into<String>) -> Unsupported {
        Unsupported(what.into())
    }
}

pub type Layout<T> = Result<T, Unsupported>;

/// Types nest, and a `type List = struct { next: List }` would nest
/// forever. Without pointers such a type is uninhabited anyway, so a depth
/// cap is a complete answer.
const MAX_TYPE_DEPTH: u32 = 64;

/// How many `i64` slots a value of this type occupies.
pub fn slots(db: &dyn Db, ty: &Ty) -> Layout<u32> {
    slots_at(db, ty, 0)
}

fn slots_at(db: &dyn Db, ty: &Ty, depth: u32) -> Layout<u32> {
    if depth > MAX_TYPE_DEPTH {
        return Err(Unsupported::new("a recursive type"));
    }
    match ty {
        // Nothing to carry: unit, divergence, and — the point of
        // monomorphization — function values.
        Ty::Unit | Ty::Never | Ty::Fn(_) => Ok(0),
        // `char` is a scalar like the others: one slot holding the
        // Unicode scalar value.
        Ty::Bool | Ty::Int(_) | Ty::Char => Ok(1),
        // (offset, length) into the module's data segment.
        Ty::Str => Ok(2),
        Ty::Record(record) => {
            let mut total = 0;
            for (_, field) in &record.fields {
                total += slots_at(db, field, depth + 1)?;
            }
            Ok(total)
        }
        Ty::Array { elem, len } => {
            let len = array_len(len)?;
            let elem = slots_at(db, elem, depth + 1)?;
            Ok(elem * len)
        }
        Ty::Named(_) => match nominal(db, ty)? {
            Nominal::Struct(fields) => {
                let mut total = 0;
                for (_, field) in &fields {
                    total += slots_at(db, field, depth + 1)?;
                }
                Ok(total)
            }
            Nominal::Enum(variants) => Ok(enum_layout_at(db, &variants, depth)?.slots),
        },
        Ty::RawPtr { .. } => Err(Unsupported::new(
            "a raw pointer (heap and pointer primitives are out of scope for this backend)",
        )),
        // The referent's byte address. The aliasing contract is not this
        // backend's to check: the static loan check is the fence (M14),
        // and `mir::mir_checked` plants a trap wherever it refused.
        Ty::Borrow { .. } => Ok(1),
        Ty::Param(param) => Err(Unsupported::new(format!(
            "a value of the unresolved type parameter `{}`",
            param.name
        ))),
        Ty::Infer(_) | Ty::UnresolvedNumber => Err(Unsupported::new(
            "a value whose type inference never pinned down",
        )),
        Ty::Error => Err(Unsupported::new("a value of an erroneous type")),
    }
}

fn array_len(len: &ConstArgValue) -> Layout<u32> {
    match len {
        ConstArgValue::Int(n) => u32::try_from(*n).map_err(|_| {
            Unsupported::new(format!("an array of {n} elements (too large to flatten)"))
        }),
        other => Err(Unsupported::new(format!(
            "an array whose length `{}` is not a known number",
            other.display()
        ))),
    }
}

/// What a `ProjElem::Deref` step lands on: a safe borrow's referent. A
/// raw pointer stays refused — its validity is the interpreter's runtime
/// check, which compiled code has no way to repeat.
pub fn referent(ty: &Ty) -> Layout<Ty> {
    match ty {
        Ty::Borrow { referent, .. } => Ok((**referent).clone()),
        Ty::RawPtr { .. } => Err(Unsupported::new(
            "a raw-pointer dereference (heap and pointer primitives are out of scope \
             for this backend)",
        )),
        other => Err(Unsupported::new(format!(
            "dereferencing `{}`, which is not a borrow",
            other.display()
        ))),
    }
}

/// What a `Ty::Named` mention projects to.
pub enum Nominal {
    Struct(Vec<(String, Ty)>),
    /// `(name, payload types)` per variant, in declaration order.
    Enum(Vec<(String, Vec<Ty>)>),
}

pub fn nominal(db: &dyn Db, ty: &Ty) -> Layout<Nominal> {
    let Ty::Named(named) = ty else {
        return Err(Unsupported::new("a non-nominal type"));
    };
    if let Some(Ty::Record(record)) = hir::type_underlying_for(db, named) {
        return Ok(Nominal::Struct(record.fields.clone()));
    }
    let item = named.decl.to_id(db);
    if let Some(variants) = hir::enum_variants(db, item).as_ref() {
        let substituted = variants
            .iter()
            .map(|(name, payload)| {
                (
                    name.clone(),
                    payload
                        .iter()
                        .map(|ty| hir::substitute_args(ty, &named.decl, &named.args))
                        .collect(),
                )
            })
            .collect();
        return Ok(Nominal::Enum(substituted));
    }
    Err(Unsupported::new(format!(
        "a value of the broken type `{}`",
        ty.display()
    )))
}

/// The layout of an enum value: slot 0 is the variant index, and
/// payload element *j* of EVERY variant lives at `columns[j]`.
///
/// Column-wise, not variant-wise: element `j`'s column is as wide as the
/// widest variant's element `j`, so a payload read needs the element
/// position only — never the variant. That is what makes
/// `ValueExpr::Field` on an enum value compile to a plain slot copy in an
/// arm, without the backend having to re-derive which variant the arm
/// matched.
pub struct EnumLayout {
    pub slots: u32,
    /// Slot offset of payload element `j`, for every position any variant
    /// declares.
    pub columns: Vec<u32>,
    pub variants: Vec<(String, Vec<Ty>)>,
}

impl EnumLayout {
    /// How many slots payload column `position` spans: up to the next
    /// column, or to the end of the value for the last one.
    pub fn column_width(&self, position: usize) -> u32 {
        let next = self
            .columns
            .get(position + 1)
            .copied()
            .unwrap_or(self.slots);
        next - self.columns[position]
    }
}

pub fn enum_layout(db: &dyn Db, ty: &Ty) -> Layout<EnumLayout> {
    let Nominal::Enum(variants) = nominal(db, ty)? else {
        return Err(Unsupported::new(format!(
            "`{}` is not an enum type",
            ty.display()
        )));
    };
    enum_layout_at(db, &variants, 0)
}

fn enum_layout_at(db: &dyn Db, variants: &[(String, Vec<Ty>)], depth: u32) -> Layout<EnumLayout> {
    let arity = variants
        .iter()
        .map(|(_, payload)| payload.len())
        .max()
        .unwrap_or(0);
    let mut columns = Vec::with_capacity(arity);
    let mut offset = 1; // slot 0 is the tag
    for position in 0..arity {
        columns.push(offset);
        let mut widest = 0;
        for (_, payload) in variants {
            if let Some(ty) = payload.get(position) {
                widest = widest.max(slots_at(db, ty, depth + 1)?);
            }
        }
        offset += widest;
    }
    Ok(EnumLayout {
        slots: offset,
        columns,
        variants: variants.to_vec(),
    })
}

/// Where field `index` of an aggregate starts, and — where it is
/// knowable — what type it has. Handles every carrier
/// `mir::ValueExpr::Field` / `mir::ProjElem::Field` can name: records, named
/// structs, and enums (positional, through the column layout).
pub fn field(db: &dyn Db, ty: &Ty, index: u32) -> Layout<FieldRead> {
    let position = index as usize;
    match ty {
        Ty::Record(record) => {
            let mut offset = 0;
            for (_, field) in record.fields.iter().take(position) {
                offset += slots(db, field)?;
            }
            let ty = record
                .fields
                .get(position)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| {
                    Unsupported::new(format!("field {position} of a record without one"))
                })?;
            Ok(FieldRead::Exact { offset, ty })
        }
        Ty::Named(_) => match nominal(db, ty)? {
            Nominal::Struct(fields) => {
                let mut offset = 0;
                for (_, field) in fields.iter().take(position) {
                    offset += slots(db, field)?;
                }
                let ty = fields
                    .get(position)
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| {
                        Unsupported::new(format!("field {position} of a record without one"))
                    })?;
                Ok(FieldRead::Exact { offset, ty })
            }
            // A payload read out of an enum value. The column layout
            // makes the OFFSET independent of the variant — the TYPE is
            // not: two variants may declare different types at the same
            // position, and MIR's `ValueExpr::Field` carries no variant
            // index, so nothing here can say which one this read landed
            // on. `FieldRead::Column` therefore carries no type at all
            // (X10): a caller must size the read from a fact it already
            // has — the destination the read is bound to — rather than
            // guessing from the first variant that declares the position.
            Nominal::Enum(variants) => {
                let layout = enum_layout_at(db, &variants, 0)?;
                let offset = *layout.columns.get(position).ok_or_else(|| {
                    Unsupported::new(format!("payload element {position} of a value without one"))
                })?;
                Ok(FieldRead::Column { offset })
            }
        },
        other => Err(Unsupported::new(format!(
            "a field of `{}`",
            other.display()
        ))),
    }
}

/// What [`field`] found: an aggregate element whose type is known, or an
/// enum's payload column, whose type is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldRead {
    /// A record field or a named struct's field: the offset, and the
    /// field's exact type.
    Exact { offset: u32, ty: Ty },
    /// A payload column of an enum. The offset is exact; the width
    /// must come from the READ's destination, and no type is available
    /// (see [`field`]). A payload element always starts at its column
    /// offset, and the column is as wide as the widest variant's element
    /// there, so a destination-sized read is both correct and in bounds.
    Column { offset: u32 },
}

impl FieldRead {
    pub fn offset(&self) -> u32 {
        match self {
            FieldRead::Exact { offset, .. } | FieldRead::Column { offset } => *offset,
        }
    }

    /// The element's type, when it is knowable. `None` for an
    /// enum's payload column — a caller that needs a type there must
    /// refuse rather than guess.
    pub fn ty(&self) -> Option<&Ty> {
        match self {
            FieldRead::Exact { ty, .. } => Some(ty),
            FieldRead::Column { .. } => None,
        }
    }
}

/// The element type of an array type, and how wide one element is.
pub fn element(db: &dyn Db, ty: &Ty) -> Layout<(Ty, u32, u32)> {
    let Ty::Array { elem, len } = ty else {
        return Err(Unsupported::new(format!(
            "indexing into `{}`",
            ty.display()
        )));
    };
    let len = array_len(len)?;
    let width = slots(db, elem)?;
    Ok(((**elem).clone(), width, len))
}

/// How to compare two values of a type slot by slot: one entry per slot,
/// saying whether it is an ordinary value slot or the first half of a
/// `str` (offset, length) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqSlot {
    /// Compare with `i64.eq`.
    Value,
    /// This slot and the next hold a `(offset, length)` pair: compare the
    /// bytes.
    StrStart,
    /// The length half of a `str` — consumed by the [`EqSlot::StrStart`]
    /// before it.
    StrLen,
}

/// The slot-by-slot equality plan for a type.
///
/// Enums carrying a `str` payload are the one refusal: with a column
/// layout, two values of *different* variants can put a `str` and a
/// number in the same column, and a byte comparison against a number
/// would be nonsense (and could read outside memory). Every other enum
/// compares slot-wise.
pub fn eq_plan(db: &dyn Db, ty: &Ty) -> Layout<Vec<EqSlot>> {
    let mut plan = Vec::new();
    eq_plan_into(db, ty, &mut plan, 0)?;
    Ok(plan)
}

fn eq_plan_into(db: &dyn Db, ty: &Ty, out: &mut Vec<EqSlot>, depth: u32) -> Layout<()> {
    if depth > MAX_TYPE_DEPTH {
        return Err(Unsupported::new("a recursive type"));
    }
    match ty {
        Ty::Unit | Ty::Never | Ty::Fn(_) => Ok(()),
        Ty::Bool | Ty::Int(_) | Ty::Char => {
            out.push(EqSlot::Value);
            Ok(())
        }
        Ty::Str => {
            out.push(EqSlot::StrStart);
            out.push(EqSlot::StrLen);
            Ok(())
        }
        Ty::Record(record) => {
            for (_, field) in &record.fields {
                eq_plan_into(db, field, out, depth + 1)?;
            }
            Ok(())
        }
        Ty::Array { elem, len } => {
            for _ in 0..array_len(len)? {
                eq_plan_into(db, elem, out, depth + 1)?;
            }
            Ok(())
        }
        Ty::Named(_) => match nominal(db, ty)? {
            Nominal::Struct(fields) => {
                for (_, field) in &fields {
                    eq_plan_into(db, field, out, depth + 1)?;
                }
                Ok(())
            }
            Nominal::Enum(variants) => {
                if variants
                    .iter()
                    .any(|(_, payload)| payload.iter().any(|ty| mentions_str(db, ty, 0)))
                {
                    return Err(Unsupported::new(format!(
                        "`==` on `{}` (an enum carrying `str` payloads)",
                        ty.display()
                    )));
                }
                let layout = enum_layout_at(db, &variants, depth)?;
                for _ in 0..layout.slots {
                    out.push(EqSlot::Value);
                }
                Ok(())
            }
        },
        other => Err(Unsupported::new(format!("`==` on `{}`", other.display()))),
    }
}

fn mentions_str(db: &dyn Db, ty: &Ty, depth: u32) -> bool {
    if depth > MAX_TYPE_DEPTH {
        return true;
    }
    match ty {
        Ty::Str => true,
        Ty::Record(record) => record
            .fields
            .iter()
            .any(|(_, field)| mentions_str(db, field, depth + 1)),
        Ty::Array { elem, .. } => mentions_str(db, elem, depth + 1),
        Ty::Named(_) => match nominal(db, ty) {
            Ok(Nominal::Struct(fields)) => fields
                .iter()
                .any(|(_, field)| mentions_str(db, field, depth + 1)),
            Ok(Nominal::Enum(variants)) => variants
                .iter()
                .any(|(_, payload)| payload.iter().any(|ty| mentions_str(db, ty, depth + 1))),
            Err(_) => true,
        },
        _ => false,
    }
}

/// Substitute an instance's generic arguments through a type — the step
/// that turns MIR's decl-keyed (erased) types into the concrete types a
/// layout can be computed from.
pub fn substitute(ty: &Ty, item: &hir::ItemLoc, args: &[GenericArg]) -> Ty {
    if args.is_empty() {
        return ty.clone();
    }
    hir::substitute_args(ty, item, args)
}
