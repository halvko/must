//! The type representation and the per-item signature query.

use std::sync::Arc;

use base_db::{Db, SourceFile};
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::FxHashMap;

use crate::item_tree::{
    ConstArgRef, GenericArgRef, GenericParamKind, RegionRef, TypeRef, type_decl,
};
use crate::scopes::{Resolution, type_scope};
use crate::{ItemId, ItemLoc};

/// One of the integer scalar types: signed/unsigned × 8/16/32/64 bits,
/// plus the pointer-sized pair. `usize` remains the index/count type
/// everywhere (indexing, array lengths, alloc counts); the interpreter
/// models `usize`/`isize` as 64-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    Isize,
}

impl IntKind {
    pub const ALL: [IntKind; 10] = [
        IntKind::I8,
        IntKind::I16,
        IntKind::I32,
        IntKind::I64,
        IntKind::U8,
        IntKind::U16,
        IntKind::U32,
        IntKind::U64,
        IntKind::Usize,
        IntKind::Isize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            IntKind::I8 => "i8",
            IntKind::I16 => "i16",
            IntKind::I32 => "i32",
            IntKind::I64 => "i64",
            IntKind::U8 => "u8",
            IntKind::U16 => "u16",
            IntKind::U32 => "u32",
            IntKind::U64 => "u64",
            IntKind::Usize => "usize",
            IntKind::Isize => "isize",
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntKind::I8 | IntKind::I16 | IntKind::I32 | IntKind::I64 | IntKind::Isize
        )
    }

    pub fn by_name(name: &str) -> Option<IntKind> {
        IntKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.name() == name)
    }
}

/// The checked-arithmetic surface an [`IntValue`] variant's carrier must
/// offer — the ONLY way the interpreter touches a typed integer's value, so
/// the "value fits its kind" invariant can never be violated by an
/// operation (every op is a primitive `checked_*`, and every result is a
/// right-sized primitive again). Implemented for the Rust primitives
/// `u8`..`i64` via the macro below; `usize`/`isize` reuse the 64-bit impls
/// (the interpreter's pointer width is 64 bits, see `IntKind`).
pub(crate) trait Number: Copy {
    fn checked_add(self, rhs: Self) -> Option<Self>;
    fn checked_sub(self, rhs: Self) -> Option<Self>;
    fn checked_mul(self, rhs: Self) -> Option<Self>;
    fn checked_div(self, rhs: Self) -> Option<Self>;
    fn checked_neg(self) -> Option<Self>;
    /// The value widened to the interpreter's `i128` carrier — lossless for
    /// every implementor (all fit in `i128`), used for display and for the
    /// same-kind comparison the machine runs.
    fn to_i128(self) -> i128;
    /// The value narrowed from the `i128` carrier, or `None` when it does
    /// not fit — the range check that makes an out-of-range [`IntValue`]
    /// unrepresentable.
    fn from_i128(value: i128) -> Option<Self>;
}

macro_rules! impl_number {
    ($($t:ty),*) => {$(
        impl Number for $t {
            fn checked_add(self, rhs: Self) -> Option<Self> { <$t>::checked_add(self, rhs) }
            fn checked_sub(self, rhs: Self) -> Option<Self> { <$t>::checked_sub(self, rhs) }
            fn checked_mul(self, rhs: Self) -> Option<Self> { <$t>::checked_mul(self, rhs) }
            fn checked_div(self, rhs: Self) -> Option<Self> { <$t>::checked_div(self, rhs) }
            fn checked_neg(self) -> Option<Self> { <$t>::checked_neg(self) }
            fn to_i128(self) -> i128 { self as i128 }
            fn from_i128(value: i128) -> Option<Self> { <$t>::try_from(value).ok() }
        }
    )*};
}

impl_number!(i8, i16, i32, i64, u8, u16, u32, u64);

/// The ONE canonical `(variant, carrier)` list behind [`IntValue`]: the
/// enum itself and EVERY dispatch over it (kind, construction, widening,
/// negation, the binary ops) are generated from this list, so a future kind
/// (`u128`, `i128`) is added here and nowhere else — the moment it enters
/// the list, every dispatch grows its arm in the same expansion, and
/// [`IntValue::new`]'s generated match on [`IntKind`] refuses to compile
/// until the kind exists there too (and vice versa: a new [`IntKind`]
/// variant makes that match non-exhaustive). No generated dispatch carries
/// a bare `_` arm, so a variant can never silently fall through to a wrong
/// answer.
macro_rules! with_int_value_variants {
    ($m:ident) => {
        $m! {
            (I8, i8),
            (I16, i16),
            (I32, i32),
            (I64, i64),
            (U8, u8),
            (U16, u16),
            (U32, u32),
            (U64, u64),
            (Usize, u64),
            (Isize, i64)
        }
    };
}

macro_rules! declare_int_value {
    ($(($variant:ident, $carrier:ty)),* $(,)?) => {
        /// A typed integer value — the carrier both `mir::Const` and
        /// `eval::Value` use: the machine's typed-memory philosophy applied
        /// to scalars (every runtime integer knows its width). One variant
        /// per [`IntKind`], each holding its right-sized carrier, so "the
        /// value fits its kind" is a STRUCTURAL invariant — an out-of-range
        /// value is simply unrepresentable. `usize`/`isize` map to the
        /// 64-bit carriers (the interpreter's pointer width). All
        /// construction from a wide `i128` goes through the one fallible
        /// [`IntValue::new`], and all arithmetic through the checked
        /// `Number` surface, so neither can produce an out-of-range value.
        /// Generated — variants and all dispatch — from the single
        /// `with_int_value_variants` list.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum IntValue {
            $($variant($carrier),)*
        }

        impl IntValue {
            /// The integer type of this value.
            pub fn kind(self) -> IntKind {
                match self {
                    $(IntValue::$variant(_) => IntKind::$variant,)*
                }
            }

            /// The value widened to the interpreter's `i128` carrier —
            /// lossless, for display and same-kind comparison.
            pub fn to_i128(self) -> i128 {
                match self {
                    $(IntValue::$variant(v) => Number::to_i128(v),)*
                }
            }

            /// The one fallible construction from a wide value: `Some` iff
            /// `value` fits `kind`, otherwise `None` (the value is
            /// unrepresentable at that width). This is where literal range
            /// checking lands.
            pub fn new(kind: IntKind, value: i128) -> Option<IntValue> {
                Some(match kind {
                    $(IntKind::$variant => IntValue::$variant(Number::from_i128(value)?),)*
                })
            }

            /// `-self`, or `None` on overflow (every non-zero unsigned
            /// operand, and the signed minimum) — the checked negation the
            /// machine's unary `-` runs.
            pub fn checked_neg(self) -> Option<IntValue> {
                Some(match self {
                    $(IntValue::$variant(v) => IntValue::$variant(Number::checked_neg(v)?),)*
                })
            }

            declare_int_value_binops! {
                [$(($variant, $carrier)),*]
                checked_add checked_sub checked_mul checked_div
            }
        }
    };
}

/// The checked binary ops, each dispatching a `Number` op over two
/// SAME-KIND [`IntValue`]s and re-wrapping the result in that variant.
/// Mixed kinds (impossible in a checked program — the machine traps them
/// ill-typed before calling these) yield `None`. The mismatch arms
/// enumerate every left-hand variant from the same canonical list as the
/// same-kind arms — no bare `_` arm — so a variant added to the list gets
/// its same-kind arm in the same expansion and can never be misreported as
/// a mismatch/overflow.
macro_rules! declare_int_value_binops {
    ([$(($variant:ident, $carrier:ty)),*]) => {};
    ([$(($variant:ident, $carrier:ty)),*] $name:ident $($rest:ident)*) => {
        pub fn $name(self, rhs: IntValue) -> Option<IntValue> {
            Some(match (self, rhs) {
                $(
                    (IntValue::$variant(a), IntValue::$variant(b)) => {
                        IntValue::$variant(Number::$name(a, b)?)
                    }
                )*
                $(
                    (IntValue::$variant(_), _) => return None,
                )*
            })
        }
        declare_int_value_binops! { [$(($variant, $carrier)),*] $($rest)* }
    };
}

with_int_value_variants!(declare_int_value);

impl IntValue {
    // The kind-pinned extractors live OUTSIDE the generated dispatch on
    // purpose: each names exactly one variant, and every other kind —
    // including any future one — must keep answering `None` (the checker
    // pins these argument positions; a wrong kind is deferred-error mode).

    /// The payload if this is the `usize` kind, widened for index/count
    /// arithmetic — `None` for every other kind, which the pinning builtins
    /// trap (`add`'s index, the alloc/copy counts, repeat counts, indexing).
    pub fn usize_payload(self) -> Option<u128> {
        match self {
            IntValue::Usize(v) => Some(u128::from(v)),
            _ => None,
        }
    }

