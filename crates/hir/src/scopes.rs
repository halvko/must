//! Lexical scopes and name resolution.
//!
//! [`expr_scopes`] depends only on the item's own [`Body`]; [`resolutions`]
//! additionally reads [`file_scope`] (range-free names of all items), so an
//! edit inside another item's body never reaches it, and adding/removing
//! items only re-resolves names — it never re-runs body lowering.

use base_db::{Db, SourceFile};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;

use crate::body::{BindingId, Body, ExprData, ExprId, Stmt, body};
use crate::item_tree::{GenericParamData, GenericParamKind, ItemKind, TypeRef, item_tree};
use crate::{ItemId, ItemLoc};

pub type ScopeId = Idx<ScopeData>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExprScopes {
    scopes: Arena<ScopeData>,
    scope_of: ArenaMap<ExprId, ScopeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeData {
    parent: Option<ScopeId>,
    entries: Vec<(String, BindingId)>,
}

impl ExprScopes {
    pub fn scope_of(&self, expr: ExprId) -> Option<ScopeId> {
        self.scope_of.get(expr).copied()
    }

    pub fn resolve_in_scope(&self, mut scope: ScopeId, name: &str) -> Option<BindingId> {
        loop {
            let data = &self.scopes[scope];
            // Latest entry wins, so shadowing within one scope works too.
            if let Some((_, binding)) = data.entries.iter().rev().find(|(n, _)| n == name) {
                return Some(*binding);
            }
            scope = data.parent?;
        }
    }

    /// All bindings visible from `scope`, shadowing respected.
    pub fn visible_bindings(&self, scope: ScopeId) -> Vec<(String, BindingId)> {
        self.visible_bindings_with_depth(scope)
            .into_iter()
            .map(|(name, binding, _)| (name, binding))
            .collect()
    }

    /// [`Self::visible_bindings`] plus each binding's **definition-scope
    /// distance**: how many scope hops separate `scope` from the scope that
    /// declares it (`0` — declared in `scope` itself). Completions rank the
    /// `match` scrutinee slot by exactly this number, nearest first.
    ///
    /// The count is the *real* chain distance, not a hand-made tier list,
    /// because [`compute_expr_scopes`] already allocates one scope per
    /// binding construct: a `let` opens a child scope of the statement
    /// before it, a fn literal's parameters open one under whatever encloses
    /// the literal, and a match arm's bindings open one under the match.
    /// Nesting is therefore strictly monotone — an inner `let` always beats
    /// an outer one, every `let` in a body always beats that body's
    /// parameters, and a closure's own bindings will always beat its
    /// captures' the day closures land, with no rule to add.
    ///
    /// Shadowing: the nearest declaration of a name wins, and the distance
    /// reported is *its* distance (the shadowed outer one is not offered at
    /// all, so its greater distance never competes).
    pub fn visible_bindings_with_depth(&self, scope: ScopeId) -> Vec<(String, BindingId, u32)> {
        let mut seen = FxHashMap::default();
        let mut current = Some(scope);
        let mut depth = 0;
        while let Some(scope) = current {
            let data = &self.scopes[scope];
            for (name, binding) in data.entries.iter().rev() {
                seen.entry(name.clone()).or_insert((*binding, depth));
            }
            current = data.parent;
            depth += 1;
        }
        seen.into_iter()
            .map(|(name, (binding, depth))| (name, binding, depth))
            .collect()
    }
}

#[salsa::tracked(returns(ref))]
pub fn expr_scopes<'db>(db: &'db dyn Db, item: ItemId<'db>) -> ExprScopes {
    let body = body(db, item);
    let mut scopes = ExprScopes::default();
    let root_scope = scopes.scopes.alloc(ScopeData::default());
    if let Some(root) = body.root {
        compute_expr_scopes(body, &mut scopes, root, root_scope);
    }
    scopes
}

