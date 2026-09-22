//! Loan liveness: the static exclusivity fence.
//!
//! A safe borrow may not survive its root's invalidation. That is the
//! safety contract of a safe borrow, and this pass is what enforces it
//! (M14). The interpreter's aliasing tree detects the same violations
//! dynamically, and it is DEPTH — raw pointers, freed allocations, the
//! paths this pass over-approximates — never the fence.
//!
//! The rule, in one sentence: **no loan may still be live where an access
//! foreign to it touches an overlapping part of the same root local.** Two
//! access kinds, mirroring `Machine::aliasing_access`:
//!
//! | access | what it kills |
//! |---|---|
//! | WRITE — a `&mut` mint (written or inserted), an assignment, a MOVE of a whole local | every overlapping loan |
//! | READ — a `.&` mint, or a by-name value read of the place | overlapping EXCLUSIVE loans |
//!
//! A read spares shared loans because any number of readers coexist. A
//! move is a write through the ROOT rather than at a path, because what it
//! takes away is the storage every loan of that local points into.
//!
//! # The shape: NLL over MIR
//!
//! Textbook non-lexical lifetimes over the CFG lowering already built
//! (X06, X07), in four steps:
//!
//! 1. **Local liveness**, backward over the blocks: which MIR locals are
//!    live before each statement.
//! 2. **Region liveness.** A region is live at a point if some local live
//!    there holds a value whose type mentions a region it covers, and at
//!    EVERY point if it covers one of the signature's universals — the
//!    caller's region outlives the whole body. "Covers" is
//!    [`hir::outlives::RegionCover`]: the outlives graph inference
//!    recorded, closed under transitivity, consumed rather than re-derived
//!    (X10). MIR types are region-erased, so a local's regions come from
//!    hir by provenance: a named local's binding type, a temp's defining
//!    expression, and for the temp that holds a fresh loan the loan's own
//!    region from [`InferenceResult::loan_regions`].
//! 3. **Loans in scope**, forward from each [`Rvalue::Borrow`]: a loan is
//!    in scope at every point reachable from its mint without passing an
//!    assignment to the borrowed place or a prefix of it, and without
//!    passing a point where its region is not live. The second exit is
//!    what makes the walk location-sensitive where step 2 is not: a
//!    holder's one region covers every loan it ever held, so a holder
//!    reassigned after its old loan went dead would otherwise revive it.
//!    Once out of scope a loan stays out (rustc's
//!    `borrows_out_of_scope_at_location`), so `r = b.&mut` in a straight
//!    line or a loop frees `a`, and only a reassignment on SOME paths
//!    keeps `a`'s loan in scope on the others.
//! 4. **Conflict.** An access conflicts with a loan in scope at that
//!    point, rooted in the same local, over an overlapping path
//!    ([`crate::paths_overlap`]), of a flavour the access is foreign to.
//!    Blamed at the access, with the loan's mint and the next use of a
//!    live holder as companions, in the interpreter's own vocabulary. One
//!    report per access expression and loan: the two MIR points of
//!    `a = a + 1` share an origin and report once, while a call that
//!    reads two borrowed locals reports each loan it kills.
//!
//! One hop is dated rather than taken everywhere. Region liveness above is
//! location-insensitive about containment: a loan whose region covers a
//! region `X` is live wherever `X` is, whenever the flow into `X` happened.
//! That costs nothing while `X` is live only through a holder — the
//! holder's uses already say where — and everything when `X` is a
//! universal, which is live at every point with no holder at all: a loan
//! handed back from one `match` arm would be live in the arm beside it,
//! and `get_or_default`'s `::None => { map.insert(..); .. }` would be
//! refused (NLL's conditional-return case). So the flow of a loan INTO a
//! universal is dated: the obligation inference recorded for it has an
//! origin, that origin is a point of this body, and the loan is live via
//! the universal at every point reachable from there — not before, and
//! not in a sibling arm. A flow whose origin is not a point of this body
//! is taken everywhere, which is the refusing direction. This is
//! origin tracking for the one origin where it changes an answer; every
//! other hop stays location-insensitive.
//!
//! Loans are in scope FORWARD from the mint, so `stdin_lib`'s `next_line`,
//! whose `return ::Some(r.*.line.&)` sits beside an `r.refill()`, is
//! accepted by reachability alone: no path from a mint that returns
//! reaches the reborrow beside it. Nothing about branches or loops is
//! special-cased.
//!
//! # Places
//!
//! MIR only ever leads a projection with a deref, so a nested pointer place
//! (`bb.*.*`, `p.q.*`) is spelled as a copy of the inner place into a temp
//! followed by a deref of the temp. For loans and accesses that temp IS the
//! place it copied: a compiler temp defined once by a `Use(Copy(place))`
//! and projected through is resolved back to `place` with the rest of the
//! projection appended, so the loan `bb.*.*.&mut` is rooted at `bb` and a
//! later write through `bb` finds it. The read that defines such a temp
//! is NO access: the access through the temp is the one operation the
//! user wrote, and it subsumes the read — a loan overlapping the prefix
//! `bb.*` either is a prefix of the longer path too, or extends below the
//! deref where only a genuinely disjoint sibling (`bb.*.*.g` beside
//! `bb.*.*.f`) fails to overlap, and that sibling must be accepted.
//! Counting the read would refuse it. A temp only ever read whole — the
//! operand of a tail `r.*`, the return slot — holds a value, not a place:
//! its defining read is the access, and copying it on is none. A temp with
//! more than one definition (a joined value dereferenced in place) is left
//! as its own root, and a loan rooted there conflicts with nothing — a
//! known accepting gap, not a shape the language produces today.
//!
//! An ASSIGNMENT is shallow: `p = q` overwrites the pointer and touches
//! nothing it points at, so it does not conflict with a loan through
//! `p.*` — it KILLS that loan, which was of the old pointee. Every other
//! access is deep: moving, reading or borrowing `p` reaches what `p`
//! points at.
//!
//! # Stricter than the interpreter, on purpose
//!
//! * A foreign read is REFUSED, where the tree merely freezes the exclusive
//!   loan it crosses. An exclusive borrow that can be read around is not
//!   exclusive; this is the ordinary NLL rule (Rust's E0502), and the
//!   tree's freeze is operational semantics for unsafe code.
//! * Every array index overlaps every other — `arr[i]` is not a static
//!   fact — where the tree carries the resolved index.
//!
//! # Storage death
//!
//! A nested block's locals — its `let`s, the temporaries materialized in
//! it — die at every exit of the block: lowering marks each with a
//! [`StatementKind::StorageDead`] at the block's end and at each `break`
//! and `continue` leaving it. A match arm is a scope of the same kind,
//! holding its pattern's binders. The marker is the last shallow write
//! of the whole local ([`AccessKind::StorageEnd`]): a loan of the local's
//! own storage that is still live there is refused, blamed at the exit,
//! and a loan THROUGH the local (of what it points at) passes. A loop
//! body is a nested block, so its storage ends before every path back to
//! the header. The body's outermost block is the frame, and its end is
//! the return below.
//!
//! # Coarse, and known
//!
//! * Blame sits at a statement's origin. A bare-name operand has no
//!   expression of its own, so `take(a)` squiggles the call and an
//!   assignment squiggles its value — where the interpreter's "invalidated
//!   here" note lands too; a projected argument (`take(p.*.f)`) is read
//!   into a temp at the read's own expression and squiggles the read. A
//!   place is rendered as the user
//!   wrote it, except that an inserted reborrow names the holder the user
//!   mentioned (`bump(r)` is "using `r`", not `r.*`).
//!
//! # Escapes
//!
//! `Return` is the storage end of every local. A loan of a local's own
//! storage that is still live there escapes the body; `hir::outlives`
//! already reports the ones forced to a universal's end, and this pass
//! reports the rest — a nested literal returning a borrow of its own local
//! at `@_`, which reaches no universal of the enclosing item.
//!
//! Nothing here plants a trap: a refused program still runs, and the
//! interpreter's detection is what stops it.