    /// The payload if this is the `isize` kind, widened for offset
    /// arithmetic — `None` for every other kind; `offset`'s argument pins
    /// this.
    pub fn isize_payload(self) -> Option<i128> {
        match self {
            IntValue::Isize(v) => Some(i128::from(v)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// An unresolved inference variable. Appears in [`crate::infer::InferenceResult`]
    /// only when inference couldn't pin the type down (rendered as `_`).
    Infer(TyVar),
    /// An integer literal's NUMBER-CLASS inference variable that no
    /// defining use ever pinned. Appears only in *finished*
    /// [`crate::infer::InferenceResult`]s (during inference the number
    /// flavor lives in the unification table — see
    /// [`TyVarValue::UnknownNumber`]); rendered as `{number}`. Never
    /// defaulted: the unresolved case is a diagnostic, not a silent pick.
    UnresolvedNumber,
    Unit,
    /// `!`: the bottom type; coerces to any expected type.
    Never,
    /// One of the integer scalar types (`usize`, `u8`, `i64`, ...).
    Int(IntKind),
    Str,
    Bool,
    /// `char`: ONE Unicode scalar value — a codepoint that is not a
    /// surrogate. A primitive of its own, never an integer alias: `char`
    /// has equality and nothing else (no ordering, no arithmetic), so the
    /// operations an integer offers are exactly the ones that would let a
    /// program build a value that is not a character.
    ///
    /// Scalar values, not raw codepoints, because UTF-8 cannot encode a
    /// surrogate: admitting them would make every encoder — `str`
    /// construction, `print`, the wasm data segment — fallible for values
    /// no text ever contains. The narrower type pays for itself at the one
    /// place it costs anything — decoding text, which reads from
    /// already-valid UTF-8 and so can never produce one.
    Char,
    Fn(Arc<FnTy>),
    /// `{ x: usize, y: str }`: a structural record. Two record types are the
    /// same type exactly when their field sets are equal (same names, same
    /// types) — no subtyping, no width coercion. Fields are always sorted by
    /// name (see [`Ty::record`]), so plain equality is field-set equality.
    Record(Arc<RecordTy>),
    /// A nominal type declared by a `type` item. Identity is the
    /// declaration (the same range-free scheme as [`ItemLoc`]) plus the
    /// FULL generic-argument list in binder order — applicative: same
    /// declaration + same args = the same type everywhere, and two
    /// `Named`s unify exactly when the declarations match and the args
    /// unify pointwise (no variance of any kind). A `Named` never unifies
    /// with the structurally-identical bare record — no implicit
    /// nominal↔structural coercion in either direction. The declared shape
    /// is *not* carried here; it is projected on demand through
    /// [`type_underlying_for`] (field access, construction) and
    /// [`variant_payloads_for`], which substitute the args for the
    /// declaration's rigid params.
    Named(NamedTy),
    /// `Shape::Circle`: the type of one variant of an enum, a first-class
    /// type of its own. It does *not* unify with its enum — a variant-typed
    /// value is tag-free at runtime, an enum-typed value is tagged, so
    /// going variant → enum is a real conversion ([`widens_to`]), never
    /// equality. Payload types are not carried here (identity only, like
    /// [`Ty::Named`] — though the enum's generic args are, so the widening
    /// conversion preserves them); they are projected through
    /// [`variant_payloads_for`].
    Variant(VariantTy),
    /// `T.&raw` / `T.&raw mut`: a raw pointer. Unifies exactly and
    /// equationally like everything else — same mutability, pointwise
    /// pointee — with NO variance (there is no subtyping to be variant
    /// over) and no implicit `T.&raw mut` → `T.&raw` conversion in v1
    /// (held open as a future shallow [`widens_to`] arm, house style,
    /// never subtyping).
    RawPtr {
        mutable: bool,
        pointee: Arc<Ty>,
    },
    /// `T.&::<@a>` / `T.&mut::<@a>`: a SAFE borrow. Carries its referent,
    /// its mutability and its [`Region`].
    ///
    /// Regions are related INVARIANTLY: unification of two borrow types
    /// unifies the referents and emits outlives constraints in BOTH
    /// directions (see `Constraints::relate_regions`), never an equality
    /// merge — the one implementation instruction that keeps variance a
    /// later, additive change instead of a rewrite. There is no
    /// `T.&mut` → `T.&` subtyping either: degradation is a REBORROW (the
    /// region shrinks and the parent suspends), which is a checker event,
    /// not a lattice edge.
    ///
    /// The region is part of this type's identity INSIDE hir, and nowhere
    /// else: [`Ty::erase_regions`] runs at the MIR boundary, so MIR,
    /// instance keys and the backend never see one.
    Borrow {
        mutable: bool,
        region: Region,
        referent: Arc<Ty>,
    },
    /// `[T; N]`: a fixed-size array — structural, like [`Ty::Record`]
    /// (never a `Named` declaration). Unifies exactly: element pointwise,
    /// length by plain equality with [`ConstArgValue::Error`] infectious
    /// (mirroring generic const-arg unification) — `[T; 8]` and `[T; 9]`
    /// never unify, and there is no variance and no implicit conversion.
    /// The length lives in the annotation-representable const domain
    /// (literal or rigid const param), never a computed value.
    Array {
        elem: Arc<Ty>,
        len: ConstArgValue,
    },
    /// A rigid type parameter of a generic item (`T` in `fn::<T>`).
    /// Identity is `(item, index)` — the declaring item plus the position
    /// in its binder — the same range-free scheme as [`Ty::Named`] /
    /// [`VariantTy`]. Rigid: it unifies ONLY with itself (never with a
    /// concrete type, never bound like an inference variable), widens to
    /// nothing and nothing widens to it — inside the generic body the
    /// param is fully opaque (pass/store/return/`==` work; field access,
    /// calls, arithmetic get the ordinary diagnostics). Mentions of the
    /// item *instantiate* the signature, replacing these with fresh
    /// variables — a `Param` never leaks into a non-generic body's types.
    Param(ParamTy),
    /// Type of broken code. Infectious and silent: producing further
    /// diagnostics from an `Error` type would only be noise.
    Error,
}

/// A region: how long a borrow is good for.
///
/// Two populations, and the split is the whole design. A region written in a
/// SIGNATURE is UNIVERSAL — a rigid [`Region::Param`], opaque to the body,
/// which the body is checked *against*. A region arising inside a BODY is
/// EXISTENTIAL — a [`Region::Var`] the outlives module solves for. Nothing
/// infers a signature's regions from a body: that would make callers depend
/// on callee bodies and blame go cross-body, which the server-first
/// architecture forbids.
///
/// Regions never reach MIR. [`Ty::erase_regions`] maps every one to
/// [`Region::Erased`] at that boundary, which is what keeps borrow checking
/// a decl-level query (one check per body, not one per instantiation) and
/// keeps regions out of instance keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Region {
    /// `@a` declared by an enclosing binder — rigid and universal. Same
    /// `(item, index)` identity scheme as [`Ty::Param`] and
    /// [`ConstArgValue::Param`]; the name (sigil included) is display-only.
    Param {
        item: ItemLoc,
        index: u32,
        name: std::sync::Arc<str>,
    },
    /// A body-local existential region — an inference variable, identified
    /// by a per-body index. Minted by `@_`, by every borrow expression, and
    /// by every reborrow.
    Var(RegionVar),
    /// `@a + @b` — the join: outlived by every member. Kept structural
    /// rather than solved on sight, so blame can point at the written join.
    Join(Vec<Region>),
    /// The erased region — what every region becomes at the MIR boundary.
    /// Two borrow types differing only in their regions are the SAME type
    /// to MIR, codegen and instance keys.
    Erased,
    /// A region argument that was missing, of the wrong kind, or naming
    /// nothing in scope. Infectious and silent, like [`Ty::Error`].
    Error,
}

/// A body-local region inference variable's identity — its index in the
/// inference context's region table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionVar(pub u32);

impl Region {
    pub fn display(&self) -> String {
        match self {
            Region::Param { name, .. } => name.to_string(),
            // Reached only where a region is rendered on its own (never
            // through `Ty::display`, which elides the turbofish instead).
            Region::Var(_) => "@_".to_owned(),
            Region::Join(parts) => parts
                .iter()
                .map(Region::display)
                .collect::<Vec<_>>()
                .join(" + "),
            // Erased regions only exist below the checker, where nothing
            // renders types to users; showing the sigil alone is honest.
            Region::Erased => "@".to_owned(),
            Region::Error => "@{error}".to_owned(),
        }
    }

    /// Every region variable mentioned, including inside a join.
    pub fn vars(&self, out: &mut Vec<RegionVar>) {
        match self {
            Region::Var(var) => out.push(*var),
            Region::Join(parts) => parts.iter().for_each(|part| part.vars(out)),
            _ => {}
        }
    }
}

/// Identity of a rigid type parameter: the declaring item plus the
/// parameter's position in the item's full generic binder (type and const
/// params share one index space). The name is carried for display; it is
/// determined by `(item, index)`, so including it in equality is harmless
/// (same reasoning as [`VariantTy`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamTy {
    /// The declaring generic item.
    pub item: ItemLoc,
    /// Position in the item's binder (`ItemData::generics`).
    pub index: u32,
    pub name: std::sync::Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// Identity of a nominal type mention: the declaration plus the full
/// generic-argument list in binder order. Non-generic declarations carry an
/// empty list. Range-free by construction (an [`ItemLoc`], types, and
/// literal const values) — nothing position-dependent may ever enter this
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamedTy {
    pub decl: ItemLoc,
    pub args: Vec<GenericArg>,
}

impl NamedTy {
    /// A mention of a non-generic declaration.
    pub fn plain(decl: ItemLoc) -> NamedTy {
        NamedTy {
            decl,
            args: Vec::new(),
        }
    }
}

/// One generic argument of a [`NamedTy`]/[`VariantTy`], in binder order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArg {
    /// A REGION argument. Only reachable through a region param on a TYPE
    /// declaration, which is reserved for a later arc (variance and
    /// well-formedness are undecided); the variant exists so binder arity
    /// stays honest at such a mention instead of silently shifting the
    /// other arguments' positions.
    Region(Region),
    Ty(Ty),
    Const(ConstArgValue),
}

/// A type-level *const* argument value. This is the annotation-representable
/// const domain — literal values and rigid const params — NOT the full
/// const-eval value domain: const eval never runs on the annotation path
/// (the firewall), so a `const { ... }` block can never parameterize a type
/// and everything here is readable straight off the syntax.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstArgValue {
    Int(u128),
    Str(std::sync::Arc<str>),
    Bool(bool),
    Char(char),
    /// A rigid const parameter of an enclosing generic binder (`N` in
    /// `Buf::<N>` inside another generic body) — the const-side sibling of
    /// [`Ty::Param`], with the same `(item, index)` identity scheme (the
    /// name is display-only, determined by the identity).
    Param {
        item: ItemLoc,
        index: u32,
        name: std::sync::Arc<str>,
    },
    /// Broken or unrepresentable (diagnosed at the use site). Infectious
    /// and silent in comparisons, like [`Ty::Error`]: it unifies with any
    /// const value so broken mentions don't cascade.
    Error,
}