fn compute_expr_scopes(body: &Body, scopes: &mut ExprScopes, expr: ExprId, scope: ScopeId) {
    scopes.scope_of.insert(expr, scope);
    match &body.exprs[expr] {
        ExprData::Block { stmts, tail } => {
            let mut scope = scope;
            for stmt in stmts {
                match stmt {
                    Stmt::Let { pat, init, .. } => {
                        // The initializer sees the scope *before* the
                        // pattern's bindings; shadowing is just a fresh
                        // child scope per `let`, entered with every binding
                        // the pattern introduces (one for a bare name, any
                        // number for a destructuring pattern).
                        compute_expr_scopes(body, scopes, *init, scope);
                        scope = scopes.scopes.alloc(ScopeData {
                            parent: Some(scope),
                            entries: body.pat_bindings(*pat),
                        });
                    }
                    Stmt::Assign { target, value } => {
                        compute_expr_scopes(body, scopes, *target, scope);
                        compute_expr_scopes(body, scopes, *value, scope);
                    }
                    Stmt::Expr(e) => compute_expr_scopes(body, scopes, *e, scope),
                }
            }
            if let Some(tail) = tail {
                compute_expr_scopes(body, scopes, *tail, scope);
            }
        }
        // Transparent: no scope of its own, just the parent's.
        ExprData::ConstBlock { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::FnLiteral {
            params, body: b, ..
        } => {
            let scope = scopes.scopes.alloc(ScopeData {
                parent: Some(scope),
                entries: params
                    .iter()
                    .flat_map(|p| body.pat_bindings(p.pat))
                    .collect(),
            });
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::Call { callee, args, .. } => {
            compute_expr_scopes(body, scopes, *callee, scope);
            for &arg in args {
                compute_expr_scopes(body, scopes, arg, scope);
            }
        }
        ExprData::Bin { lhs, rhs, .. } => {
            compute_expr_scopes(body, scopes, *lhs, scope);
            compute_expr_scopes(body, scopes, *rhs, scope);
        }
        ExprData::Neg { operand } => {
            compute_expr_scopes(body, scopes, *operand, scope);
        }
        ExprData::If {
            condition,
            then_branch,
            else_branch,
        } => {
            compute_expr_scopes(body, scopes, *condition, scope);
            compute_expr_scopes(body, scopes, *then_branch, scope);
            if let Some(else_branch) = else_branch {
                compute_expr_scopes(body, scopes, *else_branch, scope);
            }
        }
        ExprData::RecordLit { fields } => {
            for field in fields {
                compute_expr_scopes(body, scopes, field.value, scope);
            }
        }
        // The field name is a projection, not a scoped reference; only the
        // receiver is an expression.
        ExprData::Field { receiver, .. } => {
            compute_expr_scopes(body, scopes, *receiver, scope);
        }
        ExprData::ArrayLit { elements } => {
            for &element in elements {
                compute_expr_scopes(body, scopes, element, scope);
            }
        }
        ExprData::ArrayRepeat { element, count } => {
            compute_expr_scopes(body, scopes, *element, scope);
            compute_expr_scopes(body, scopes, *count, scope);
        }
        ExprData::Index { base, index } => {
            compute_expr_scopes(body, scopes, *base, scope);
            compute_expr_scopes(body, scopes, *index, scope);
        }
        ExprData::AddrOf { place, .. } | ExprData::Borrow { place, .. } => {
            compute_expr_scopes(body, scopes, *place, scope);
        }
        ExprData::Deref { receiver } => {
            compute_expr_scopes(body, scopes, *receiver, scope);
        }
        // Transparent, like a `const` block: a pure checker region.
        ExprData::Unsafe { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        // The variant name is resolved against the enum during inference,
        // not lexically; only the base is a scoped reference — plus any
        // turbofish const-arg values, which are ordinary scoped
        // expressions (same as a `GenericApp`'s). A SECOND segment's own
        // arguments are semantically reserved but lexically ordinary:
        // their names resolve here like anyone else's, so the reservation
        // is the only thing standing between them and running.
        ExprData::VariantPath {
            base,
            args,
            member_args,
            ..
        } => {
            compute_expr_scopes(body, scopes, *base, scope);
            for arg in args.iter().flatten().chain(member_args.iter().flatten()) {
                if let crate::body::GenericArgData::Const(value) = arg {
                    compute_expr_scopes(body, scopes, *value, scope);
                }
            }
        }
        // The base is a scoped reference like a variant path's; type args
        // are not expressions, const-arg *values* are ordinary scoped
        // expressions.
        ExprData::GenericApp { base, args } => {
            compute_expr_scopes(body, scopes, *base, scope);
            for arg in args {
                if let crate::body::GenericArgData::Const(value) = arg {
                    compute_expr_scopes(body, scopes, *value, scope);
                }
            }
        }
        ExprData::Match { scrutinee, arms } => {
            compute_expr_scopes(body, scopes, *scrutinee, scope);
            for arm in arms {
                // Each arm's body sees its own pattern's bindings — a fresh
                // child scope per arm, like a fn literal's params. A bare
                // binding always binds the whole scrutinee (never
                // reinterpreted as a variant), so its entry is just an
                // ordinary local.
                let arm_scope = scopes.scopes.alloc(ScopeData {
                    parent: Some(scope),
                    entries: body.pat_bindings(arm.pat),
                });
                compute_expr_scopes(body, scopes, arm.body, arm_scope);
            }
        }
        // A loop introduces no bindings of its own; its body is a block,
        // which scopes itself.
        ExprData::Loop { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::Break { value } | ExprData::Return { value } => {
            if let Some(value) = value {
                compute_expr_scopes(body, scopes, *value, scope);
            }
        }
        // An elided variant names nothing scopes can resolve: its one
        // segment is the VARIANT, resolved type-directed during
        // inference, exactly like a variant path's second segment.
        // A host import declares no names of its own either: the parameter
        // names in `unsafe fn(buf: ..., len: ...)` are a signature's
        // spelling, and there is no body for them to be visible in.
        ExprData::Missing
        | ExprData::Literal(_)
        | ExprData::NameRef(_)
        | ExprData::ExternImport
        | ExprData::ElidedVariant { .. }
        | ExprData::Continue => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolution {
    Local(BindingId),
    /// A `const` parameter of the enclosing item's generic binder
    /// (`fn::<const N: usize>` — `N` reads as a value of its declared type
    /// inside the body). Deliberately NOT a [`Resolution::Local`]: const
    /// params have no [`BindingId`] (they are binder facts from the item
    /// tree, not body bindings), so none of the locals machinery — value
    /// slots, `mut`, scopes arena — applies to them; carrying the binder
    /// index instead keeps every consumer honest about that. Locals shadow
    /// const params; const params shadow items and builtins (the binder
    /// sits lexically between the body and the file).
    ConstParam(u32),
    Item(ItemLoc),
    /// A `type` item. Kept apart from [`Resolution::Item`] so every consumer
    /// is forced to decide what a *type* means for it: a value use is an
    /// error, a call is a construction, a type position is a name hit.
    TypeItem(ItemLoc),
    /// A `trait` item. Its own variant for the same reason as
    /// [`Resolution::TypeItem`]: a trait is neither a value nor a type —
    /// the legal positions are bounds, impl heads and the base of a
    /// qualified member call (`Display::fmt(...)`).
    TraitItem(ItemLoc),
    /// The name is defined by more than one item. Resolves to the first
    /// definition so navigation has a target, but no use can be given a
    /// meaning: inference types these as `{error}`, and the extra
    /// definitions carry the diagnostic.
    Ambiguous(ItemLoc),
    Builtin(Builtin),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Print,
    Panic,
    /// `alloc_array::<T>(n)` — one fresh heap allocation of `n`
    /// uninitialized elements. SAFE (allocating cannot UB); returns the
    /// result-shaped [`ALLOC_RESULT_NAME`] enum (`Ok(T.&raw mut)` /
    /// `Err`) — the interpreter never produces `Err` (it cannot
    /// meaningfully OOM), but the signature is stable for the codegen era.
    /// Refused in const contexts (heap values wait for an interning
    /// design).
    AllocArray,
    /// `dealloc_array::<T>(p, n)` — exact-match free. UNSAFE. `p` must be
    /// the head of a live heap allocation and `n` its alloc-time count;
    /// anything else is detected UB. Refused in const contexts.
    DeallocArray,
    /// `add(p, i)` — pointer to element `i` past `p`, within the same
    /// allocation. UNSAFE: it carries a real precondition — advancing a
    /// pointer that does not address an array element with `i > 0` is
    /// detected UB at the call. Flavor-preserving (`.&raw mut` in →
    /// `.&raw mut` out) — a checker special case, not expressible as one
    /// `fn` type. Takes a `usize`; [`Builtin::Offset`] is the signed
    /// sibling, mirroring Rust's `add`/`offset` split.
    Add,
    /// `offset(p, i)` — the signed sibling of [`Builtin::Add`]: element
    /// arithmetic in both directions, `i: isize`. UNSAFE like `add`, with
    /// one extra precondition: a result index below the allocation's start
    /// (index < 0) is detected UB at the call (the abstract machine cannot
    /// represent it — the same class as `add`'s non-array-element rule).
    /// Everything else mirrors `add`, flavor preservation included.
    Offset,
    /// `copy(src, dst, n)` — element-count bulk copy, memmove semantics
    /// (overlap is DEFINED). UNSAFE (writes through a raw pointer).
    /// Copying an uninitialized element propagates the marker silently —
    /// only reading one *as a value* traps.
    Copy,
    /// `dangling::<T>()` — a `T.&raw mut` that was never valid (one
    /// reserved never-live allocation per machine). SAFE; any deref is
    /// detected UB.
    Dangling,
    /// `read_line()` — `print`'s input twin, a layer-1 platform hook
    /// (P01, P04). SAFE, monomorphic, nullary: returns the compiler-provided
    /// [`READ_LINE_RESULT_NAME`] enum (`Line(str)` / `End`), exactly the
    /// way `alloc_array` returns [`ALLOC_RESULT_NAME`]. Refused in const
    /// contexts like `print` (const evaluation cannot have side effects).
    ReadLine,
    /// `s.next_char(i)` — index-threading codepoint access on `str`. The
    /// one builtin reached as a MEMBER rather than a name: a free
    /// `next_char(s, i)` would burn a top-level name for an operation that
    /// only ever applies to one type, and the dot is where the reader
    /// already looks for "what can this value do".
    ///
    /// SAFE and PURE: decoding a `str` observes nothing outside its own
    /// arguments, so a `const` context takes it, like the pointer builtins
    /// and unlike `print`/`read_line`, which have effects to refuse.
    ///
    /// Not in [`Builtin::by_name`] on purpose: the name is reachable only
    /// through the dot on a `str` receiver, so nothing is taken out of the
    /// value namespace and a user's own `next_char` is an ordinary,
    /// unshadowed item. Inference re-opens it as a member candidate only
    /// after user trait impls have had their turn, so a `next_char` written
    /// in an `impl ... for str` still wins — the `print` shadowing rule,
    /// applied to a member.
    NextChar,
    /// `str_from_utf8(p, len)` — the CHECKED bless: `len` bytes at `p`
    /// validated as UTF-8, answering the compiler-provided
    /// [`UTF8_RESULT_NAME`] enum (`Ok(str)` / `Err`).
    ///
    /// UNSAFE, and the marker is about the POINTER, not the text: that `p`
    /// addresses `len` readable bytes is the caller's claim and nothing
    /// checks it. "Checked" names the other half — whether those bytes
    /// spell a string is answered, not assumed. Flavor-polymorphic in `p`
    /// (either raw flavor reads), like `copy`'s source.
    ///
    /// PURE, and therefore const-legal: reading bytes and deciding whether
    /// they are UTF-8 observes nothing outside its own arguments.
    StrFromUtf8,
    /// `str_from_utf8_unchecked(p, len)` — the CLAIMED bless: `len` bytes
    /// at `p`, taken as a `str` with no validation.
    ///
    /// UNSAFE twice over — the pointer claim of [`Builtin::StrFromUtf8`],
    /// plus the text claim it drops. Handing it bytes that are not UTF-8
    /// is undefined behavior, and the interpreter DETECTS it: a `str` whose
    /// contents are not a string would be a value the type system's own
    /// invariant says cannot exist, so producing one silently is exactly
    /// the class of bug this machine exists to catch.
    StrFromUtf8Unchecked,
    /// `s.len()` — the BYTE length of a `str`. A member, like
    /// [`Builtin::NextChar`], and for the same reason: it is a question
    /// about a value, so the dot is where a reader already looks for it.
    ///
    /// SAFE and PURE, so const-legal. Bytes and not characters, because
    /// bytes are what every other `str` operation counts — `next_char`
    /// threads a byte index, both blesses take a byte length, and a
    /// character count that agreed with none of them would be a trap
    /// wearing the shorter name.
    StrLen,
    /// `str_bytes(s, dst)` — the bless read backwards: `s.len()` bytes of
    /// `s`, written to `dst`.
    ///
    /// UNSAFE, and the marker is about the DESTINATION, exactly as
    /// [`Builtin::StrFromUtf8`]'s is about the source: that `dst` addresses
    /// `s.len()` WRITABLE bytes is the caller's claim and nothing checks
    /// it. Unlike the blesses this one is not flavor-polymorphic — a
    /// destination is written, so it is `u8.&raw mut` and nothing else,
    /// which also makes it the one `str` byte builtin with an ordinary
    /// first-class `fn` type.
    ///
    /// PURE in the sense const contexts care about (it writes only through
    /// a pointer whose target already exists), so it is const-legal on
    /// `copy`'s reasoning rather than `next_char`'s.
    StrBytes,
}

/// What const evaluation does with a call of a builtin, and when it
/// refuses, WHY ([`Builtin::const_legality`]).
///
/// The reason is part of the property rather than something a caller
/// re-derives from the variant: the two refusals mean different things and
/// have different futures — a host effect is refused forever (const
/// evaluation cannot have one), while the heap fence is a fence, which the
/// interning design (C06) is expected to lift. A caller that had to guess
/// would have to carry its own list of which builtins are heap-shaped,
/// which is exactly the second table this property exists to abolish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstLegality {
    /// No side effect and no host dependence: callable during const
    /// evaluation.
    Legal,
    /// Refused: an effect the compiler cannot perform on the program's
    /// behalf — `print` and `read_line` reach the host.
    HostEffect,
    /// Refused by the eager const fence (C04): const-built heap values
    /// wait for the interning design (C06), and refusing at the call keeps
    /// the later relaxation a grant, not a retraction.
    HeapFence,
}

impl Builtin {
    pub fn by_name(name: &str) -> Option<Builtin> {
        match name {
            "print" => Some(Builtin::Print),
            "panic" => Some(Builtin::Panic),
            "alloc_array" => Some(Builtin::AllocArray),
            "dealloc_array" => Some(Builtin::DeallocArray),
            "add" => Some(Builtin::Add),
            "offset" => Some(Builtin::Offset),
            "copy" => Some(Builtin::Copy),
            "dangling" => Some(Builtin::Dangling),
            "read_line" => Some(Builtin::ReadLine),
            "str_from_utf8" => Some(Builtin::StrFromUtf8),
            "str_from_utf8_unchecked" => Some(Builtin::StrFromUtf8Unchecked),
            "str_bytes" => Some(Builtin::StrBytes),
            // `len` is deliberately absent for `next_char`'s reason: it is
            // a MEMBER of `str`, so the name stays out of the value
            // namespace entirely.
            // `next_char` is deliberately absent: it is a MEMBER of `str`,
            // not a top-level name (see `Builtin::NextChar`).
            _ => None,
        }
    }

    /// Every builtin MEMBER a receiver of type `recv` carries — the
    /// member-side twin of [`Builtin::by_name`], and the single source of
    /// truth both resolution ([`Builtin::member_by_name`]) and the editor's
    /// dot-completions read, so neither can list a member the other
    /// doesn't.
    pub fn members_of(recv: &crate::Ty) -> &'static [Builtin] {
        match recv {
            crate::Ty::Str => &[Builtin::NextChar, Builtin::StrLen],
            _ => &[],
        }
    }

    /// The builtin member `name` names on a receiver of type `recv`, if
    /// any.
    pub fn member_by_name(recv: &crate::Ty, name: &str) -> Option<Builtin> {
        Builtin::members_of(recv)
            .iter()
            .copied()
            .find(|builtin| builtin.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Panic => "panic",
            Builtin::AllocArray => "alloc_array",
            Builtin::DeallocArray => "dealloc_array",
            Builtin::Add => "add",
            Builtin::Offset => "offset",
            Builtin::Copy => "copy",
            Builtin::Dangling => "dangling",
            Builtin::ReadLine => "read_line",
            Builtin::NextChar => "next_char",
            Builtin::StrFromUtf8 => "str_from_utf8",
            Builtin::StrFromUtf8Unchecked => "str_from_utf8_unchecked",
            Builtin::StrLen => "len",
            Builtin::StrBytes => "str_bytes",
        }
    }

    /// Whether calling this builtin requires an enclosing
    /// `unsafe { ... }` block — exactly the operations that carry a
    /// precondition whose violation is UB even without a visible deref;
    /// each arm below says which precondition and why.
    ///
    /// Exhaustive on purpose, like [`Builtin::const_legality`]: safety is
    /// a decision about a new builtin, not a default it inherits by being
    /// left off a list.
    pub fn requires_unsafe(self) -> bool {
        match self {
            // Freeing invalidates every pointer into the allocation,
            // `copy` writes through a raw pointer, and `add`/`offset` on a
            // pointer that does not address an array element is detected
            // UB at the call.
            Builtin::DeallocArray | Builtin::Copy | Builtin::Add | Builtin::Offset => true,
            // Both blesses read a RANGE through a raw pointer on the
            // caller's word that it is readable, and the unchecked one
            // additionally claims the bytes spell a string.
            Builtin::StrFromUtf8 | Builtin::StrFromUtf8Unchecked => true,
            // Writes a RANGE through a raw pointer on the caller's word
            // that it is writable — `copy`'s destination half, with the
            // length coming from the text instead of an argument.
            Builtin::StrBytes => true,
            // Nothing to vouch for. The effects (`print`, `panic`,
            // `read_line`) have no precondition, allocating cannot UB, a
            // `dangling` pointer is hazardous only at a deref — which
            // carries its own marker — and the two `str` MEMBERS read what
            // the receiver already holds.
            Builtin::Print
            | Builtin::Panic
            | Builtin::AllocArray
            | Builtin::ReadLine
            | Builtin::Dangling
            | Builtin::NextChar
            | Builtin::StrLen => false,
        }
    }

    /// Whether this builtin may be CALLED during const evaluation, and
    /// when not, WHY — no side effect and no host dependence means what a
    /// const context computes is what every run would have computed.
    ///
    /// The reason travels with the verdict because the two refusals are
    /// different judgements with different futures (see
    /// [`ConstLegality`]), and because a caller that renders them must not
    /// have to re-derive which builtin is which.
    ///
    /// Orthogonal to [`Builtin::requires_unsafe`] — unsafe operations are
    /// legal in const evaluation (every would-be UB there is a
    /// deterministic detected trap), so `copy` and the blesses are
    /// const-legal despite their markers.
    ///
    /// The one home for the question: both ways a builtin is reached — by
    /// NAME and through the DOT as a member — ask it here, and the
    /// exhaustive match means a new variant cannot reach either checker
    /// without an answer.
    pub fn const_legality(self) -> ConstLegality {
        match self {
            // Pure: `panic` is the one side effect a const context allows,
            // pointer arithmetic and `dangling` only compute addresses,
            // and the `str` operations read and decode bytes — none of
            // them observes anything outside its own arguments.
            Builtin::Panic
            | Builtin::Add
            | Builtin::Offset
            | Builtin::Dangling
            | Builtin::NextChar
            | Builtin::StrLen
            | Builtin::StrFromUtf8
            | Builtin::StrFromUtf8Unchecked => ConstLegality::Legal,
            // They WRITE, but only through a pointer whose target already
            // exists in const memory: nothing outside the evaluation can
            // see it, and the escape rule in `eval` guards the results.
            Builtin::Copy | Builtin::StrBytes => ConstLegality::Legal,
            Builtin::Print | Builtin::ReadLine => ConstLegality::HostEffect,
            Builtin::AllocArray | Builtin::DeallocArray => ConstLegality::HeapFence,
        }
    }

    /// Whether this builtin takes a pointer argument in BOTH raw flavors —
    /// `T.&raw` and `T.&raw mut` in the same position, which no one `fn`
    /// type says. Such a builtin is not first-class (a mention that is not
    /// a call has no type to be a value at) and its call is intercepted by
    /// the checker instead of being checked against a signature.
    ///
    /// Exhaustive on purpose, like [`Builtin::const_legality`] and
    /// [`Builtin::requires_unsafe`]: a new builtin's pointer shape is a
    /// decision, not something it inherits by being left off a list.
    pub fn flavor_polymorphic(self) -> bool {
        match self {
            // Each reads (the blesses) or moves (`add`/`offset`) through a
            // pointer whose flavor the caller chose, and `copy` reads its
            // source the same way — no one `fn` type says that.
            Builtin::Add
            | Builtin::Offset
            | Builtin::Copy
            | Builtin::StrFromUtf8
            | Builtin::StrFromUtf8Unchecked => true,
            // A single fixed flavor each, so an ordinary `fn` type says
            // it: `str_bytes` WRITES through `u8.&raw mut` only,
            // `dealloc_array` frees a `.&raw mut` allocation only, and
            // `dangling` PRODUCES a pointer rather than taking one.
            Builtin::StrBytes | Builtin::DeallocArray | Builtin::Dangling => false,
            // No pointer argument to be polymorphic in at all:
            // `alloc_array` takes a count, `next_char`/`len` read through
            // the receiver's own representation, and
            // `print`/`panic`/`read_line` take no pointer either.
            Builtin::Print
            | Builtin::Panic
            | Builtin::AllocArray
            | Builtin::ReadLine
            | Builtin::NextChar
            | Builtin::StrLen => false,
        }
    }
}

/// The disambiguator reserved for compiler-provided declarations —
/// [`crate::file_item_ids`] numbers real items 0.., so no source item can
/// ever collide with it, and an [`ItemLoc`] carrying it round-trips through
/// interning like any other (its item-tree queries answer the builtin
/// shape; its *source* queries answer the empty case).
///
/// TWO kinds of compiler-provided declaration share this reserved namespace,
/// keyed only by name: the ENUMS ([`synthetic_decls`]) and the generic
/// builtin FUNCTIONS, whose schemes are keyed the same way
/// (`infer::builtin_scheme`). A collision is unconstructible today —
/// builtins are `snake_case` names and the enums are `CamelCase` ones — but
/// it is a naming convention holding it up, not a type.
pub const BUILTIN_DISAMBIGUATOR: u32 = u32::MAX;

/// The result-shaped return of `alloc_array` (and the library convention
/// for fallible allocators built over it): a compiler-provided generic enum
/// `AllocResult::<T> = enum { Ok(T.&raw mut), Err }`.
pub const ALLOC_RESULT_NAME: &str = "AllocResult";

/// The result-shaped return of `read_line` (P04): a compiler-provided
/// NON-generic enum `ReadLineResult = enum { Line(str), End }` — a line is
/// always `str`, so there is no `T` to carry. `Line` holds the line with
/// its terminator stripped; a blank line is `::Line("")`, never `::End`.
/// The idiom it exists for:
///
/// ```text
/// loop {
///     match read_line() {
///         ::Line(s) => { print(s); },
///         ::End => break,
///     }
/// }
/// ```
pub const READ_LINE_RESULT_NAME: &str = "ReadLineResult";

/// What `str.next_char(i)` answers (T16): a compiler-provided non-generic
/// enum `NextChar = enum { Char(char, usize), End }`, one more row of
/// [`synthetic_decls`].
///
/// The payload is TWO POSITIONAL fields, not a record: `char` is the scalar
/// value starting at `i`, `usize` is the byte index of the NEXT boundary —
/// the index to thread into the following call. Positional because that is
/// what pattern binding is (flat, positional, one binder per payload), so
/// the working idiom reads with no field access and no intermediate value:
///
/// ```text
/// loop {
///     match line.next_char(i) {
///         ::Char(c, next) => { /* use c */ i = next; },
///         ::End => break,
///     }
/// }
/// ```
///
/// `End` means `i` is at or past the end of the string — genuinely no
/// character here, the same "the input is over" reading `ReadLineResult`'s
/// `End` has. An `i` in the MIDDLE of a codepoint is not this case: it is a
/// program that lost track of its own index, and it traps.
pub const NEXT_CHAR_NAME: &str = "NextChar";

/// What `str_from_utf8(p, len)` answers (T17): a compiler-provided
/// non-generic enum `Utf8Result = enum { Ok(str), Err }`, one more row of
/// [`synthetic_decls`].
///
/// `Err` carries no payload, deliberately matching `AllocResult::Err`: which
/// byte broke the encoding is real information and a real *addition*, but a
/// payload cannot be taken away once every match arm has learned to bind it.
pub const UTF8_RESULT_NAME: &str = "Utf8Result";

/// A declaration the compiler provides without source: the result enum some
/// builtin returns. Provided per FILE — resolution is per-file today, so
/// each file sees "its" declaration; the identity scheme is the ordinary
/// [`ItemLoc`] one with the reserved [`BUILTIN_DISAMBIGUATOR`], which keeps
/// every downstream consumer (patterns, match lowering, widening, the
/// runtime tag) on the completely ordinary nominal-enum machinery.
pub struct SyntheticDecl {
    /// Its file-scope name. A file's own declaration of the name shadows
    /// it, the `print` precedent, and is never reported as a duplicate.
    pub name: &'static str,
    /// Its type parameters ([`ALLOC_RESULT_NAME`] has one,
    /// [`READ_LINE_RESULT_NAME`] none).
    pub generics: Vec<GenericParamData>,
    /// Its variants, IN SOURCE ORDER, each with its POSITIONAL payload — a
    /// variant's INDEX is its identity at the type, value and runtime-tag
    /// level, so nothing here is sorted and no reader may re-derive the
    /// order. The declaration is stated here and nowhere else, so nothing
    /// downstream needs a special case.
    pub variants: Vec<(String, Vec<TypeRef>)>,
}

impl SyntheticDecl {
    /// The INDEX and payload shape of this declaration's variant `name` —
    /// the one place a variant's runtime tag comes from.
    ///
    /// The interpreter's `builtin_variant` needs both to build a tagged
    /// value (this row's order IS the tag `SwitchVariant` dispatches on),
    /// and reading them off the row is what keeps a reordered table from
    /// silently misrouting values: the row moves, the tag moves with it.
    pub fn variant(&self, name: &str) -> Option<(u32, &[TypeRef])> {
        self.variants
            .iter()
            .position(|(variant, _)| variant == name)
            .map(|index| (index as u32, self.variants[index].1.as_slice()))
    }
}

/// Every compiler-provided declaration, stated once. The FIVE sites that
/// must know about them — [`type_scope`], [`file_scope`],
/// [`crate::item_data`], [`crate::type_decl`] and the interpreter's
/// `builtin_variant` (the first cross-crate reader of a row's variant
/// order) — each handle "a synthetic declaration", so another one is a row
/// here rather than another special case in each of them.
///
/// THE PRELUDE MUST CANNOT WRITE YET. These are ordinary Must
/// declarations in every respect except where they come from — nothing here
/// is a compiler concept, it is source the language has no place to put.
/// When modules and cross-file resolution land, the prelude becomes a Must
/// file and this table DELETES ITSELF; every concept invented here has to be
/// unwound then, so the table stays deliberately concept-poor (a name, a
/// binder, variants — the item tree's own types, no mirror of them).
///
/// SHADOWABLE, and KIND-BLIND about it: a file that declares the name as a
/// TYPE sees its own everywhere (the `print` precedent). A file that
/// declares it as a VALUE (`static NextChar = 'x';`) takes the name just as
/// completely — both registrations ask only whether the name is declared, so
/// the builtin type is simply not there and an annotation naming it is an
/// ordinary "`NextChar` is not a type". Pinned by
/// `a_value_item_taking_a_builtin_enum_name_takes_the_type_with_it`.
pub fn synthetic_decls() -> &'static [SyntheticDecl] {
    static DECLS: std::sync::LazyLock<Vec<SyntheticDecl>> = std::sync::LazyLock::new(|| {
        vec![
            // `AllocResult::<T> = enum { Ok(T.&raw mut), Err }` — the one
            // generic row. `AllocResult::<T>` only ever holds a POINTER to a
            // `T` (`Ok(T.&raw mut)`), never a `T`, so a linear `T` costs
            // it nothing — and a data-side parameter asks for no
            // capability anyway (T22).
            SyntheticDecl {
                name: ALLOC_RESULT_NAME,
                generics: vec![type_param("T")],
                variants: vec![
                    ("Ok".to_owned(), vec![raw_ptr_mut("T")]),
                    ("Err".to_owned(), Vec::new()),
                ],
            },
            // `ReadLineResult = enum { Line(str), End }` — non-generic: a
            // line is always `str`, so there is no `T` to carry.
            SyntheticDecl {
                name: READ_LINE_RESULT_NAME,
                generics: Vec::new(),
                variants: vec![
                    ("Line".to_owned(), vec![path("str")]),
                    ("End".to_owned(), Vec::new()),
                ],
            },
            // `NextChar = enum { Char(char, usize), End }` — the second
            // payload is the byte index of the next boundary: the value
            // threaded into the following call.
            SyntheticDecl {
                name: NEXT_CHAR_NAME,
                generics: Vec::new(),
                variants: vec![
                    ("Char".to_owned(), vec![path("char"), path("usize")]),
                    ("End".to_owned(), Vec::new()),
                ],
            },
            // `Utf8Result = enum { Ok(str), Err }` — `Ok` carries the
            // blessed view; `Err` carries nothing, matching
            // `AllocResult::Err`.
            SyntheticDecl {
                name: UTF8_RESULT_NAME,
                generics: Vec::new(),
                variants: vec![
                    ("Ok".to_owned(), vec![path("str")]),
                    ("Err".to_owned(), Vec::new()),
                ],
            },
        ]
    });
    &DECLS
}

/// `T` — a named type, the payload spelling every row but `AllocResult`'s
/// `Ok` uses.
fn path(name: &str) -> TypeRef {
    TypeRef::Path(name.to_owned())
}

/// `T.&raw mut` — a mutable raw pointer to a named type.
fn raw_ptr_mut(name: &str) -> TypeRef {
    TypeRef::RawPtr {
        mutable: true,
        inner: Box::new(path(name)),
    }
}

/// A rigid, unbounded TYPE parameter — the only binder shape any row needs
/// so far; a bounded parameter (or a const or region one) would be its own
/// helper beside this, spelled where the reader is already looking.
fn type_param(name: &str) -> GenericParamData {
    GenericParamData {
        name: name.to_owned(),
        kind: GenericParamKind::Type,
        bounds: Vec::new(),
        outlives: Vec::new(),
        forget: false,
    }
}

/// The compiler-provided declaration named `name`, if there is one.
pub fn synthetic_decl_named(name: &str) -> Option<&'static SyntheticDecl> {
    synthetic_decls().iter().find(|decl| decl.name == name)
}

/// The [`ItemLoc`] of `file`'s copy of the synthetic declaration `name`.
///
/// `name` must BE a row's: a reserved-disambiguator location for a name no
/// table row declares is a stale id nothing will ever answer for — a typo,
/// not a case to handle.
pub fn synthetic_decl_loc(file: SourceFile, name: &str) -> ItemLoc {
    debug_assert!(
        synthetic_decl_named(name).is_some(),
        "`{name}` is not a compiler-provided declaration (see `synthetic_decls`)"
    );
    ItemLoc::top_level(file, std::sync::Arc::from(name), BUILTIN_DISAMBIGUATOR)
}

/// The [`ItemLoc`] of `file`'s compiler-provided [`ALLOC_RESULT_NAME`] enum.
pub fn alloc_result_loc(file: SourceFile) -> ItemLoc {
    synthetic_decl_loc(file, ALLOC_RESULT_NAME)
}

/// The [`ItemLoc`] of `file`'s compiler-provided [`READ_LINE_RESULT_NAME`]
/// enum.
pub fn read_line_result_loc(file: SourceFile) -> ItemLoc {
    synthetic_decl_loc(file, READ_LINE_RESULT_NAME)
}

/// The [`ItemLoc`] of `file`'s compiler-provided [`NEXT_CHAR_NAME`] enum.
pub fn next_char_loc(file: SourceFile) -> ItemLoc {
    synthetic_decl_loc(file, NEXT_CHAR_NAME)
}

/// The [`ItemLoc`] of `file`'s compiler-provided [`UTF8_RESULT_NAME`] enum.
pub fn utf8_result_loc(file: SourceFile) -> ItemLoc {
    synthetic_decl_loc(file, UTF8_RESULT_NAME)
}

/// Top-level names of a file, *including* what's wrong with them: the scope
/// decides that the first declaration wins, so it is also the analysis that
/// knows about the losers. Diagnostics travel with the analysis that
/// discovers them; the aggregator only attaches ranges. Range-free, so body
/// edits backdate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileScope {
    entries: FxHashMap<String, ScopeEntry>,
    /// One entry per extra declaration of an already-declared name.
    pub duplicates: Vec<Duplicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopeEntry {
    loc: ItemLoc,
    kind: ItemKind,
    ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub first: ItemLoc,
    pub second: ItemLoc,
}

impl FileScope {
    pub fn resolve(&self, name: &str) -> Option<Resolution> {
        let entry = self.entries.get(name)?;
        Some(if entry.ambiguous {
            Resolution::Ambiguous(entry.loc.clone())
        } else {
            match entry.kind {
                ItemKind::Value(_) | ItemKind::Member => Resolution::Item(entry.loc.clone()),
                ItemKind::Type => Resolution::TypeItem(entry.loc.clone()),
                ItemKind::Trait => Resolution::TraitItem(entry.loc.clone()),
            }
        })
    }

    /// Every unambiguous name this file declares, each with its resolution —
    /// completions' candidate source (used instead of [`Self::resolve`] when
    /// the caller wants *all* names, not one). An ambiguous name is skipped:
    /// no single resolution names a symbol worth offering, and its
    /// definitions already carry the diagnostic.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Resolution)> {
        self.entries.iter().filter_map(|(name, entry)| {
            if entry.ambiguous {
                return None;
            }
            let resolution = match entry.kind {
                ItemKind::Value(_) | ItemKind::Member => Resolution::Item(entry.loc.clone()),
                ItemKind::Type => Resolution::TypeItem(entry.loc.clone()),
                ItemKind::Trait => Resolution::TraitItem(entry.loc.clone()),
            };
            Some((name.as_str(), resolution))
        })
    }
}

/// The type-item names of a file — the slice of [`FileScope`] that type
/// *annotation* lowering depends on. Its own query so that adding or
/// removing a **value** item leaves this value unchanged and inference of
/// annotated items backdates behind it (the item-insertion firewall test
/// pins this); depending on the full [`file_scope`] from `lower_type_ref`
/// would re-run every annotated item's inference on any item insertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeScope {
    entries: FxHashMap<String, TypeScopeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeScopeEntry {
    loc: ItemLoc,
    /// Whether the name is declared more than once file-wide (by items of
    /// *any* kind): an ambiguous name names no type, and a value item
    /// stealing a type's name must flip this value (so dependents re-run).
    ambiguous: bool,
}

impl TypeScope {
    pub fn resolve(&self, name: &str) -> Option<Resolution> {
        let entry = self.entries.get(name)?;
        Some(if entry.ambiguous {
            Resolution::Ambiguous(entry.loc.clone())
        } else {
            Resolution::TypeItem(entry.loc.clone())
        })
    }

    /// Every unambiguous type name this file declares — completions' type-
    /// position candidate source. Same ambiguity handling as
    /// [`FileScope::iter`].
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ItemLoc)> {
        self.entries
            .iter()
            .filter_map(|(name, entry)| (!entry.ambiguous).then_some((name.as_str(), &entry.loc)))
    }
}