use base_db::Db;
use hir::infer::InferenceResult;
use hir::outlives::RegionCover;
use hir::ty::{GenericArg, Region};
use hir::{BindingId, ExprId, ItemId, RegionConstraint, Ty};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::TextRange;

use crate::{
    BlockId, LocalId, MirBody, Operand, Place, ProjElem, Rvalue, Statement, StatementKind,
    TerminatorKind, paths_overlap,
};

/// One finding. Carries the expressions to blame and everything the
/// message needs; ranges attach in the aggregator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoanDiagnostic {
    /// An access invalidated a loan of the same place that is still live —
    /// the stale-loan refusal, blamed at the invalidating access: it is
    /// the operation the checker refuses, it is always in this body (the
    /// stale use routinely is not — a view handed to a callee is read
    /// there), and it is the site the analysis knows exactly.
    Invalidated {
        /// The access that kills the loan (a statement's origin).
        access: ExprId,
        /// What the user did there, which is what the message names.
        kind: AccessKind,
        /// The place the access names, rendered for diagnostics.
        place: String,
        /// The loan it kills — the borrow's own mint site.
        borrow: ExprId,
        /// Where that loan is still needed.
        still_used: StillUsed,
    },
    /// A loan of a local's own storage is still live where the body
    /// returns — the storage is gone by then.
    Escapes {
        borrow: ExprId,
        /// Whether the storage is a materialized temporary (M12).
        temporary: bool,
    },
}

/// The operation that invalidated a loan, named as the user wrote it. Two
/// access kinds in the model — read and write — and five spellings here,
/// because what the message has to say is what the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    /// A `&mut` mint, written or inserted. Foreign to every overlapping
    /// loan, whatever its flavour.
    MutBorrow,
    /// An assignment to the place — `n = 99`, `r.*.f = v`.
    Write,
    /// A `.&` mint. A foreign READ: it suspends an exclusive loan and
    /// leaves shared ones alone.
    SharedBorrow,
    /// A by-name value read of the place. Same access as a `.&` mint.
    Read,
    /// A whole-local move — including the by-name read of a value that
    /// cannot be duplicated. A write through the root.
    Move,
    /// The end of the local's storage at its block's exit
    /// ([`StatementKind::StorageDead`]): the last write to the whole
    /// local, foreign to every loan of its storage.
    StorageEnd,
}

impl AccessKind {
    fn is_write(self) -> bool {
        matches!(
            self,
            AccessKind::MutBorrow | AccessKind::Write | AccessKind::Move | AccessKind::StorageEnd
        )
    }
}

/// Where the loan the access killed is still needed — the second half of
/// the message, which is not the same question as what the access did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StillUsed {
    /// The ordinary case: a holder of the loan is used at this expression,
    /// below the access.
    Later(ExprId),
    /// A holder is used at this expression ABOVE the access: control only
    /// comes back round to it on the loop's next iteration.
    NextIteration(ExprId),
    /// The invalidating operation itself is where a holder is needed.
    AtTheAccess,
    /// Handed to the caller through a signature's region. There is no
    /// later use in this body to point at.
    HandedBack,
}

impl LoanDiagnostic {
    pub fn expr(&self) -> ExprId {
        match self {
            LoanDiagnostic::Invalidated { access, .. } => *access,
            LoanDiagnostic::Escapes { borrow, .. } => *borrow,
        }
    }

    /// Where the squiggle goes: the blamed expression's node, except that
    /// a storage end sits where the block is left — its closing brace, or
    /// the `break`/`continue` that exits it — not on the whole block.
    pub fn range(&self, source_map: &hir::BodySourceMap) -> Option<TextRange> {
        let expr = self.expr();
        if let LoanDiagnostic::Invalidated {
            kind: AccessKind::StorageEnd,
            ..
        } = self
        {
            return source_map.exit_range_for_expr(expr);
        }
        Some(source_map.node_for_expr(expr)?.text_range())
    }

    /// The companion sites, with the note each carries. The vocabulary is
    /// the interpreter's own ("this borrow was created here"), so the
    /// static refusal and the dynamic detection read as one story told at
    /// two different times.
    pub fn related(&self) -> Vec<(ExprId, &'static str)> {
        match self {
            LoanDiagnostic::Invalidated {
                borrow, still_used, ..
            } => {
                let mut notes = vec![(*borrow, "this borrow was created here")];
                match still_used {
                    StillUsed::Later(at) => notes.push((*at, "and it is still used here")),
                    StillUsed::NextIteration(at) => notes.push((
                        *at,
                        "and the loop brings control back round to this use of it",
                    )),
                    StillUsed::AtTheAccess | StillUsed::HandedBack => {}
                }
                notes
            }
            LoanDiagnostic::Escapes { .. } => Vec::new(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            // Named as the operation the user performed: under
            // reborrow-at-every-use a plain mention of a borrow IS a
            // `&mut` mint, so "reborrowing" would point at code with no
            // visible borrow in it. "using `r` mutably here" is true of
            // both spellings.
            LoanDiagnostic::Invalidated {
                kind,
                place,
                still_used,
                ..
            } => {
                // `place` arrives rendered, e.g. "this temporary" or "`n`",
                // giving "writing to this temporary here" or "writing to `n` here".
                let did = match kind {
                    AccessKind::MutBorrow => format!("using {place} mutably here"),
                    AccessKind::Write => format!("writing to {place} here"),
                    AccessKind::SharedBorrow => format!("borrowing {place} here"),
                    AccessKind::Read => format!("reading {place} here"),
                    AccessKind::Move => format!("moving {place} here"),
                    AccessKind::StorageEnd => {
                        format!("leaving this block ends the storage of {place}, which")
                    }
                };
                let victim = if kind.is_write() {
                    "a borrow of it"
                } else {
                    "an exclusive borrow of it"
                };
                let because = match (still_used, kind) {
                    (StillUsed::HandedBack, _) => {
                        "the borrow is handed back to the caller, and this invalidates it \
                         before the caller can read it"
                    }
                    (StillUsed::NextIteration(_), AccessKind::StorageEnd) => {
                        "the loop brings control back round to a use of the borrow, which \
                         would then read storage that no longer exists"
                    }
                    (StillUsed::NextIteration(_), _) => {
                        "the loop brings control back round to a use of the borrow, which \
                         would then read through an invalidated borrow"
                    }
                    (StillUsed::AtTheAccess, _) => {
                        "the borrow is still needed by this very operation"
                    }
                    (StillUsed::Later(_), AccessKind::SharedBorrow | AccessKind::Read) => {
                        "a `.&mut` is the only way to the value while it lasts, and this \
                         one is used after this point"
                    }
                    (StillUsed::Later(_), AccessKind::Move) => {
                        "the borrow points into storage this move takes away, and it is \
                         used after this point"
                    }
                    (StillUsed::Later(_), AccessKind::StorageEnd) => {
                        "the borrow is used after this point, and reading through it then \
                         would read storage that no longer exists"
                    }
                    (StillUsed::Later(_), _) => {
                        "the borrow is used after this point, and reading through it then \
                         would read through an invalidated borrow"
                    }
                };
                format!("{did} invalidates {victim} that is still live: {because}")
            }
            LoanDiagnostic::Escapes {
                temporary: true, ..
            } => "borrowed value does not live long enough: this borrows a temporary, \
                  which lives no longer than the block that creates it, but the borrow \
                  is still live when the body returns"
                .to_owned(),
            LoanDiagnostic::Escapes { .. } => "borrowed value does not live long enough: this \
                                              borrows a local, but the borrow is still live \
                                              when the body returns and the local is gone by \
                                              then"
                .to_owned(),
        }
    }
}

/// Loan-check every body lowered from one item. Keyed by item because the
/// bodies share the item's one inference result and region numbering;
/// each body is analysed on its own CFG.
#[salsa::tracked(returns(ref))]
pub fn loan_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<LoanDiagnostic> {
    let lowered = crate::mir_lowered(db, item);
    let hir_body = hir::body::body(db, item);
    let infer = hir::infer::infer(db, item);
    let cover = hir::outlives::region_cover(db, item);
    let mut diagnostics = Vec::new();
    for (_, body) in lowered.bodies.iter() {
        BodyCheck::new(db, body, hir_body, infer, cover).run(&mut diagnostics);
    }
    diagnostics
}

/// A location in one body: before statement `index` of `block`, or before
/// the terminator when `index` is the statement count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Point {
    block: BlockId,
    index: usize,
}