impl ConstArgValue {
    pub fn display(&self) -> String {
        match self {
            ConstArgValue::Int(v) => v.to_string(),
            ConstArgValue::Str(s) => format!("{s:?}"),
            ConstArgValue::Bool(b) => b.to_string(),
            ConstArgValue::Char(c) => format!("{c:?}"),
            ConstArgValue::Param { name, .. } => name.to_string(),
            ConstArgValue::Error => "{error}".to_owned(),
        }
    }
}

impl GenericArg {
    pub fn display(&self) -> String {
        match self {
            GenericArg::Region(region) => region.display(),
            GenericArg::Ty(ty) => ty.display(),
            GenericArg::Const(value) => value.display(),
        }
    }
}

/// Render a generic-argument list as `::<a, b>`; empty for an empty list.
fn display_args(args: &[GenericArg]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let list = args
        .iter()
        .map(GenericArg::display)
        .collect::<Vec<_>>()
        .join(", ");
    format!("::<{list}>")
}

/// Identity of a variant type: the enum declaration plus the variant's
/// position in it. The name is carried for display (and for the runtime
/// value's DAP rendering); it is determined by `(decl, index)`, so including
/// it in equality is harmless.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariantTy {
    /// The declaring `type ... = enum { ... };` item.
    pub decl: ItemLoc,
    /// The enum mention's generic arguments — the same full-list identity
    /// as [`NamedTy::args`], carried so variant → enum widening preserves
    /// them and payload projection can substitute.
    pub args: Vec<GenericArg>,
    /// Position in the declaration's source-order variant list.
    pub index: u32,
    pub name: std::sync::Arc<str>,
}

/// The implicit-conversion lattice, checked where unification fails at a
/// check site (an annotation, a call argument, a join edge): `!` widens to
/// everything (divergence produces no value to convert), and a variant type
/// widens to *its* enum — that one is a real runtime conversion (the tag is
/// injected; see `mir`'s `WidenToEnum`). Nothing else widens, and only
/// shallowly: no variance through `fn` types or record fields. Unification
/// itself stays equational — `unify(Variant, Named)` is false.
///
/// Declaration-level only: the enum's generic ARGS must additionally agree
/// (widening preserves them — `Option::<usize>::Some` widens to
/// `Option::<usize>`, never to `Option::<str>`). The two check sites
/// (`InferCtx::check` and the join solver) unify the args through
/// `Constraints::widen_to_enum`; this predicate alone is not the whole
/// judgement for a generic enum.
pub fn widens_to(actual: &Ty, expected: &Ty) -> bool {
    match (actual, expected) {
        (Ty::Never, _) => true,
        (Ty::Variant(variant), Ty::Named(named)) => variant.decl == named.decl,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordTy {
    /// Sorted by field name — the canonical order for equality, unification
    /// and display. Construct via [`Ty::record`] to keep the invariant.
    pub fields: Vec<(String, Ty)>,
}

impl RecordTy {
    pub fn field_ty(&self, name: &str) -> Option<&Ty> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, ty)| ty)
    }
}

impl Ty {
    pub fn fn_type(params: Vec<Ty>, ret: Ty) -> Ty {
        Ty::Fn(Arc::new(FnTy { params, ret }))
    }

    pub fn raw_ptr(mutable: bool, pointee: Ty) -> Ty {
        Ty::RawPtr {
            mutable,
            pointee: Arc::new(pointee),
        }
    }

    pub fn borrow(mutable: bool, region: Region, referent: Ty) -> Ty {
        Ty::Borrow {
            mutable,
            region,
            referent: Arc::new(referent),
        }
    }