#[salsa::tracked(returns(ref))]
pub fn type_scope(db: &dyn Db, file: SourceFile) -> TypeScope {
    let tree = item_tree(db, file);
    let mut counts: FxHashMap<&str, u32> = FxHashMap::default();
    for data in tree.items.iter() {
        *counts.entry(data.name.as_str()).or_insert(0) += 1;
    }
    // Disambiguators count occurrences the same way `file_item_ids` does,
    // so an `ItemLoc` here and the interned `ItemId` agree on identity.
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    let mut scope = TypeScope::default();
    for data in tree.items.iter() {
        let disambiguator = {
            let counter = seen.entry(data.name.as_str()).or_insert(0);
            let current = *counter;
            *counter += 1;
            current
        };
        if data.name.is_empty() || !matches!(data.kind, ItemKind::Type) {
            continue;
        }
        let loc = ItemLoc::top_level(
            file,
            std::sync::Arc::from(data.name.as_str()),
            disambiguator,
        );
        scope
            .entries
            .entry(data.name.clone())
            .or_insert(TypeScopeEntry {
                loc,
                ambiguous: counts[data.name.as_str()] > 1,
            });
    }
    // The compiler-provided declarations, visible everywhere like the
    // builtin value names — inserted only when the file doesn't declare the
    // name itself (user declarations shadow builtins, the `print`
    // precedent), and never counted as a duplicate.
    for decl in synthetic_decls() {
        if counts.contains_key(decl.name) {
            continue;
        }
        scope.entries.insert(
            decl.name.to_owned(),
            TypeScopeEntry {
                loc: synthetic_decl_loc(file, decl.name),
                ambiguous: false,
            },
        );
    }
    scope
}