/// One loan: what a [`Rvalue::Borrow`] minted, with its place resolved
/// through any temp that stands for a nested place.
#[derive(Debug, Clone)]
struct Loan {
    origin: ExprId,
    mint: Point,
    mutable: bool,
    place: Place,
    /// Whether the region covers a universal — what the escape check in
    /// `hir::outlives` measures, so a loan of a local's storage with this
    /// set is already reported there.
    reaches_universal: bool,
    /// The region graph nodes the loan's region covers, sorted.
    covers: Vec<u32>,
    /// Points at which the loan flows into a universal: from each of them
    /// onward the loan is live with no holder needed.
    flow_points: Vec<Point>,
    /// A flow into a universal that has no point in this body (the region
    /// is a universal itself, or the obligation's origin is not a
    /// statement here): live at every point in scope.
    flows_everywhere: bool,
}

/// One access, in the model's terms.
struct Access {
    kind: AccessKind,
    place: Place,
    /// An assignment touches the place and nothing it points through.
    shallow: bool,
}

/// A dense set of locals (or loans), indexed by arena position.
#[derive(Clone, PartialEq, Eq)]
struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    fn new(len: usize) -> BitSet {
        BitSet {
            words: vec![0; len.div_ceil(64)],
        }
    }

    fn contains(&self, index: usize) -> bool {
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    /// Whether the bit was newly set.
    fn insert(&mut self, index: usize) -> bool {
        let was = self.contains(index);
        self.words[index / 64] |= 1 << (index % 64);
        !was
    }

    fn remove(&mut self, index: usize) {
        self.words[index / 64] &= !(1 << (index % 64));
    }

    fn union_with(&mut self, other: &BitSet) {
        for (word, more) in self.words.iter_mut().zip(&other.words) {
            *word |= more;
        }
    }

    fn intersect_with(&mut self, other: &BitSet) {
        for (word, mask) in self.words.iter_mut().zip(&other.words) {
            *word &= mask;
        }
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(w, &word)| {
            (0..64)
                .filter(move |bit| word & (1 << bit) != 0)
                .map(move |bit| w * 64 + bit)
        })
    }
}

struct BodyCheck<'a> {
    db: &'a dyn Db,
    body: &'a MirBody,
    hir_body: &'a hir::Body,
    infer: &'a InferenceResult,
    cover: &'a RegionCover,
    /// Per local: the place it stands for, when it is a compiler temp
    /// holding one copy of a place (see the module doc, "Places").
    aliases: Vec<Option<Place>>,
    /// Per local: the region graph nodes its value's type mentions, sorted.
    local_regions: Vec<Vec<u32>>,
    /// Per local: whether a plain read of it duplicates the value. A read
    /// of a local that cannot be duplicated is a move.
    local_copyable: Vec<bool>,
    loans: Vec<Loan>,
    /// Per block, per point (statements then terminator): the locals live
    /// there — used at that point, or later on some path.
    live_before: Vec<Vec<BitSet>>,
    /// Per block: every block reachable from it by one or more edges.
    reach: Vec<BitSet>,
    /// Per block, per point: the region graph nodes some local live there
    /// keeps live — the union of the live locals' `local_regions`.
    live_regions: Vec<Vec<BitSet>>,
    /// Per block, per point: the loans whose region is live there. Fixed
    /// once liveness is, so the forward walk applies it as a mask.
    live_loans: Vec<Vec<BitSet>>,
}