    /// This type with EVERY region replaced by [`Region::Erased`] — the MIR
    /// boundary, and the reason borrow checking is a decl-level query.
    /// Applied to every type on its way out of hir into `mir`, so nothing
    /// below can accidentally observe a region.
    pub fn erase_regions(&self) -> Ty {
        match self {
            Ty::Borrow {
                mutable,
                referent,
                region: _,
            } => Ty::borrow(*mutable, Region::Erased, referent.erase_regions()),
            Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(*mutable, pointee.erase_regions()),
            Ty::Array { elem, len } => Ty::array(elem.erase_regions(), len.clone()),
            Ty::Fn(f) => Ty::fn_type(
                f.params.iter().map(Ty::erase_regions).collect(),
                f.ret.erase_regions(),
            ),
            Ty::Record(rec) => Ty::record(
                rec.fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.erase_regions()))
                    .collect(),
            ),
            Ty::Named(named) => Ty::Named(NamedTy {
                decl: named.decl.clone(),
                args: named.args.iter().map(erase_regions_arg).collect(),
            }),
            Ty::Variant(variant) => Ty::Variant(VariantTy {
                args: variant.args.iter().map(erase_regions_arg).collect(),
                ..variant.clone()
            }),
            other => other.clone(),
        }
    }

    pub fn array(elem: Ty, len: ConstArgValue) -> Ty {
        Ty::Array {
            elem: Arc::new(elem),
            len,
        }
    }

    /// A record type in canonical form: fields sorted by name. Duplicate
    /// names keep the FIRST occurrence (the stable sort preserves source
    /// order among equals) — validation already errors on the duplicate, so
    /// this is recovery, not semantics.
    pub fn record(mut fields: Vec<(String, Ty)>) -> Ty {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        fields.dedup_by(|second, first| second.0 == first.0);
        Ty::Record(Arc::new(RecordTy { fields }))
    }

    /// Whether any part of the type is an inference variable — after
    /// `resolve_fully`, an *unbound* one. `Named`/`Variant` are
    /// identity-only for the declared SHAPE (not carried here), but their
    /// generic args ARE the identity, so the traversal recurses into them
    /// (`Pair::<_>` is undetermined until the arg resolves).
    pub fn contains_infer(&self) -> bool {
        match self {
            Ty::Infer(_) | Ty::UnresolvedNumber => true,
            Ty::Fn(f) => f.ret.contains_infer() || f.params.iter().any(Ty::contains_infer),
            Ty::RawPtr { pointee, .. } => pointee.contains_infer(),
            // A borrow's REGION is never "undetermined" in this sense: an
            // unsolved region is the outlives module's business, never a
            // reason to demand a type annotation.
            Ty::Borrow { referent, .. } => referent.contains_infer(),
            // The length is never an inference variable (const args are
            // never inferred, TR06) — only the element can be undetermined.
            Ty::Array { elem, .. } => elem.contains_infer(),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_infer()),
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(GenericArg::contains_infer)
            }
            _ => false,
        }
    }

    /// Whether an fn type appears anywhere in the type. Same traversal
    /// shape (and the same declared-shape blind spot on `Named`/`Variant`,
    /// whose args are still visited) as [`Ty::contains_infer`]. Used by the
    /// mention-side belt that keeps fn values out of the const-arg domain
    /// (TR06) — the declaration-side twin checks the syntactic `TypeRef`
    /// (see `TypeRef::mentions_fn`).
    pub fn mentions_fn(&self) -> bool {
        match self {
            Ty::Fn(_) => true,
            Ty::RawPtr { pointee, .. } => pointee.mentions_fn(),
            Ty::Borrow { referent, .. } => referent.mentions_fn(),
            Ty::Array { elem, .. } => elem.mentions_fn(),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.mentions_fn()),
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(|arg| match arg {
                    GenericArg::Ty(ty) => ty.mentions_fn(),
                    GenericArg::Region(_) | GenericArg::Const(_) => false,
                })
            }
            _ => false,
        }
    }

    /// Whether an array type appears anywhere in the type — the mention-side
    /// belt keeping array VALUES out of the const-arg domain (the ruled
    /// domain is builtins + records + variants); the declaration-side twin
    /// is [`crate::item_tree::TypeRef::mentions_array`]. Same traversal
    /// shape (and the same nominal-opaque blind spot) as
    /// [`Ty::mentions_fn`].
    pub fn mentions_array(&self) -> bool {
        match self {
            Ty::Array { .. } => true,
            Ty::RawPtr { pointee, .. } => pointee.mentions_array(),
            Ty::Borrow { referent, .. } => referent.mentions_array(),
            Ty::Fn(f) => f.params.iter().any(Ty::mentions_array) || f.ret.mentions_array(),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.mentions_array()),
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(|arg| match arg {
                    GenericArg::Ty(ty) => ty.mentions_array(),
                    GenericArg::Region(_) | GenericArg::Const(_) => false,
                })
            }
            _ => false,
        }
    }

    /// Whether a borrow appears anywhere in this type.
    ///
    /// Asked by exactly one caller, and for a structural reason: any fast
    /// path that agrees two types WITHOUT relating their regions has to
    /// stand aside for a borrow, because `unify` is region-blind by design
    /// and "they agree" is therefore always true of two borrows that
    /// differ only in how long they live.
    pub fn contains_borrow(&self) -> bool {
        match self {
            Ty::Borrow { .. } => true,
            Ty::RawPtr { pointee, .. } => pointee.contains_borrow(),
            Ty::Array { elem, .. } => elem.contains_borrow(),
            Ty::Fn(f) => f.params.iter().any(Ty::contains_borrow) || f.ret.contains_borrow(),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_borrow()),
            // BLIND SPOT, and it is the one that matters the day region
            // parameters on type declarations are un-reserved: this reads
            // a nominal type's generic ARGS, never the fields of its
            // declaration. Today no `type` can name a region, so a
            // declaration cannot hide one — the args are the whole story.
            // The moment `struct::<@a, T>` becomes legal, a `Named` whose
            // declaration stores a borrow will answer `false` here and
            // slip through every guard this predicate protects.
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(|arg| match arg {
                    GenericArg::Ty(ty) => ty.contains_borrow(),
                    // A region argument is not a borrow, and a const one
                    // cannot contain one. Exhaustive on purpose (see
                    // `Constraints::freshen_regions_args`): this predicate
                    // guards every region-blind fast path, so the next
                    // argument kind must be decided here, not inherited.
                    GenericArg::Region(_) | GenericArg::Const(_) => false,
                })
            }
            _ => false,
        }
    }

    pub fn contains_error(&self) -> bool {
        match self {
            Ty::Error => true,
            Ty::Fn(f) => f.ret.contains_error() || f.params.iter().any(Ty::contains_error),
            Ty::RawPtr { pointee, .. } => pointee.contains_error(),
            // A broken region is this type's business — a borrow whose
            // region names nothing cannot check.
            Ty::Borrow {
                referent, region, ..
            } => referent.contains_error() || matches!(region, Region::Error),
            // A broken length is this type's business, exactly like a
            // broken generic const ARG on a `Named` mention.
            Ty::Array { elem, len } => elem.contains_error() || matches!(len, ConstArgValue::Error),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_error()),
            // The declared shape stays identity-only: a broken *declaration*
            // carries its own diagnostics at the declaration site; uses of
            // the name must not cascade. But the mention's own ARGS are
            // this type's business — a broken argument (`Pair::<Unknown>`)
            // makes the whole mention broken. A rigid `Param` is
            // identity-only too — and it is *not* an inference variable
            // either (`contains_infer` must stay false for it: a
            // `Param`-typed expectation is a real, concrete expectation,
            // never "no expectation").
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(GenericArg::contains_error)
            }
            Ty::Param(_) => false,
            _ => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::Infer(_) => "_".to_owned(),
            Ty::UnresolvedNumber => "{number}".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Never => "!".to_owned(),
            Ty::Int(kind) => kind.name().to_owned(),
            Ty::Str => "str".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Char => "char".to_owned(),
            Ty::Error => "{error}".to_owned(),
            Ty::Fn(f) => {
                let params = f
                    .params
                    .iter()
                    .map(Ty::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                match &f.ret {
                    Ty::Unit => format!("fn({params})"),
                    ret => format!("fn({params}) -> {}", ret.display()),
                }
            }
            Ty::RawPtr { mutable, pointee } => {
                if *mutable {
                    format!("{}.&raw mut", pointee.display())
                } else {
                    format!("{}.&raw", pointee.display())
                }
            }
            Ty::Borrow {
                mutable,
                region,
                referent,
            } => {
                let m = if *mutable { "mut" } else { "" };
                match region {
                    // Below the checker there IS no region, so no
                    // turbofish is printed: a MIR snapshot showing one
                    // would be advertising information that layer does not
                    // have.
                    //
                    // An unsolved VARIABLE prints bare for a different
                    // reason: `@_` is exactly what a signature may not
                    // say, so a message rendering one shows the user a
                    // type they are forbidden to write — and the mismatch
                    // is never about the region anyway.
                    Region::Erased | Region::Var(_) => format!("{}.&{m}", referent.display()),
                    region => format!("{}.&{m}::<{}>", referent.display(), region.display()),
                }
            }
            Ty::Array { elem, len } => {
                format!("[{}; {}]", elem.display(), len.display())
            }
            Ty::Record(rec) => {
                if rec.fields.is_empty() {
                    return "struct {}".to_owned();
                }
                let fields = rec
                    .fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", ty.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("struct {{ {fields} }}")
            }
            Ty::Named(named) => {
                format!("{}{}", named.decl.display_name(), display_args(&named.args))
            }
            Ty::Variant(variant) => {
                format!(
                    "{}{}::{}",
                    variant.decl.display_name(),
                    display_args(&variant.args),
                    variant.name
                )
            }
            Ty::Param(param) => param.name.to_string(),
        }
    }
}

impl GenericArg {
    fn contains_infer(&self) -> bool {
        match self {
            GenericArg::Ty(ty) => ty.contains_infer(),
            GenericArg::Region(_) | GenericArg::Const(_) => false,
        }
    }