/// Items are visible everywhere, including their own bodies — mutual
/// recursion needs no special casing.
#[salsa::tracked(returns(ref))]
pub fn file_scope(db: &dyn Db, file: SourceFile) -> FileScope {
    let tree = item_tree(db, file);
    let mut scope = FileScope::default();
    // Disambiguators count occurrences the same way `file_item_ids` does,
    // so an `ItemLoc` here and the interned `ItemId` agree on identity.
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    for data in tree.items.iter() {
        let disambiguator = {
            let counter = seen.entry(data.name.as_str()).or_insert(0);
            let current = *counter;
            *counter += 1;
            current
        };
        if data.name.is_empty() {
            continue;
        }
        let loc = ItemLoc::top_level(
            file,
            std::sync::Arc::from(data.name.as_str()),
            disambiguator,
        );
        match scope.entries.entry(data.name.clone()) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(ScopeEntry {
                    loc,
                    kind: data.kind,
                    ambiguous: false,
                });
            }
            std::collections::hash_map::Entry::Occupied(mut first) => {
                first.get_mut().ambiguous = true;
                scope.duplicates.push(Duplicate {
                    first: first.get().loc.clone(),
                    second: loc,
                });
            }
        }
    }
    // The compiler-provided declarations — shadowable, never a duplicate
    // (see `type_scope`).
    for decl in synthetic_decls() {
        if scope.entries.contains_key(decl.name) {
            continue;
        }
        scope.entries.insert(
            decl.name.to_owned(),
            ScopeEntry {
                loc: synthetic_decl_loc(file, decl.name),
                kind: ItemKind::Type,
                ambiguous: false,
            },
        );
    }
    scope
}