impl<'a> BodyCheck<'a> {
    fn new(
        db: &'a dyn Db,
        body: &'a MirBody,
        hir_body: &'a hir::Body,
        infer: &'a InferenceResult,
        cover: &'a RegionCover,
    ) -> BodyCheck<'a> {
        let mut check = BodyCheck {
            db,
            body,
            hir_body,
            infer,
            cover,
            aliases: Vec::new(),
            local_regions: Vec::new(),
            local_copyable: Vec::new(),
            loans: Vec::new(),
            live_before: Vec::new(),
            reach: Vec::new(),
            live_regions: Vec::new(),
            live_loans: Vec::new(),
        };
        check.aliases = check.compute_aliases();
        let (regions, copyable) = check.compute_local_types();
        check.local_regions = regions;
        check.local_copyable = copyable;
        check.reach = check.compute_reach();
        check.loans = check.collect_loans();
        check.live_before = check.compute_liveness();
        check.live_regions = check.compute_live_regions();
        check.live_loans = check.compute_live_loans();
        check
    }

    fn compute_reach(&self) -> Vec<BitSet> {
        let blocks = self.body.blocks.len();
        let mut reach: Vec<BitSet> = vec![BitSet::new(blocks); blocks];
        for (block, _) in self.body.blocks.iter() {
            let mut stack: Vec<BlockId> = self.successors(block);
            while let Some(next) = stack.pop() {
                let index = Self::block_index(next);
                if reach[Self::block_index(block)].insert(index) {
                    stack.extend(self.successors(next));
                }
            }
        }
        reach
    }

    /// Whether `from` is `to` or a point control can reach from it.
    fn reaches(&self, from: Point, to: Point) -> bool {
        (from.block == to.block && from.index <= to.index)
            || self.reach[Self::block_index(from.block)].contains(Self::block_index(to.block))
    }

    fn local_index(local: LocalId) -> usize {
        u32::from(local.into_raw()) as usize
    }

    fn block_index(block: BlockId) -> usize {
        u32::from(block.into_raw()) as usize
    }

    // ---- per-local facts --------------------------------------------------

    /// Every point that DEFINES the whole of a local, with its origin.
    fn defs_of(&self, local: LocalId) -> Vec<(Point, ExprId)> {
        let mut defs = Vec::new();
        for (block, data) in self.body.blocks.iter() {
            for (index, stmt) in data.statements.iter().enumerate() {
                let StatementKind::Assign { dest, .. } = &stmt.kind else {
                    continue;
                };
                if dest.local == local && dest.projection.is_empty() {
                    defs.push((Point { block, index }, stmt.origin));
                }
            }
            let dest = match &data.terminator.kind {
                TerminatorKind::Call { dest, .. } | TerminatorKind::Trap { dest, .. } => {
                    Some(*dest)
                }
                _ => None,
            };
            if dest == Some(local) {
                defs.push((
                    Point {
                        block,
                        index: data.statements.len(),
                    },
                    data.terminator.origin,
                ));
            }
        }
        defs
    }

    /// The temps standing for a nested place: unbound, defined once by a
    /// copy of a place, and projected through somewhere. The last part is
    /// what makes a temp a PLACE rather than a value: a temp holding a
    /// copied value — the return slot, the operand of a tail `r.*` — is
    /// only ever read whole, and no loan is rooted in it. The copy that
    /// defines a place temp performs no access of its own; every access
    /// through the temp resolves to the place it stands for.
    fn compute_aliases(&self) -> Vec<Option<Place>> {
        let mut projected_through = BitSet::new(self.body.locals.len());
        self.for_each_place(&mut |place| {
            if !place.projection.is_empty() {
                projected_through.insert(Self::local_index(place.local));
            }
        });
        let mut aliases: Vec<Option<Place>> = vec![None; self.body.locals.len()];
        for (local, data) in self.body.locals.iter() {
            if data.binding.is_some() || !projected_through.contains(Self::local_index(local)) {
                continue;
            }
            let defs = self.defs_of(local);
            let [(point, _)] = defs.as_slice() else {
                continue;
            };
            let Some(stmt) = self.body.blocks[point.block].statements.get(point.index) else {
                continue;
            };
            let StatementKind::Assign {
                rvalue: Rvalue::Use(Operand::Copy(place)),
                ..
            } = &stmt.kind
            else {
                continue;
            };
            aliases[Self::local_index(local)] = Some(place.clone());
        }
        // Resolve chains once, so lookups are one step. A temp copies a
        // place lowered before it, so the chain is finite.
        for index in 0..aliases.len() {
            let Some(mut place) = aliases[index].clone() else {
                continue;
            };
            let mut steps = 0;
            while let Some(base) = &aliases[Self::local_index(place.local)] {
                if steps == aliases.len() {
                    break;
                }
                steps += 1;
                let mut projection = base.projection.clone();
                projection.extend(place.projection);
                place = Place {
                    local: base.local,
                    projection,
                };
            }
            aliases[index] = Some(place);
        }
        aliases
    }

    /// The place a MIR place stands for, through the temp it may be rooted
    /// at.
    fn resolve(&self, place: &Place) -> Place {
        match &self.aliases[Self::local_index(place.local)] {
            Some(base) => {
                let mut projection = base.projection.clone();
                projection.extend(place.projection.iter().cloned());
                Place {
                    local: base.local,
                    projection,
                }
            }
            None => place.clone(),
        }
    }

    /// Whether `local` is a temp standing for a nested place.
    fn is_place_temp(&self, local: LocalId) -> bool {
        self.aliases[Self::local_index(local)].is_some()
    }

    /// Every place the body names, index operands included.
    fn for_each_place(&self, f: &mut impl FnMut(&Place)) {
        for (_, data) in self.body.blocks.iter() {
            for stmt in &data.statements {
                let StatementKind::Assign { dest, rvalue } = &stmt.kind else {
                    continue;
                };
                rvalue_places(rvalue, f);
                place_places(dest, f);
            }
            match &data.terminator.kind {
                TerminatorKind::SwitchBool { discr, .. }
                | TerminatorKind::SwitchVariant { discr, .. } => operand_places(discr, f),
                TerminatorKind::Call { callee, args, .. } => {
                    operand_places(callee, f);
                    for arg in args {
                        operand_places(arg, f);
                    }
                }
                TerminatorKind::Return
                | TerminatorKind::Goto { .. }
                | TerminatorKind::Trap { .. }
                | TerminatorKind::ConstTrap { .. }
                | TerminatorKind::Unreachable => {}
            }
        }
    }

    /// The hir types (regions intact) a local's value may have: a binding's
    /// inferred type, or the type of each expression a temp is defined
    /// from.
    fn hir_types_of(&self, local: LocalId) -> Vec<Ty> {
        if let Some(binding) = self.body.locals[local].binding {
            return self
                .infer
                .type_of_binding
                .get(binding)
                .cloned()
                .into_iter()
                .collect();
        }
        self.defs_of(local)
            .into_iter()
            .filter_map(|(_, origin)| self.infer.type_of_expr.get(origin).cloned())
            .collect()
    }

    fn compute_local_types(&self) -> (Vec<Vec<u32>>, Vec<bool>) {
        let mut regions = Vec::with_capacity(self.body.locals.len());
        let mut copyable = Vec::with_capacity(self.body.locals.len());
        for (local, data) in self.body.locals.iter() {
            let types = self.hir_types_of(local);
            let mut nodes = Vec::new();
            for ty in &types {
                collect_regions(ty, &mut |region| nodes.extend(self.cover.node(region)));
            }
            // The temp holding a fresh loan: its region is the loan's,
            // recorded at the mint (the use site's type may hold what the
            // loan flowed into, or what it came from).
            if data.binding.is_none() {
                for (_, origin) in self.defs_of(local) {
                    if let Some(region) = self.infer.loan_regions.get(origin) {
                        nodes.extend(self.cover.node(region));
                    }
                }
            }
            // A universal is live everywhere whether or not a local
            // mentions it, so a local mentioning one HOLDS nothing: the
            // loans it would keep live are live already.
            nodes.retain(|&node| !self.cover.is_universal(node));
            nodes.sort_unstable();
            nodes.dedup();
            regions.push(nodes);
            copyable.push(
                types
                    .iter()
                    .all(|ty| hir::capability::is_copyable(self.db, ty)),
            );
        }
        (regions, copyable)
    }

    // ---- loans ------------------------------------------------------------

    /// The region of the loan a `Borrow` statement mints: recorded by
    /// inference for every written borrow and inserted reborrow; a match
    /// payload binder's borrow (M13) carries it as the binder's own type.
    fn loan_region(&self, stmt: &Statement) -> Option<Region> {
        if let Some(region) = self.infer.loan_regions.get(stmt.origin) {
            return Some(region.clone());
        }
        let StatementKind::Assign { dest, .. } = &stmt.kind else {
            return None;
        };
        if !dest.projection.is_empty() {
            return None;
        }
        let binding: BindingId = self.body.locals[dest.local].binding?;
        match self.infer.type_of_binding.get(binding)? {
            Ty::Borrow { region, .. } => Some(region.clone()),
            _ => None,
        }
    }

    /// The points lowered from each expression, for locating obligations.
    fn points_by_origin(&self) -> FxHashMap<ExprId, Vec<Point>> {
        let mut points: FxHashMap<ExprId, Vec<Point>> = FxHashMap::default();
        for (block, _) in self.body.blocks.iter() {
            for point in self.points_of(block) {
                points.entry(self.origin_at(point)).or_default().push(point);
            }
        }
        points
    }

    /// The obligations by which a loan with this cover flows into a
    /// universal: `sup ⊇ sub` with `sup` a body region the loan's region
    /// covers and `sub` a universal — the hop INTO the universal. An edge
    /// between two universals adds nothing: a loan that reached the first
    /// is live everywhere already.
    fn flows_into_universal<'c>(
        &'c self,
        covers: &'c [u32],
    ) -> impl Iterator<Item = &'c RegionConstraint> + 'c {
        let nodes = move |region: &Region| -> Vec<u32> {
            match region {
                Region::Join(parts) => parts.iter().filter_map(|p| self.cover.node(p)).collect(),
                region => self.cover.node(region).into_iter().collect(),
            }
        };
        self.infer.region_constraints.iter().filter(move |c| {
            nodes(&c.sup)
                .iter()
                .any(|&sup| !self.cover.is_universal(sup) && covers.binary_search(&sup).is_ok())
                && nodes(&c.sub)
                    .iter()
                    .any(|&sub| self.cover.is_universal(sub))
        })
    }

    fn collect_loans(&self) -> Vec<Loan> {
        let points_by_origin = self.points_by_origin();
        let mut loans = Vec::new();
        for (block, data) in self.body.blocks.iter() {
            for (index, stmt) in data.statements.iter().enumerate() {
                let StatementKind::Assign {
                    rvalue: Rvalue::Borrow { mutable, place },
                    ..
                } = &stmt.kind
                else {
                    continue;
                };
                // A loan whose region nothing recorded cannot be judged
                // live anywhere; inference records one for every borrow
                // the language produces, so this is the broken-program
                // case, and a broken program grows no refusals here.
                let Some(node) = self.loan_region(stmt).and_then(|r| self.cover.node(&r)) else {
                    continue;
                };
                let covers = self.cover.covers(node).to_vec();
                let mut flow_points = Vec::new();
                let mut flows_everywhere = self.cover.is_universal(node);
                for constraint in self.flows_into_universal(&covers) {
                    match points_by_origin.get(&constraint.origin) {
                        Some(points) => flow_points.extend(points.iter().copied()),
                        None => flows_everywhere = true,
                    }
                }
                loans.push(Loan {
                    origin: stmt.origin,
                    mint: Point { block, index },
                    mutable: *mutable,
                    place: self.resolve(place),
                    reaches_universal: covers.iter().any(|&n| self.cover.is_universal(n)),
                    covers,
                    flow_points,
                    flows_everywhere,
                });
            }
        }
        loans
    }

    /// Whether a loan is live at a point: a live local's value mentions a
    /// region it covers, or it has flowed into a universal on the way here.
    fn loan_live(&self, loan: &Loan, point: Point) -> bool {
        loan.flows_everywhere
            || loan
                .flow_points
                .iter()
                .any(|&flow| self.reaches(flow, point))
            || {
                let live = &self.live_regions[Self::block_index(point.block)][point.index];
                loan.covers.iter().any(|&node| live.contains(node as usize))
            }
    }

    fn compute_live_regions(&self) -> Vec<Vec<BitSet>> {
        let nodes = self
            .local_regions
            .iter()
            .flatten()
            .chain(self.loans.iter().flat_map(|loan| &loan.covers))
            .map(|&node| node as usize + 1)
            .max()
            .unwrap_or(0);
        self.body
            .blocks
            .iter()
            .map(|(block, _)| {
                self.points_of(block)
                    .map(|point| {
                        let mut regions = BitSet::new(nodes);
                        for local in self.live_at(point).iter() {
                            for &node in &self.local_regions[local] {
                                regions.insert(node as usize);
                            }
                        }
                        regions
                    })
                    .collect()
            })
            .collect()
    }

    fn compute_live_loans(&self) -> Vec<Vec<BitSet>> {
        self.body
            .blocks
            .iter()
            .map(|(block, _)| {
                self.points_of(block)
                    .map(|point| {
                        let mut loans = BitSet::new(self.loans.len());
                        for (index, loan) in self.loans.iter().enumerate() {
                            if self.loan_live(loan, point) {
                                loans.insert(index);
                            }
                        }
                        loans
                    })
                    .collect()
            })
            .collect()
    }

    /// The locals live at a point whose value keeps the loan's region
    /// live.
    fn holders<'s>(&'s self, loan: &'s Loan, live: &'s BitSet) -> impl Iterator<Item = usize> + 's {
        live.iter().filter(move |&local| {
            let regions = &self.local_regions[local];
            let mut a = regions.iter().peekable();
            let mut b = loan.covers.iter().peekable();
            while let (Some(&x), Some(&y)) = (a.peek(), b.peek()) {
                match x.cmp(y) {
                    std::cmp::Ordering::Less => {
                        a.next();
                    }
                    std::cmp::Ordering::Greater => {
                        b.next();
                    }
                    std::cmp::Ordering::Equal => return true,
                }
            }
            false
        })
    }

    // ---- local liveness ---------------------------------------------------

    /// The locals a point reads. A projected destination reads its root
    /// (the write goes into or through it); a whole-local destination is
    /// a definition, not a use.
    fn uses_at(&self, point: Point) -> Vec<LocalId> {
        let data = &self.body.blocks[point.block];
        let mut uses = Vec::new();
        let place_uses = |place: &Place, uses: &mut Vec<LocalId>| {
            uses.push(place.local);
            for elem in &place.projection {
                if let ProjElem::Index(op) = elem {
                    operand_uses(op, uses);
                }
            }
        };
        match data.statements.get(point.index) {
            Some(stmt) => match &stmt.kind {
                StatementKind::Assign { dest, rvalue } => {
                    rvalue_uses(rvalue, &mut uses);
                    if !dest.projection.is_empty() {
                        place_uses(dest, &mut uses);
                    }
                }
                // Neither a use nor a def: counting it as a use would keep
                // every holder live up to its own storage end.
                StatementKind::StorageDead { .. } => {}
            },
            None => match &data.terminator.kind {
                TerminatorKind::SwitchBool { discr, .. }
                | TerminatorKind::SwitchVariant { discr, .. } => operand_uses(discr, &mut uses),
                TerminatorKind::Call { callee, args, .. } => {
                    operand_uses(callee, &mut uses);
                    for arg in args {
                        operand_uses(arg, &mut uses);
                    }
                }
                TerminatorKind::Return => uses.push(self.body.return_local()),
                TerminatorKind::Goto { .. }
                | TerminatorKind::Trap { .. }
                | TerminatorKind::ConstTrap { .. }
                | TerminatorKind::Unreachable => {}
            },
        }
        uses
    }

    fn def_at(&self, point: Point) -> Option<LocalId> {
        let data = &self.body.blocks[point.block];
        match data.statements.get(point.index) {
            Some(stmt) => match &stmt.kind {
                StatementKind::Assign { dest, .. } => {
                    dest.projection.is_empty().then_some(dest.local)
                }
                StatementKind::StorageDead { .. } => None,
            },
            None => match &data.terminator.kind {
                TerminatorKind::Call { dest, .. } | TerminatorKind::Trap { dest, .. } => {
                    Some(*dest)
                }
                _ => None,
            },
        }
    }

    fn successors(&self, block: BlockId) -> Vec<BlockId> {
        match &self.body.blocks[block].terminator.kind {
            TerminatorKind::Goto { target } => vec![*target],
            TerminatorKind::SwitchBool {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            TerminatorKind::SwitchVariant {
                arms, otherwise, ..
            } => arms
                .iter()
                .map(|(_, target)| *target)
                .chain(std::iter::once(*otherwise))
                .collect(),
            TerminatorKind::Call { target, .. } => target.iter().copied().collect(),
            TerminatorKind::Trap { target, .. } | TerminatorKind::ConstTrap { target, .. } => {
                vec![*target]
            }
            TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
        }
    }

    fn points_of(&self, block: BlockId) -> impl Iterator<Item = Point> + '_ {
        (0..=self.body.blocks[block].statements.len()).map(move |index| Point { block, index })
    }

    /// Backward liveness to a fixpoint, then the per-point sets.
    fn compute_liveness(&self) -> Vec<Vec<BitSet>> {
        let locals = self.body.locals.len();
        let blocks = self.body.blocks.len();
        let mut live_in: Vec<BitSet> = vec![BitSet::new(locals); blocks];
        let transfer = |block: BlockId, live_out: BitSet| -> Vec<BitSet> {
            let mut before: Vec<BitSet> = Vec::new();
            let mut live = live_out;
            for point in self.points_of(block).collect::<Vec<_>>().into_iter().rev() {
                if let Some(def) = self.def_at(point) {
                    live.remove(Self::local_index(def));
                }
                for local in self.uses_at(point) {
                    live.insert(Self::local_index(local));
                }
                before.push(live.clone());
            }
            before.reverse();
            before
        };
        let mut changed = true;
        while changed {
            changed = false;
            for (block, _) in self
                .body
                .blocks
                .iter()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                let mut out = BitSet::new(locals);
                for succ in self.successors(block) {
                    out.union_with(&live_in[Self::block_index(succ)]);
                }
                let before = transfer(block, out);
                let entry = before[0].clone();
                if entry != live_in[Self::block_index(block)] {
                    live_in[Self::block_index(block)] = entry;
                    changed = true;
                }
            }
        }
        self.body
            .blocks
            .iter()
            .map(|(block, _)| {
                let mut out = BitSet::new(locals);
                for succ in self.successors(block) {
                    out.union_with(&live_in[Self::block_index(succ)]);
                }
                transfer(block, out)
            })
            .collect()
    }

    fn live_at(&self, point: Point) -> &BitSet {
        &self.live_before[Self::block_index(point.block)][point.index]
    }

    // ---- accesses ---------------------------------------------------------

    fn operand_access(&self, op: &Operand, out: &mut Vec<Access>) {
        let (place, moved) = match op {
            Operand::Copy(place) => (place, false),
            Operand::Move(place) => (place, true),
            Operand::Const(_) => return,
        };
        self.index_accesses(place, out);
        // A by-name read of a value that cannot be duplicated is a MOVE,
        // whichever operand lowering chose: `Move` is the linear case,
        // and a `&mut` or a rigid parameter reads as `Copy` and is
        // affine all the same.
        let moves = moved
            || (place.projection.is_empty()
                && !self.local_copyable[Self::local_index(place.local)]);
        out.push(Access {
            kind: if moves {
                AccessKind::Move
            } else {
                AccessKind::Read
            },
            place: self.resolve(place),
            shallow: false,
        });
    }

    /// The reads an element index inside a projection performs.
    fn index_accesses(&self, place: &Place, out: &mut Vec<Access>) {
        for elem in &place.projection {
            if let ProjElem::Index(op) = elem {
                self.operand_access(op, out);
            }
        }
    }

    /// Every access a point performs, in evaluation order.
    fn accesses_at(&self, point: Point) -> Vec<Access> {
        let data = &self.body.blocks[point.block];
        let mut out = Vec::new();
        match data.statements.get(point.index) {
            // The last shallow write of the whole local: a loan of its
            // storage conflicts, a loan through it (of the pointee's
            // storage) does not, and `step` retires every loan of it
            // afterwards, so no later use reports the same loan again.
            Some(Statement {
                kind: StatementKind::StorageDead { local },
                ..
            }) => out.push(Access {
                kind: AccessKind::StorageEnd,
                place: (*local).into(),
                shallow: true,
            }),
            Some(Statement {
                kind: StatementKind::Assign { dest, rvalue },
                ..
            }) => {
                match rvalue {
                    // The copy that defines a temp standing for a nested
                    // place is no access (module doc, "Places"): the
                    // access through the temp is the one the user wrote.
                    // Its index operands still read.
                    Rvalue::Use(Operand::Copy(place))
                        if dest.projection.is_empty() && self.is_place_temp(dest.local) =>
                    {
                        self.index_accesses(place, &mut out);
                    }
                    Rvalue::Use(op) | Rvalue::UnaryNeg(op) | Rvalue::WidenToEnum { op, .. } => {
                        self.operand_access(op, &mut out);
                    }
                    Rvalue::BinaryOp(_, a, b) => {
                        self.operand_access(a, &mut out);
                        self.operand_access(b, &mut out);
                    }
                    Rvalue::Aggregate { ops, .. } => {
                        for op in ops {
                            self.operand_access(op, &mut out);
                        }
                    }
                    Rvalue::Field { base, .. } => self.operand_access(base, &mut out),
                    Rvalue::Index { base, index } => {
                        self.operand_access(base, &mut out);
                        self.operand_access(index, &mut out);
                    }
                    Rvalue::Repeat { elem, count } => {
                        self.operand_access(elem, &mut out);
                        self.operand_access(count, &mut out);
                    }
                    Rvalue::Instantiate { const_args, .. } => {
                        for op in const_args {
                            self.operand_access(op, &mut out);
                        }
                    }
                    Rvalue::Borrow { mutable, place } => {
                        self.index_accesses(place, &mut out);
                        out.push(Access {
                            kind: if *mutable {
                                AccessKind::MutBorrow
                            } else {
                                AccessKind::SharedBorrow
                            },
                            place: self.resolve(place),
                            shallow: false,
                        });
                    }
                    // Minting a raw pointer touches nothing (M08): it is
                    // the deref, under `unsafe`, that accesses. Its index
                    // operands still read.
                    Rvalue::AddrOf { place, .. } => self.index_accesses(place, &mut out),
                    Rvalue::AddrOfStatic { .. } => {}
                }
                self.index_accesses(dest, &mut out);
                // The temp a nested place is copied into is written as
                // itself; a write THROUGH such a temp is a write to the
                // place it stands for.
                let place = if dest.projection.is_empty() {
                    dest.clone()
                } else {
                    self.resolve(dest)
                };
                out.push(Access {
                    kind: AccessKind::Write,
                    place,
                    shallow: true,
                });
            }
            None => match &data.terminator.kind {
                TerminatorKind::SwitchBool { discr, .. }
                | TerminatorKind::SwitchVariant { discr, .. } => {
                    self.operand_access(discr, &mut out);
                }
                TerminatorKind::Call {
                    callee, args, dest, ..
                } => {
                    self.operand_access(callee, &mut out);
                    for arg in args {
                        self.operand_access(arg, &mut out);
                    }
                    out.push(Access {
                        kind: AccessKind::Write,
                        place: (*dest).into(),
                        shallow: true,
                    });
                }
                TerminatorKind::Trap { dest, .. } => out.push(Access {
                    kind: AccessKind::Write,
                    place: (*dest).into(),
                    shallow: true,
                }),
                TerminatorKind::Return
                | TerminatorKind::Goto { .. }
                | TerminatorKind::ConstTrap { .. }
                | TerminatorKind::Unreachable => {}
            },
        }
        out
    }

    /// Whether `access` is foreign to `loan` and touches it.
    fn conflicts(&self, access: &Access, loan: &Loan) -> bool {
        if access.place.local != loan.place.local
            || !paths_overlap(&access.place.projection, &loan.place.projection)
        {
            return false;
        }
        if !access.kind.is_write() && !loan.mutable {
            return false;
        }
        // A shallow write into a prefix of the loan's place does not reach
        // through a deref below it: it replaces the pointer, not the
        // pointee.
        if access.shallow && access.place.projection.len() < loan.place.projection.len() {
            let below = &loan.place.projection[access.place.projection.len()..];
            if below.iter().any(|elem| matches!(elem, ProjElem::Deref)) {
                return false;
            }
        }
        true
    }

    /// Whether an assignment to `dest` ends `loan`: the loan is of `dest`
    /// or of something inside or through it, and the old value is gone.
    fn kills(dest: &Place, loan: &Loan) -> bool {
        dest.local == loan.place.local
            && dest.projection.len() <= loan.place.projection.len()
            && paths_overlap(&dest.projection, &loan.place.projection)
    }

    /// One point's effect on the loans in scope, with every access
    /// reported to `on_access` against the loans in scope BEFORE it.
    ///
    /// A loan whose region is not live at the point leaves scope here and
    /// does not come back: the walk forward from the mint ends on this
    /// path. In scope therefore implies live.
    fn step(&self, point: Point, scope: &mut BitSet, mut on_access: impl FnMut(&Access, &BitSet)) {
        scope.intersect_with(&self.live_loans[Self::block_index(point.block)][point.index]);
        for access in self.accesses_at(point) {
            on_access(&access, scope);
            if access.shallow {
                for (index, loan) in self.loans.iter().enumerate() {
                    if Self::kills(&access.place, loan) {
                        scope.remove(index);
                    }
                }
            }
        }
        if let Some(index) = self.loans.iter().position(|loan| loan.mint == point) {
            scope.insert(index);
        }
    }

    /// Forward loans-in-scope to a fixpoint: the set entering each block.
    fn compute_scope(&self) -> Vec<BitSet> {
        let blocks = self.body.blocks.len();
        let mut scope_in: Vec<BitSet> = vec![BitSet::new(self.loans.len()); blocks];
        let mut changed = true;
        while changed {
            changed = false;
            for (block, _) in self.body.blocks.iter() {
                let mut scope = scope_in[Self::block_index(block)].clone();
                for point in self.points_of(block) {
                    self.step(point, &mut scope, |_, _| {});
                }
                for succ in self.successors(block) {
                    let entry = &mut scope_in[Self::block_index(succ)];
                    let mut merged = entry.clone();
                    merged.union_with(&scope);
                    if merged != *entry {
                        *entry = merged;
                        changed = true;
                    }
                }
            }
        }
        scope_in
    }

    // ---- reporting --------------------------------------------------------

    fn run(&self, diagnostics: &mut Vec<LoanDiagnostic>) {
        let scope_in = self.compute_scope();
        let mut escaped: FxHashSet<ExprId> = FxHashSet::default();
        // One report per (access expression, loan): the earliest-minted
        // loan each access kills, and never twice for one origin.
        let mut reported: FxHashSet<(ExprId, usize)> = FxHashSet::default();
        for (block, data) in self.body.blocks.iter() {
            let mut scope = scope_in[Self::block_index(block)].clone();
            for point in self.points_of(block) {
                self.step(point, &mut scope, |access, scope| {
                    // A loan of dying storage that reaches a universal is
                    // `hir::outlives`' escape finding already, as it is
                    // at the return below.
                    let victim = scope.iter().find(|&index| {
                        let loan = &self.loans[index];
                        self.conflicts(access, loan)
                            && !(access.kind == AccessKind::StorageEnd && loan.reaches_universal)
                    });
                    let Some(index) = victim else {
                        return;
                    };
                    let origin = self.origin_at(point);
                    if !reported.insert((origin, index)) {
                        return;
                    }
                    let loan = &self.loans[index];
                    diagnostics.push(LoanDiagnostic::Invalidated {
                        access: origin,
                        kind: access.kind,
                        place: self
                            .render_place(&access.place, self.is_inserted_reborrow(origin, access)),
                        borrow: loan.origin,
                        still_used: self.still_used(point, loan),
                    });
                });
                if matches!(data.terminator.kind, TerminatorKind::Return)
                    && point.index == data.statements.len()
                {
                    // Every loan still in scope here is live at the
                    // return, `step` having dropped the rest.
                    for index in scope.iter() {
                        let loan = &self.loans[index];
                        let of_storage =
                            !matches!(loan.place.projection.first(), Some(ProjElem::Deref));
                        if of_storage && !loan.reaches_universal && escaped.insert(loan.origin) {
                            diagnostics.push(LoanDiagnostic::Escapes {
                                borrow: loan.origin,
                                temporary: self.is_temporary(loan.place.local),
                            });
                        }
                    }
                }
            }
        }
    }

    fn origin_at(&self, point: Point) -> ExprId {
        let data = &self.body.blocks[point.block];
        match data.statements.get(point.index) {
            Some(stmt) => stmt.origin,
            None => data.terminator.origin,
        }
    }

    /// Why the loan is live at the access: handed back, when a flow into
    /// a universal reaches the access — always true then, where a holder's
    /// use may name another loan the holder's region also covers — or
    /// else where a holder is used next, from the access onward.
    fn still_used(&self, from: Point, loan: &Loan) -> StillUsed {
        if loan.flows_everywhere
            || loan
                .flow_points
                .iter()
                .any(|&flow| self.reaches(flow, from))
        {
            return StillUsed::HandedBack;
        }
        let live = self.live_at(from);
        let holders: FxHashSet<usize> = self.holders(loan, live).collect();
        if holders.is_empty() {
            return StillUsed::HandedBack;
        }
        let uses_holder = |point: Point| {
            self.uses_at(point)
                .iter()
                .any(|local| holders.contains(&Self::local_index(*local)))
        };
        if uses_holder(from) {
            return StillUsed::AtTheAccess;
        }
        // Breadth-first over points, so the nearest use is the one named.
        let mut visited: FxHashSet<BlockId> = FxHashSet::default();
        let mut queue: std::collections::VecDeque<Point> = std::collections::VecDeque::new();
        let enqueue_rest = |start: Point, queue: &mut std::collections::VecDeque<Point>| {
            let len = self.body.blocks[start.block].statements.len();
            for index in start.index..=len {
                queue.push_back(Point {
                    block: start.block,
                    index,
                });
            }
        };
        let mut enqueue_block = |block: BlockId, queue: &mut std::collections::VecDeque<Point>| {
            if visited.insert(block) {
                enqueue_rest(Point { block, index: 0 }, queue);
            }
        };
        if from.index < self.body.blocks[from.block].statements.len() {
            enqueue_rest(
                Point {
                    block: from.block,
                    index: from.index + 1,
                },
                &mut queue,
            );
        } else {
            for succ in self.successors(from.block) {
                enqueue_block(succ, &mut queue);
            }
        }
        while let Some(point) = queue.pop_front() {
            if uses_holder(point) {
                let at = self.origin_at(point);
                let access = self.origin_at(from);
                return if u32::from(at.into_raw()) < u32::from(access.into_raw()) {
                    StillUsed::NextIteration(at)
                } else {
                    StillUsed::Later(at)
                };
            }
            if point.index == self.body.blocks[point.block].statements.len() {
                for succ in self.successors(point.block) {
                    enqueue_block(succ, &mut queue);
                }
            }
        }
        StillUsed::HandedBack
    }

    /// Whether an access is a reborrow lowering inserted for a mention of
    /// a borrow-typed place (`bump(r)`, `r.refill()`): a mint whose origin
    /// is not a written borrow. Its place ends in the deref the user did
    /// not write.
    fn is_inserted_reborrow(&self, origin: ExprId, access: &Access) -> bool {
        matches!(
            access.kind,
            AccessKind::MutBorrow | AccessKind::SharedBorrow
        ) && !matches!(
            self.hir_body.exprs[origin],
            hir::body::ExprData::Borrow { .. }
        ) && matches!(access.place.projection.last(), Some(ProjElem::Deref))
    }

    /// Whether a local is a materialized temporary (M12).
    fn is_temporary(&self, local: LocalId) -> bool {
        self.body.locals[local]
            .binding
            .is_some_and(|binding| self.hir_body.bindings[binding].is_temporary())
    }

    /// The place as the user wrote it, quoted for insertion into a
    /// sentence: `` `n` ``, `` `p.x` ``, `` `r.*.v` ``, `` `bb.*.*` ``,
    /// `` `arr[_]` ``. For an inserted reborrow the trailing deref is
    /// dropped, so `bump(r)` names `r`. A materialized temporary has no
    /// name to quote and renders as "this temporary".
    fn render_place(&self, place: &Place, inserted_reborrow: bool) -> String {
        if self.is_temporary(place.local) {
            return "this temporary".to_owned();
        }
        let data = &self.body.locals[place.local];
        let mut out = format!("`{}", data.name.clone().unwrap_or_default());
        let mut ty = Some(data.ty.clone());
        let shown = place.projection.len() - usize::from(inserted_reborrow);
        for elem in &place.projection[..shown] {
            match elem {
                ProjElem::Field(index) => {
                    let record = ty.take().and_then(|ty| self.record_of(&ty));
                    let name = record
                        .as_ref()
                        .and_then(|record| record.fields.get(*index as usize))
                        .map(|(name, _)| name.clone());
                    ty = record.and_then(|record| {
                        record.fields.get(*index as usize).map(|(_, ty)| ty.clone())
                    });
                    match name {
                        Some(name) => {
                            out.push('.');
                            out.push_str(&name);
                        }
                        None => out.push_str(&format!(".{index}")),
                    }
                }
                ProjElem::Index(_) => {
                    ty = ty.take().and_then(|ty| match ty {
                        Ty::Array { elem, .. } => Some((*elem).clone()),
                        _ => None,
                    });
                    out.push_str("[_]");
                }
                ProjElem::Deref => {
                    ty = ty.take().and_then(|ty| match ty {
                        Ty::Borrow { referent, .. } => Some((*referent).clone()),
                        Ty::RawPtr { pointee, .. } => Some((*pointee).clone()),
                        _ => None,
                    });
                    out.push_str(".*");
                }
            }
        }
        out.push('`');
        out
    }

    fn record_of(&self, ty: &Ty) -> Option<std::sync::Arc<hir::ty::RecordTy>> {
        match ty {
            Ty::Record(record) => Some(record.clone()),
            Ty::Named(named) => self.record_of(&hir::ty::type_underlying_for(self.db, named)?),
            _ => None,
        }
    }
}