    fn contains_error(&self) -> bool {
        match self {
            GenericArg::Ty(ty) => ty.contains_error(),
            GenericArg::Region(region) => matches!(region, Region::Error),
            GenericArg::Const(value) => matches!(value, ConstArgValue::Error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVar(u32);

impl UnifyKey for TyVar {
    type Value = TyVarValue;
    fn index(&self) -> u32 {
        self.0
    }
    fn from_index(i: u32) -> TyVar {
        TyVar(i)
    }
    fn tag() -> &'static str {
        "TyVar"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TyVarValue {
    Known(Ty),
    Unknown,
    /// A NUMBER-CLASS variable: still unknown, but restricted to unify
    /// only with integer scalar types ([`Ty::Int`]) and other number
    /// variables — the type an integer literal gets until a defining use
    /// (annotation, parameter type, index position, typed operand, ...)
    /// pins it. The restriction is enforced in `Constraints::unify`; the
    /// flavor survives unions through [`UnifyValue::unify_values`] below.
    UnknownNumber,
}

impl UnifyValue for TyVarValue {
    type Error = NoError;
    fn unify_values(a: &Self, b: &Self) -> Result<Self, NoError> {
        Ok(match (a, b) {
            // A poisoned variable stays poisoned: `Ty::Error` is infectious,
            // so it beats an earlier (wrong) commitment instead of the
            // commitment beating the poison.
            (TyVarValue::Known(Ty::Error), _) | (_, TyVarValue::Known(Ty::Error)) => {
                TyVarValue::Known(Ty::Error)
            }
            // Two `Known`s only meet after the inference context has already
            // unified them structurally (it resolves vars before unioning),
            // so keeping either is fine.
            (TyVarValue::Known(t), _) | (_, TyVarValue::Known(t)) => TyVarValue::Known(t.clone()),
            // Number-ness is infectious across var↔var merges: a plain
            // variable unified with a number variable is a number variable.
            (TyVarValue::UnknownNumber, _) | (_, TyVarValue::UnknownNumber) => {
                TyVarValue::UnknownNumber
            }
            (TyVarValue::Unknown, TyVarValue::Unknown) => TyVarValue::Unknown,
        })
    }
}

/// The nameable builtin types. Also the authority for "is this type name
/// known?" — diagnostics use `is_none` to report unknown type names.
pub fn builtin_type_by_name(name: &str) -> Option<Ty> {
    if let Some(kind) = IntKind::by_name(name) {
        return Some(Ty::Int(kind));
    }
    match name {
        "str" | "string" => Some(Ty::Str),
        "bool" => Some(Ty::Bool),
        "char" => Some(Ty::Char),
        _ => None,
    }
}

/// The generic binder of a declaration, for lowering to consult arity and
/// kinds. Empty for non-generic (and vanished) items.
fn decl_generics(db: &dyn Db, loc: &ItemLoc) -> Vec<crate::item_tree::GenericParamData> {
    crate::item_data(db, loc.to_id(db))
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default()
}

/// Resolve a name in *type* position: a `type` item wins, then the builtin
/// types, and a value item is no type at all (the diagnostics pass in
/// [`crate::file_diagnostics`] mirrors this order exactly — the two must
/// agree so every silent `Ty::Error` here has a diagnostic there).
///
/// A *generic* `type` item mentioned bare (no turbofish) is `Ty::Error`:
/// annotation lowering is syntactic, so the argument list must always be
/// spelled in type position (`Pair::<usize>`, or `Pair::<_>` where
/// inference may fill it) — the mirror pass reports the arity.
///
/// Goes through [`type_scope`] — not [`crate::scopes::file_scope`] — so
/// annotation lowering (and with it `infer`/`signature`) only depends on
/// the file's *type* items and backdates across value-item edits.
fn lower_type_path(db: &dyn Db, file: SourceFile, name: &str) -> Ty {
    match type_scope(db, file).resolve(name) {
        Some(Resolution::TypeItem(loc)) => {
            if decl_generics(db, &loc).is_empty() {
                Ty::Named(NamedTy::plain(loc))
            } else {
                Ty::Error
            }
        }
        // The duplicate definitions carry the diagnostics.
        Some(Resolution::Ambiguous(_)) => Ty::Error,
        // A value item (invisible here) or nothing: only a builtin type
        // name can save it.
        _ => builtin_type_by_name(name).unwrap_or(Ty::Error),
    }
}

/// Resolve `Enum::Variant` in *type* position to a [`Ty::Variant`]. Every
/// `Ty::Error` case (base not an enum type, unknown variant, generic enum —
/// variant types of a generic enum have no annotation spelling yet) has a
/// matching diagnostic in [`crate::file_diagnostics`]'s `PathType` pass —
/// same mirror contract as [`lower_type_path`].
fn lower_variant_type_path(db: &dyn Db, file: SourceFile, enum_name: &str, variant: &str) -> Ty {
    let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(enum_name) else {
        return Ty::Error;
    };
    if !decl_generics(db, &loc).is_empty() {
        return Ty::Error;
    }
    let Some(variants) = enum_variants(db, loc.to_id(db)).as_ref() else {
        // A struct `type` item, or a broken declaration.
        return Ty::Error;
    };
    match variants.iter().position(|(name, _)| name == variant) {
        Some(index) => Ty::Variant(VariantTy {
            decl: loc,
            args: Vec::new(),
            index: index as u32,
            name: std::sync::Arc::from(variant),
        }),
        None => Ty::Error,
    }
}

/// Lower a turbofish mention in type position (`Pair::<usize, 8>`) —
/// purely syntactic, the annotation firewall: type args lower recursively
/// (`_` mints an inference variable where a table can fill it), const args
/// are restricted to literal-shaped values and in-scope const-param names.
/// A `const { ... }` block never lowers here (const eval must not run on
/// this path) — it becomes [`ConstArgValue::Error`], and the mirror pass
/// carries the clean diagnostic.
///
/// Recovery shape (load-bearing for the mirror contract): a whole-mention
/// problem (unknown name, non-generic target, arity mismatch) is
/// `Ty::Error`; a per-argument problem (kind mismatch, unrepresentable
/// const value) breaks only that ARGUMENT (`Ty::Error` / `Error` in its
/// slot) so the mention still names the right type family and downstream
/// checking doesn't cascade.
fn lower_apply(
    db: &dyn Db,
    file: SourceFile,
    name: &str,
    written: &[GenericArgRef],
    table: &mut InPlaceUnificationTable<TyVar>,
    scope: &ParamScope,
) -> Ty {
    // A binder param takes no generic arguments (`T::<usize>`).
    if scope.types.contains_key(name) || scope.consts.contains_key(name) {
        return Ty::Error;
    }
    let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(name) else {
        return Ty::Error;
    };
    let generics = decl_generics(db, &loc);
    if generics.is_empty() || written.len() != generics.len() {
        return Ty::Error;
    }
    let args = generics
        .iter()
        .zip(written)
        .map(|(param, arg)| match (&param.kind, arg) {
            // Region params on a TYPE declaration are reserved; the mirror
            // pass carries the story. The slot is still filled so the
            // remaining arguments keep their binder positions.
            (GenericParamKind::Region, _) => GenericArg::Region(Region::Erased),
            (_, GenericArgRef::Region(_)) => GenericArg::Ty(Ty::Error),
            (GenericParamKind::Type, GenericArgRef::Type(type_ref)) => {
                GenericArg::Ty(lower_type_ref_in(db, file, type_ref, table, scope))
            }
            (GenericParamKind::Type, GenericArgRef::Const(_)) => GenericArg::Ty(Ty::Error),
            (GenericParamKind::Const(_), GenericArgRef::Const(value)) => {
                GenericArg::Const(lower_const_arg_ref(value, scope))
            }
            // A bare `N`: a const position whose written argument
            // parsed as a type naming an in-scope const param.
            (GenericParamKind::Const(_), GenericArgRef::Type(TypeRef::Path(path)))
                if scope.consts.contains_key(path.as_str()) =>
            {
                GenericArg::Const(scope.const_param_value(path))
            }
            (GenericParamKind::Const(_), GenericArgRef::Type(_)) => {
                GenericArg::Const(ConstArgValue::Error)
            }
        })
        .collect();
    Ty::Named(NamedTy { decl: loc, args })
}

fn lower_const_arg_ref(value: &ConstArgRef, scope: &ParamScope) -> ConstArgValue {
    match value {
        ConstArgRef::Int(v) => ConstArgValue::Int(*v),
        ConstArgRef::Str(s) => ConstArgValue::Str(std::sync::Arc::from(s.as_str())),
        ConstArgRef::Bool(b) => ConstArgValue::Bool(*b),
        ConstArgRef::Char(c) => ConstArgValue::Char(*c),
        ConstArgRef::Name(name) => scope.const_param_value(name),
        // Blocks are outside the annotation domain (the firewall); the
        // mirror pass carries the diagnostic.
        ConstArgRef::Block | ConstArgRef::Error => ConstArgValue::Error,
    }
}

/// The rigid-param scope a generic binder puts type annotations under:
/// type-param names resolve to rigid [`Ty::Param`]s (TR06), and const-param
/// names are nameable in *const-argument* positions (`Buf::<N>`), where
/// they resolve to rigid [`ConstArgValue::Param`]s. Empty everywhere
/// outside a generic body.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParamScope {
    /// Region-param name (sigil included, `@a`) → its rigid
    /// [`Region::Param`]. Regions are neither values nor types, so they
    /// live in neither the expression namespace (`Resolution`) nor
    /// [`Self::types`]: only REGION-argument positions consult this map,
    /// and the mirror pass reports a region name used anywhere else.
    pub(crate) regions: FxHashMap<String, Region>,
    /// Type-param name → its rigid [`Ty::Param`].
    pub(crate) types: FxHashMap<String, Ty>,
    /// Const-param name → its rigid identity. Const params are *value*
    /// names, not types — in plain type position they resolve to nothing
    /// (the mirror pass reports it); only const-argument positions consult
    /// this map.
    pub(crate) consts: FxHashMap<String, ConstArgValue>,
}

impl ParamScope {
    /// The rigid value of the const param `name`, or `Error` when no such
    /// param is in scope (diagnosed by the mirror pass).
    fn const_param_value(&self, name: &str) -> ConstArgValue {
        self.consts
            .get(name)
            .cloned()
            .unwrap_or(ConstArgValue::Error)
    }

    /// The rigid region `name` denotes, or [`Region::Error`] when the
    /// enclosing binder declares no such region (the mirror pass reports
    /// it).
    fn region_value(&self, name: &str) -> Region {
        self.regions.get(name).cloned().unwrap_or(Region::Error)
    }
}

/// Lower a written region argument under a binder's [`ParamScope`].
///
/// `fresh` mints the existential region a `@_` stands for. Signature
/// lowering has no inference context and passes `None`, which turns a
/// wildcard into [`Region::Error`] — that is the no-elision rule biting
/// exactly where it should: a signature may not decline to name a region.
pub(crate) fn lower_region_ref(
    value: &RegionRef,
    scope: &ParamScope,
    fresh: &mut Option<&mut dyn FnMut() -> Region>,
) -> Region {
    match value {
        RegionRef::Named(name) => scope.region_value(name),
        RegionRef::Wildcard => match fresh {
            Some(mint) => mint(),
            None => Region::Error,
        },
        RegionRef::Join(parts) => Region::Join(
            parts
                .iter()
                .map(|part| lower_region_ref(part, scope, fresh))
                .collect(),
        ),
        RegionRef::Error => Region::Error,
    }
}

/// Lower a syntactic type annotation under a generic binder's
/// [`ParamScope`] (empty outside generic bodies — the binder shadows
/// file-level type items and builtins, so params are consulted first).
pub(crate) fn lower_type_ref_in(
    db: &dyn Db,
    file: SourceFile,
    value: &TypeRef,
    table: &mut InPlaceUnificationTable<TyVar>,
    scope: &ParamScope,
) -> Ty {
    match value {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Fn { params, ret } => {
            let params = params
                .iter()
                .map(|param_ty| lower_type_ref_in(db, file, param_ty, table, scope))
                .collect();

            let ret = ret
                .as_ref()
                .map(|ret_ty| lower_type_ref_in(db, file, ret_ty, table, scope))
                .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));

            Ty::fn_type(params, ret)
        }
        TypeRef::RawPtr { mutable, inner } => {
            Ty::raw_ptr(*mutable, lower_type_ref_in(db, file, inner, table, scope))
        }
        // `T.&::<@a>` — a safe borrow. The region is resolved against the
        // enclosing binder; a missing turbofish is `Region::Error` (no
        // elision at launch — the mirror pass names the omission), and a
        // wildcard needs an inference context, which this path does not
        // have (see `lower_type_ref_in_body`).
        TypeRef::Borrow {
            mutable,
            region,
            inner,
        } => Ty::borrow(
            *mutable,
            match region {
                Some(region) => lower_region_ref(region, scope, &mut None),
                None => Region::Error,
            },
            lower_type_ref_in(db, file, inner, table, scope),
        ),
        TypeRef::Array { elem, len } => Ty::array(
            lower_type_ref_in(db, file, elem, table, scope),
            lower_const_arg_ref(len, scope),
        ),
        TypeRef::Path(path) => match scope.types.get(path) {
            Some(param) => param.clone(),
            None => lower_type_path(db, file, path),
        },
        TypeRef::Apply { name, args } => lower_apply(db, file, name, args, table, scope),
        TypeRef::Variant { enum_name, variant } => {
            // A type param has no variants (`T::X` is silently `{error}` —
            // the diagnostics pass reports it); an unshadowed name resolves
            // as usual.
            if scope.types.contains_key(enum_name) {
                Ty::Error
            } else {
                lower_variant_type_path(db, file, enum_name, variant)
            }
        }
        TypeRef::Hole => Ty::Infer(table.new_key(TyVarValue::Unknown)),
        TypeRef::Record(fields) => Ty::record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), lower_type_ref_in(db, file, ty, table, scope)))
                .collect(),
        ),
        TypeRef::Error => Ty::Error,
    }
}