/// Resolution of every `NameRef` expression in `item`'s body. A `NameRef`
/// absent from the map is unresolved.
#[salsa::tracked(returns(ref))]
pub fn resolutions<'db>(db: &'db dyn Db, item: ItemId<'db>) -> ArenaMap<ExprId, Resolution> {
    let body = body(db, item);
    let scopes = expr_scopes(db, item);
    let file_scope = file_scope(db, item.file(db));
    // The item's own generic binder: const params are value names for the
    // whole body (the body IS the binder's literal for a generic item), a
    // scope layer between the locals and the file. A name duplicated
    // within ONE binder is diagnosed at its second occurrence (see
    // `file_diagnostics`'s duplicate-generic-parameter pass), so this
    // fallback lookup only ever has to pick one, total-lowering-style;
    // last declaration wins the same way local shadowing does.
    let generics = &crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let const_param = |name: &str| {
        generics
            .iter()
            .enumerate()
            .rev()
            .find(|(_, param)| {
                matches!(param.kind, crate::item_tree::GenericParamKind::Const(_))
                    && param.name == name
                    && !param.name.is_empty()
            })
            .map(|(index, _)| Resolution::ConstParam(index as u32))
    };
    // Inside a MEMBER's body, `Self` in expression position names the
    // owning type (so `Self(struct { ... })` constructs it) — a scope
    // layer between the locals and the file, like the binder's const
    // params.
    let self_owner = crate::member_owner(db, item).map(|owner| crate::item_loc(db, owner));
    let mut map = ArenaMap::default();
    for (expr, data) in body.exprs.iter() {
        let ExprData::NameRef(name) = data else {
            continue;
        };
        let local = scopes
            .scope_of(expr)
            .and_then(|scope| scopes.resolve_in_scope(scope, name));
        let resolution = local.map(Resolution::Local).or_else(|| {
            const_param(name).or_else(|| {
                if name == "Self"
                    && let Some(owner) = &self_owner
                {
                    return Some(Resolution::TypeItem(owner.clone()));
                }
                file_scope
                    .resolve(name)
                    .or_else(|| Builtin::by_name(name).map(Resolution::Builtin))
            })
        });
        if let Some(resolution) = resolution {
            map.insert(expr, resolution);
        }
    }
    map
}