fn operand_uses(op: &Operand, uses: &mut Vec<LocalId>) {
    operand_places(op, &mut |place| uses.push(place.local));
}

fn rvalue_uses(rvalue: &Rvalue, uses: &mut Vec<LocalId>) {
    rvalue_places(rvalue, &mut |place| uses.push(place.local));
}

/// A place and the places its index operands name.
fn place_places(place: &Place, f: &mut impl FnMut(&Place)) {
    f(place);
    for elem in &place.projection {
        if let ProjElem::Index(op) = elem {
            operand_places(op, f);
        }
    }
}

fn operand_places(op: &Operand, f: &mut impl FnMut(&Place)) {
    match op {
        Operand::Copy(place) | Operand::Move(place) => place_places(place, f),
        Operand::Const(_) => {}
    }
}

fn rvalue_places(rvalue: &Rvalue, f: &mut impl FnMut(&Place)) {
    match rvalue {
        Rvalue::Use(op) | Rvalue::UnaryNeg(op) | Rvalue::WidenToEnum { op, .. } => {
            operand_places(op, f)
        }
        Rvalue::BinaryOp(_, a, b) => {
            operand_places(a, f);
            operand_places(b, f);
        }
        Rvalue::Aggregate { ops, .. }
        | Rvalue::Instantiate {
            const_args: ops, ..
        } => {
            for op in ops {
                operand_places(op, f);
            }
        }
        Rvalue::Field { base, .. } => operand_places(base, f),
        Rvalue::Index { base, index } => {
            operand_places(base, f);
            operand_places(index, f);
        }
        Rvalue::Repeat { elem, count } => {
            operand_places(elem, f);
            operand_places(count, f);
        }
        Rvalue::AddrOf { place, .. } | Rvalue::Borrow { place, .. } => place_places(place, f),
        Rvalue::AddrOfStatic { projection, .. } => {
            for elem in projection {
                if let ProjElem::Index(op) = elem {
                    operand_places(op, f);
                }
            }
        }
    }
}