/// The declared type of `item`'s const param `index` (`usize` for
/// `const N: usize`), lowered eval-free — `{error}` when out of range or
/// not a const param. MIR consults this to type the receiver-supplied
/// const-argument values of a member call on a const-generic type.
pub fn const_param_declared_ty(db: &dyn Db, item: ItemId<'_>, index: u32) -> Ty {
    let declared = crate::item_data(db, item)
        .as_ref()
        .and_then(|data| data.generics.get(index as usize))
        .and_then(|param| match &param.kind {
            GenericParamKind::Const(type_ref) => Some(type_ref.clone()),
            GenericParamKind::Region | GenericParamKind::Type => None,
        });
    match declared {
        Some(type_ref) => lower_const_decl_ty(db, item.file(db), &type_ref),
        None => Ty::Error,
    }
}

/// Lower a const param's *declared* type outside any inference context —
/// scope-less (dependent `const N: T` is rejected, TR06) and hole-free (any
/// minted variable erases to `{error}`; the declaration has nothing to
/// fill it with). Used by the annotation mirror in
/// [`crate::file_diagnostics`] for literal/forwarded const-arg checks.
pub(crate) fn lower_const_decl_ty(db: &dyn Db, file: SourceFile, type_ref: &TypeRef) -> Ty {
    let mut table = InPlaceUnificationTable::new();
    let ty = lower_type_ref_in(db, file, type_ref, &mut table, &ParamScope::default());
    if ty.contains_infer() { Ty::Error } else { ty }
}

/// The self-type of a MEMBER item: the OWNING type declaration at its full
/// binders — each of the owner's params spelled as the member's own rigid
/// [`Ty::Param`]/[`ConstArgValue::Param`] (member schemes are keyed by the
/// member's [`ItemLoc`], exactly like any generic item's). `None` for
/// non-member ids.
pub fn member_self_ty(db: &dyn Db, item: ItemId<'_>) -> Option<Ty> {
    let owner = crate::member_owner(db, item)?;
    // A TRAIT-side impl member's `Self` is the implementing type its
    // element head names (a builtin scalar or a non-generic `type` item),
    // not the owning trait.
    if let Some((member_name, member_dis)) = item.member(db)
        && crate::item_data(db, owner)
            .as_ref()
            .is_some_and(|data| matches!(data.kind, crate::item_tree::ItemKind::Trait))
    {
        let members = crate::item_tree::type_members(db, owner);
        let data = members
            .iter()
            .find(|m| m.name == member_name && m.disambiguator == member_dis)?;
        let crate::item_tree::MemberHome::TraitImpl { head } = &data.home else {
            return Some(Ty::Error);
        };
        return Some(
            match crate::traits::resolve_self_head(db, item.file(db), head) {
                Some(key) => key.to_ty(),
                // Unresolvable implementer: the impl head carries the
                // diagnostic; the member's Self is broken.
                None => Ty::Error,
            },
        );
    }
    let owner_loc = crate::item_loc(db, owner);
    let member_loc = crate::item_loc(db, item);
    let generics = crate::item_data(db, owner)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let args = generics
        .iter()
        .enumerate()
        .map(|(index, param)| match param.kind {
            GenericParamKind::Type => GenericArg::Ty(Ty::Param(ParamTy {
                item: member_loc.clone(),
                index: index as u32,
                name: std::sync::Arc::from(param.name.as_str()),
            })),
            GenericParamKind::Region => GenericArg::Region(Region::Param {
                item: member_loc.clone(),
                index: index as u32,
                name: std::sync::Arc::from(param.name.as_str()),
            }),
            GenericParamKind::Const(_) => GenericArg::Const(ConstArgValue::Param {
                item: member_loc.clone(),
                index: index as u32,
                name: std::sync::Arc::from(param.name.as_str()),
            }),
        })
        .collect();
    Some(Ty::Named(NamedTy {
        decl: owner_loc,
        args,
    }))
}

/// Where a dot-callable member's `Self` sits in its LAST parameter —
/// dot-callability is structural (TR01: there is no `self` token), and this
/// is the whole of the structure it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfPosition {
    /// `fn(..., s: Self)` — the receiver IS the value.
    Value,
    /// `fn::<@b>(..., s: Self.&::<@b>)` / `Self.&mut::<@b>` — the receiver
    /// is a BORROW of the value.
    Borrow { mutable: bool },
}

/// The shape a dot-call's RECEIVER arrives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverShape {
    /// The receiver is the value itself.
    Owned,
    /// The receiver is a borrow of the value.
    Borrow { mutable: bool },
}

impl ReceiverShape {
    /// The shape of an already-resolved receiver type.
    pub fn of(ty: &Ty) -> Self {
        match ty {
            Ty::Borrow { mutable, .. } => ReceiverShape::Borrow { mutable: *mutable },
            _ => ReceiverShape::Owned,
        }
    }

    /// Whether the receiver arrived as a borrow.
    pub fn is_borrow(self) -> bool {
        matches!(self, ReceiverShape::Borrow { .. })
    }
}

/// Where `Self` sits in an already-lowered signature, given the type
/// standing for `Self` — the structural test over a `Ty` rather than over a
/// declaration, so a trait REQUIREMENT's freshly-lowered signature and a
/// member's stored one are judged by one function.
///
/// A raw pointer to `Self` is deliberately NOT a self position: `.&raw` is
/// not a decayed borrow, no raw borrow is ever inserted, and there is no
/// reborrow relation to check it against.
pub fn self_position_of(sig: &Ty, self_ty: &Ty) -> Option<SelfPosition> {
    let Ty::Fn(f) = sig else {
        return None;
    };
    match f.params.last()? {
        last if last == self_ty => Some(SelfPosition::Value),
        Ty::Borrow {
            mutable, referent, ..
        } if &**referent == self_ty => Some(SelfPosition::Borrow { mutable: *mutable }),
        _ => None,
    }
}

/// Where `item` — a member id — takes its receiver, or `None` when it has
/// no dot-callable shape at all.
pub fn member_self_position(db: &dyn Db, item: ItemId<'_>) -> Option<SelfPosition> {
    let self_ty = member_self_ty(db, item)?;
    self_position_of(&signature(db, item), &self_ty)
}

/// Whether a receiver of this shape can take a dot-call of a member with
/// this `Self` position. The table is deliberately ASYMMETRIC, and the
/// asymmetry is G14's bounded exception, both clauses:
///
/// - A BORROW receiver meeting a borrow `Self` is licensed by clause 1 —
///   the inserted borrow is of `x.*`, where `x` is already a borrow — and
///   it is the ordinary reborrow every other argument position gets, not a
///   new kind of event. Exclusive may degrade to shared; shared may never
///   sharpen to exclusive.
/// - An OWNED receiver meeting a borrow `Self` is refused by clause 2: the
///   borrow the compiler would have to insert is a borrow of the LOCAL
///   itself, which the exception forbids outright. The user writes
///   `m.&mut.f(...)` — which is a postfix chain whose receiver is then a
///   borrow, so it comes back through the licensed case.
/// - A BORROW receiver meeting a value `Self` would be auto-deref, sealed
///   absolutely. The user writes `m.*.f(...)`.
///
/// Nothing here can insert a borrow, and nothing downstream can either: the
/// only inserter is `Constraints::try_reborrow`, which requires a borrow on
/// BOTH sides before it emits anything.
pub fn receiver_takes(receiver: ReceiverShape, position: SelfPosition) -> bool {
    match (receiver, position) {
        (ReceiverShape::Owned, SelfPosition::Value) => true,
        (
            ReceiverShape::Borrow {
                mutable: receiver_mut,
            },
            SelfPosition::Borrow { mutable },
        ) => receiver_mut || !mutable,
        (ReceiverShape::Owned, SelfPosition::Borrow { .. })
        | (ReceiverShape::Borrow { .. }, SelfPosition::Value) => false,
    }
}

/// The [`ParamScope`] of `item`'s generic binder. For a MEMBER item the
/// scope additionally binds `Self` to the owning type at its full binders
/// — the one extra name member signatures and bodies resolve.
pub(crate) fn generic_param_scope(
    db: &dyn Db,
    item: ItemId<'_>,
    generics: &[crate::item_tree::GenericParamData],
) -> ParamScope {
    let loc = crate::item_loc(db, item);
    let mut scope = ParamScope::default();
    if let Some(self_ty) = member_self_ty(db, item) {
        scope.types.insert("Self".to_owned(), self_ty);
    }
    for (index, param) in generics.iter().enumerate() {
        if param.name.is_empty() {
            continue;
        }
        match param.kind {
            GenericParamKind::Type => {
                scope.types.insert(
                    param.name.clone(),
                    Ty::Param(ParamTy {
                        item: loc.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    }),
                );
            }
            GenericParamKind::Region => {
                scope.regions.insert(
                    param.name.clone(),
                    Region::Param {
                        item: loc.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    },
                );
            }
            GenericParamKind::Const(_) => {
                scope.consts.insert(
                    param.name.clone(),
                    ConstArgValue::Param {
                        item: loc.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    },
                );
            }
        }
    }
    scope
}

/// The underlying record type a `type` item declares — for a GENERIC
/// declaration, the generic body: the binder's params appear as rigid
/// [`Ty::Param`]s / [`ConstArgValue::Param`]s. `None` when the declaration
/// is broken (RHS not a `struct` literal — diagnosed at the declaration).
/// Field access on a [`Ty::Named`] and construction calls project through
/// [`type_underlying_for`], which substitutes a mention's args into this
/// stored body; MIR uses its sorted field order for projections (the order
/// is arg-independent — names sort the same under any substitution).
///
/// A declaration's `TypeRef`s are mostly `Path`/`Record`/`Error` (see
/// [`crate::item_tree::type_decl`]), but a generic mention's args
/// (`next: Wrap::<_>`) can contain holes that would mint inference
/// variables; a declaration has no inference context to fill them, so any
/// variable-typed leftover is erased to `Ty::Error` here, with the matching
/// diagnostic in [`crate::file_diagnostics`]. Recursive declarations
/// (`type List = struct::<T> { next: List::<T> }`) terminate because
/// lowering a path stops at [`Ty::Named`] — nothing expands, and
/// substitution ([`substitute_args`]) stops at `Named` args the same way.
#[salsa::tracked]
pub fn type_underlying<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<Ty> {
    let crate::item_tree::TypeDeclData::Struct { fields } = type_decl(db, item).as_ref()? else {
        return None;
    };
    let file = item.file(db);
    let generics = crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let scope = generic_param_scope(db, item, &generics);
    let mut table = InPlaceUnificationTable::new();
    Some(Ty::record(
        fields
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    erase_infer(&lower_type_ref_in(db, file, ty, &mut table, &scope)),
                )
            })
            .collect(),
    ))
}

/// Whether a `match` on a value of this (shallow-resolved) type dispatches
/// on it — an enum declaration, one of its variants, or `char`, whose
/// literal patterns dispatch by equality rather than by a tag. Everything
/// else can only be matched by a catch-all.
///
/// The one predicate every pass asks: inference uses it to decide whether
/// a BORROWED scrutinee lifts the projection lens (M13 — `match` projects
/// through a borrow exactly when a `match` on the referent would dispatch
/// on it), mir asks the same question of the same referent when it lowers
/// the match, and ide's scrutinee-side completions ask it behind the same
/// peeled borrow so they offer arms for exactly the scrutinees the checker
/// accepts. The rule moves by design: granting a new pattern kind widens
/// this, and every borrowed scrutinee of that type follows.
pub fn dispatches_on(db: &dyn Db, ty: &Ty) -> bool {
    match ty {
        Ty::Named(named) => enum_variants(db, named.decl.to_id(db)).is_some(),
        Ty::Variant(_) => true,
        Ty::Char => true,
        _ => false,
    }
}