/// Every region a type mentions in a value position. A `fn` type's
/// regions are its own binder's and bind nothing here.
fn collect_regions(ty: &Ty, found: &mut impl FnMut(&Region)) {
    fn one(region: &Region, found: &mut impl FnMut(&Region)) {
        match region {
            Region::Join(parts) => parts.iter().for_each(|part| one(part, found)),
            _ => found(region),
        }
    }
    fn args(args: &[GenericArg], found: &mut impl FnMut(&Region)) {
        for arg in args {
            match arg {
                GenericArg::Ty(ty) => collect_regions(ty, found),
                GenericArg::Region(region) => one(region, found),
                _ => {}
            }
        }
    }
    match ty {
        Ty::Borrow {
            region, referent, ..
        } => {
            one(region, found);
            collect_regions(referent, found);
        }
        Ty::RawPtr { pointee, .. } => collect_regions(pointee, found),
        Ty::Record(record) => record
            .fields
            .iter()
            .for_each(|(_, field)| collect_regions(field, found)),
        Ty::Array { elem, .. } => collect_regions(elem, found),
        Ty::Named(named) => args(&named.args, found),
        Ty::Variant(variant) => args(&variant.args, found),
        Ty::Fn(_)
        | Ty::Param(_)
        | Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => {}
    }
}