/// The variants an enum `type` item declares — `(name, payload types)` in
/// source order (a variant's index is its identity) — or `None` when the
/// item declares a struct shape or is broken. The enum-side counterpart of
/// [`type_underlying`], and the same incrementality firewall; for a generic
/// enum the payloads are the generic body (rigid params), substituted per
/// mention by [`variant_payloads_for`].
///
/// Payload `TypeRef`s come from real type syntax, so they *can* contain
/// holes (`_`) or `fn` types without a return — positions that would mint
/// inference variables. A declaration has no inference context to fill
/// them, so any variable-typed leftover is erased to `Ty::Error` here;
/// [`crate::file_diagnostics`] rejects those payloads with a diagnostic.
#[salsa::tracked(returns(ref))]
pub fn enum_variants<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<Vec<(String, Vec<Ty>)>> {
    let crate::item_tree::TypeDeclData::Enum { variants } = type_decl(db, item).as_ref()? else {
        return None;
    };
    let file = item.file(db);
    let generics = crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let scope = generic_param_scope(db, item, &generics);
    let mut table = InPlaceUnificationTable::new();
    Some(
        variants
            .iter()
            .map(|(name, payload)| {
                (
                    name.clone(),
                    payload
                        .iter()
                        .map(|ty| erase_infer(&lower_type_ref_in(db, file, ty, &mut table, &scope)))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// Erase dangling inference variables to `Ty::Error` — for lowering done
/// outside any inference context (declarations).
fn erase_infer(ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(_) | Ty::UnresolvedNumber => Ty::Error,
        Ty::Fn(f) => Ty::fn_type(
            f.params.iter().map(erase_infer).collect(),
            erase_infer(&f.ret),
        ),
        Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(*mutable, erase_infer(pointee)),
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(*mutable, region.clone(), erase_infer(referent)),
        Ty::Array { elem, len } => Ty::array(erase_infer(elem), len.clone()),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), erase_infer(ty)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: named.args.iter().map(erase_infer_arg).collect(),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: variant.args.iter().map(erase_infer_arg).collect(),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

fn erase_infer_arg(arg: &GenericArg) -> GenericArg {
    match arg {
        GenericArg::Ty(ty) => GenericArg::Ty(erase_infer(ty)),
        GenericArg::Region(region) => GenericArg::Region(region.clone()),
        GenericArg::Const(value) => GenericArg::Const(value.clone()),
    }
}

/// Substitute `decl`'s rigid binder params by `args` throughout `ty` — the
/// on-demand projection that turns a stored generic body ([`type_underlying`],
/// [`enum_variants`]) into a mention's concrete shape. A pure walk keyed by
/// `(item, index)`, exactly like fn-scheme instantiation; `Named`/`Variant`
/// args are substituted but their declared shapes never expand, so
/// self-referential generic types terminate. Out-of-range or kind-crossed
/// substitutions (broken mentions, diagnosed upstream) land on
/// `Ty::Error`/`ConstArgValue::Error`.
pub fn substitute_args(ty: &Ty, decl: &ItemLoc, args: &[GenericArg]) -> Ty {
    if args.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::Param(param) if param.item == *decl => match args.get(param.index as usize) {
            Some(GenericArg::Ty(ty)) => ty.clone(),
            _ => Ty::Error,
        },
        Ty::Fn(f) => Ty::fn_type(
            f.params
                .iter()
                .map(|param| substitute_args(param, decl, args))
                .collect(),
            substitute_args(&f.ret, decl, args),
        ),
        Ty::RawPtr { mutable, pointee } => {
            Ty::raw_ptr(*mutable, substitute_args(pointee, decl, args))
        }
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(
            *mutable,
            substitute_region(region, decl, args),
            substitute_args(referent, decl, args),
        ),
        Ty::Array { elem, len } => {
            let len = match len {
                ConstArgValue::Param { item, index, .. } if item == decl => {
                    match args.get(*index as usize) {
                        Some(GenericArg::Const(value)) => value.clone(),
                        _ => ConstArgValue::Error,
                    }
                }
                other => other.clone(),
            };
            Ty::array(substitute_args(elem, decl, args), len)
        }
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_args(ty, decl, args)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: named
                .args
                .iter()
                .map(|arg| substitute_generic_arg(arg, decl, args))
                .collect(),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: variant
                .args
                .iter()
                .map(|arg| substitute_generic_arg(arg, decl, args))
                .collect(),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

fn substitute_generic_arg(arg: &GenericArg, decl: &ItemLoc, args: &[GenericArg]) -> GenericArg {
    match arg {
        GenericArg::Ty(ty) => GenericArg::Ty(substitute_args(ty, decl, args)),
        // A region ARGUMENT substitutes exactly as a borrow's region does
        // (`substitute_region` a few arms up) — the same walk, one
        // constructor along. Unreachable until a type declaration may take
        // a region; correct the day it can.
        GenericArg::Region(region) => GenericArg::Region(substitute_region(region, decl, args)),
        GenericArg::Const(ConstArgValue::Param { item, index, .. }) if item == decl => {
            match args.get(*index as usize) {
                Some(GenericArg::Const(value)) => GenericArg::Const(value.clone()),
                _ => GenericArg::Const(ConstArgValue::Error),
            }
        }
        GenericArg::Const(value) => GenericArg::Const(value.clone()),
    }
}

/// The underlying record of a *mention* — [`type_underlying`] with the
/// mention's args substituted for the declaration's rigid params. Memoized
/// per declaration (the salsa query stores the generic body once);
/// substitution is an on-demand walk, cheap and unmemoized.
pub fn type_underlying_for(db: &dyn Db, named: &NamedTy) -> Option<Ty> {
    let ty = type_underlying(db, named.decl.to_id(db))?;
    Some(substitute_args(&ty, &named.decl, &named.args))
}

/// The payload types of one variant *mention* — [`enum_variants`]'s stored
/// generic payloads with the variant's enum args substituted. Same
/// memoization split as [`type_underlying_for`].
pub fn variant_payloads_for(db: &dyn Db, variant: &VariantTy) -> Option<Vec<Ty>> {
    let variants = enum_variants(db, variant.decl.to_id(db)).as_ref()?;
    let (_, payload) = variants.get(variant.index as usize)?;
    Some(
        payload
            .iter()
            .map(|ty| substitute_args(ty, &variant.decl, &variant.args))
            .collect(),
    )
}

/// The type other items see for `item`. A fully-typed contract is the
/// whole answer (a hard firewall edge: body edits never reach dependents) —
/// written as an annotation, or synthesized from a self-sufficient
/// fn-literal body (see [`crate::item_tree`]). Anything less — no contract,
/// or one with holes — comes from the item's binding group: its own body,
/// inferred together with everything grouped with it (mutual recursion and
/// callers alike — see [`crate::groups`]), where holes join in as
/// unconstrained variables the bodies fill in. The firewall is then salsa
/// early-cutoff: dependents re-run only when the *inferred* signature value
/// changes.
///
/// **Generic items — the scheme.** For a generic item this returns the
/// SCHEME: the `Ty::Fn` lowered from the literal's mandatory annotations
/// (TR06), with the binder's type params appearing as rigid [`Ty::Param`]s.
/// The scheme representation is deliberately just this `Ty` plus
/// `item_data(item).generics` (arity and kinds): every `Ty::Param` carries
/// its declaring item and binder index, so instantiation is a pure
/// walk-and-replace keyed by `(item, index)` with no separate binder
/// structure — see `infer`'s `instantiate_scheme`. `Ty::Error` when the
/// fully-annotated rule is violated (the definition site carries the
/// diagnostic). Generic items never join binding groups, so this never
/// consults [`crate::groups`] for them.
#[salsa::tracked]
pub fn signature<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Ty {
    // A MEMBER's signature is always annotation-derived (dot-call
    // resolution must read heads without inference) and always lowers
    // under its param scope — even with an empty binder, `Self` is in
    // scope. `Ty::Error` when the fully-annotated member rule is violated
    // (the definition site carries the diagnostic).
    if item.member(db).is_some() {
        let Some(data) = crate::item_data(db, item).as_ref() else {
            return Ty::Error;
        };
        let Some(type_ref) = data.type_ref.as_ref() else {
            return Ty::Error;
        };
        let scope = generic_param_scope(db, item, &data.generics);
        let mut table = InPlaceUnificationTable::new();
        return lower_type_ref_in(db, item.file(db), type_ref, &mut table, &scope);
    }
    if let Some(data) = crate::item_data(db, item).as_ref()
        && !data.generics.is_empty()
    {
        let Some(type_ref) = data.type_ref.as_ref() else {
            // The fully-annotated rule is violated; the definition site
            // carries the diagnostic (see `file_diagnostics`).
            return Ty::Error;
        };
        let type_params = generic_param_scope(db, item, &data.generics);
        // The scheme's `TypeRef` is fully typed by construction (see
        // `item_tree::type_ref_from_fn_literal`), so no inference variables
        // are minted and a throwaway table is sound — the same reasoning as
        // [`type_underlying`].
        let mut table = InPlaceUnificationTable::new();
        return lower_type_ref_in(db, item.file(db), type_ref, &mut table, &type_params);
    }
    if let Some(type_ref) = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
        && type_ref.is_fully_typed()
    {
        // Fully typed: no hole ever creates a variable, so the
        // lowering table stays empty and is thrown away.
        let mut table = InPlaceUnificationTable::new();
        return lower_type_ref_in(
            db,
            item.file(db),
            type_ref,
            &mut table,
            &ParamScope::default(),
        );
    }
    let Some(index) = crate::item_index(db, item) else {
        return Ty::Error;
    };
    let file = item.file(db);
    let groups = crate::groups::inference_groups(db, file);
    let Some(Some(group)) = groups.group_of.get(index).copied() else {
        return Ty::Error;
    };
    let members = &groups.groups[group as usize];
    let Some(position) = members.iter().position(|&member| member == index) else {
        return Ty::Error;
    };
    crate::groups::infer_group(db, crate::groups::GroupId::new(db, file, group))
        .signatures
        .get(position)
        .cloned()
        .unwrap_or(Ty::Error)
}

/// Whether uses of `item` should say "add a type annotation": inference
/// couldn't determine its type from the definition (group inference erases
/// undetermined leftovers to `{error}`). A hole-bearing annotation counts
/// as undetermined when the body couldn't fill the holes — a partial
/// contract that stays partial publishes no silent `{error}`.
///
/// Language decision: exported symbols will *always* require a written
/// contract, even with interprocedural inference — once a visibility notion
/// exists, exported items go back to requiring annotations; today every
/// item counts as private.
pub fn signature_needs_annotation<'db>(db: &'db dyn Db, item: ItemId<'db>) -> bool {
    // Members follow the generic-item rule: the definition site carries
    // the unconditional fully-annotated diagnostic; repeating it at uses
    // would be noise.
    if item.member(db).is_some() {
        return false;
    }
    if let Some(data) = crate::item_data(db, item).as_ref() {
        // A generic item is never "needs annotation": its scheme either is
        // complete (fully-annotated rule) or the *definition* carries the
        // rule's unconditional diagnostic — repeating it at every mention
        // would be noise.
        if !data.generics.is_empty() {
            return false;
        }
        if let Some(type_ref) = data.type_ref.as_ref()
            && type_ref.is_fully_typed()
        {
            return false;
        }
    }
    if crate::body::body(db, item).root.is_none() {
        // No value at all: the parse errors cover it.
        return false;
    }
    signature(db, item).contains_error()
}

fn erase_regions_arg(arg: &GenericArg) -> GenericArg {
    match arg {
        GenericArg::Ty(ty) => GenericArg::Ty(ty.erase_regions()),
        GenericArg::Region(_) => GenericArg::Region(Region::Erased),
        GenericArg::Const(value) => GenericArg::Const(value.clone()),
    }
}

/// Substitute a declaration's rigid region params by a mention's args — the
/// region half of [`substitute_args`]. Only reachable once region params on
/// type declarations are un-reserved; written now so the walk is total.
fn substitute_region(region: &Region, decl: &ItemLoc, args: &[GenericArg]) -> Region {
    match region {
        Region::Param { item, index, .. } if item == decl => match args.get(*index as usize) {
            Some(GenericArg::Region(region)) => region.clone(),
            _ => Region::Error,
        },
        Region::Join(parts) => Region::Join(
            parts
                .iter()
                .map(|part| substitute_region(part, decl, args))
                .collect(),
        ),
        other => other.clone(),
    }
}
